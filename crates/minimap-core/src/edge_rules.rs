//! The edge-type matrix (CLAUDE.md section 4.2): which edge may connect which node
//! types, and which attributes it may carry.

use minimap_types::{EdgeType, NodeRef, NodeType};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EdgeRuleError {
    #[error("a {edge_type} link cannot go from a {from} to a {to}")]
    NotAllowed {
        edge_type: EdgeType,
        from: NodeType,
        to: NodeType,
    },
    #[error("a node cannot be linked to itself")]
    SelfEdge,
    #[error("invalid link attribute: {0}")]
    BadAttr(String),
}

/// Edge types that must stay acyclic.
pub fn must_be_acyclic(edge_type: EdgeType) -> bool {
    matches!(
        edge_type,
        EdgeType::Blocks | EdgeType::DependsOn | EdgeType::ReportsTo
    )
}

/// Whether the (type, from, to) combination is in the matrix.
pub fn is_allowed(edge_type: EdgeType, from: NodeType, to: NodeType) -> bool {
    use EdgeType as E;
    use NodeType as N;
    match edge_type {
        E::Blocks => (from, to) == (N::Task, N::Task),
        E::DependsOn => (from, to) == (N::Project, N::Project),
        E::ContributesTo => matches!(from, N::Project | N::Task) && to == N::Objective,
        E::AssignedTo => (from, to) == (N::Task, N::Person),
        E::MemberOf => (from, to) == (N::Person, N::Team),
        E::ReportsTo => (from, to) == (N::Person, N::Person),
        E::RelatesTo => true,
        E::Mentions => from == N::Note,
        E::Affects => from == N::Decision && matches!(to, N::Project | N::Task | N::Objective),
        E::About => from == N::WaitingOn && matches!(to, N::Task | N::Project),
    }
}

/// Validates type combination, self-links and attributes (not cycles; see `cycles`).
pub fn validate(
    edge_type: EdgeType,
    from: NodeRef,
    to: NodeRef,
    attrs: &Value,
) -> Result<(), EdgeRuleError> {
    if from.id == to.id {
        return Err(EdgeRuleError::SelfEdge);
    }
    validate_types(edge_type, from.node_type, to.node_type)?;
    validate_attrs(edge_type, attrs)
}

pub fn validate_types(
    edge_type: EdgeType,
    from: NodeType,
    to: NodeType,
) -> Result<(), EdgeRuleError> {
    if is_allowed(edge_type, from, to) {
        Ok(())
    } else {
        Err(EdgeRuleError::NotAllowed {
            edge_type,
            from,
            to,
        })
    }
}

fn bad(msg: impl Into<String>) -> EdgeRuleError {
    EdgeRuleError::BadAttr(msg.into())
}

/// Attribute names and ranges per edge type. All attributes are optional.
pub fn validate_attrs(edge_type: EdgeType, attrs: &Value) -> Result<(), EdgeRuleError> {
    let Some(map) = attrs.as_object() else {
        return Err(bad("attributes must be an object"));
    };
    let allowed: &[&str] = match edge_type {
        EdgeType::Blocks => &["lag_days"],
        EdgeType::DependsOn | EdgeType::RelatesTo => &["note"],
        EdgeType::ContributesTo => &["weight"],
        EdgeType::AssignedTo => &["allocation_pct"],
        EdgeType::MemberOf => &["role"],
        EdgeType::ReportsTo | EdgeType::Mentions | EdgeType::Affects | EdgeType::About => &[],
    };
    for (key, value) in map {
        if !allowed.contains(&key.as_str()) {
            return Err(bad(format!("{edge_type} links have no '{key}' attribute")));
        }
        match key.as_str() {
            "lag_days" => match value.as_u64() {
                Some(_) => {}
                None => return Err(bad("lag_days must be a whole number of days, 0 or more")),
            },
            "weight" => match value.as_f64() {
                Some(w) if (0.0..=1.0).contains(&w) => {}
                _ => return Err(bad("weight must be between 0 and 1")),
            },
            "allocation_pct" => match value.as_u64() {
                Some(p) if (1..=100).contains(&p) => {}
                _ => return Err(bad("allocation_pct must be a whole number from 1 to 100")),
            },
            "role" => match value.as_str() {
                Some("lead" | "member") => {}
                _ => return Err(bad("role must be 'lead' or 'member'")),
            },
            "note" if !value.is_string() => return Err(bad("note must be text")),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    use EdgeType as E;
    use NodeType as N;

    #[test]
    fn matrix_matches_spec() {
        let allowed = [
            (E::Blocks, N::Task, N::Task),
            (E::DependsOn, N::Project, N::Project),
            (E::ContributesTo, N::Project, N::Objective),
            (E::ContributesTo, N::Task, N::Objective),
            (E::AssignedTo, N::Task, N::Person),
            (E::MemberOf, N::Person, N::Team),
            (E::ReportsTo, N::Person, N::Person),
            (E::Mentions, N::Note, N::Person),
            (E::Affects, N::Decision, N::Project),
            (E::Affects, N::Decision, N::Task),
            (E::Affects, N::Decision, N::Objective),
            (E::About, N::WaitingOn, N::Task),
            (E::About, N::WaitingOn, N::Project),
        ];
        for &(e, f, t) in &allowed {
            assert!(is_allowed(e, f, t), "{e} {f}->{t}");
        }
        // Exhaustively: everything not listed (and not relates_to / mentions-any) is rejected.
        for &e in EdgeType::ALL {
            for &f in NodeType::ALL {
                for &t in NodeType::ALL {
                    let expected = e == E::RelatesTo
                        || (e == E::Mentions && f == N::Note)
                        || allowed.contains(&(e, f, t));
                    assert_eq!(is_allowed(e, f, t), expected, "{e} {f}->{t}");
                }
            }
        }
    }

    #[test]
    fn rejects_wrong_types_with_a_readable_message() {
        let err = validate_types(E::Blocks, N::Task, N::Person).unwrap_err();
        assert_eq!(
            err.to_string(),
            "a blocks link cannot go from a task to a person"
        );
    }

    #[test]
    fn attrs_are_checked() {
        assert!(validate_attrs(E::Blocks, &json!({})).is_ok());
        assert!(validate_attrs(E::Blocks, &json!({"lag_days": 2})).is_ok());
        assert!(validate_attrs(E::Blocks, &json!({"lag_days": -1})).is_err());
        assert!(validate_attrs(E::Blocks, &json!({"lag_days": 1.5})).is_err());
        assert!(validate_attrs(E::Blocks, &json!({"weight": 1})).is_err());
        assert!(validate_attrs(E::ContributesTo, &json!({"weight": 0.5})).is_ok());
        assert!(validate_attrs(E::ContributesTo, &json!({"weight": 1.5})).is_err());
        assert!(validate_attrs(E::AssignedTo, &json!({"allocation_pct": 100})).is_ok());
        assert!(validate_attrs(E::AssignedTo, &json!({"allocation_pct": 0})).is_err());
        assert!(validate_attrs(E::AssignedTo, &json!({"allocation_pct": 101})).is_err());
        assert!(validate_attrs(E::MemberOf, &json!({"role": "lead"})).is_ok());
        assert!(validate_attrs(E::MemberOf, &json!({"role": "boss"})).is_err());
        assert!(validate_attrs(E::ReportsTo, &json!({"note": "x"})).is_err());
        assert!(validate_attrs(E::RelatesTo, &json!({"note": "x"})).is_ok());
        assert!(validate_attrs(E::RelatesTo, &json!([])).is_err());
    }

    #[test]
    fn validate_rejects_self_links_first() {
        let t = NodeRef::new(N::Task, minimap_types::Uuid::from_u128(1));
        assert_eq!(
            validate(E::Blocks, t, t, &json!({})),
            Err(EdgeRuleError::SelfEdge)
        );
        let u = NodeRef::new(N::Task, minimap_types::Uuid::from_u128(2));
        assert!(validate(E::Blocks, t, u, &json!({})).is_ok());
    }

    #[test]
    fn acyclic_types() {
        assert!(must_be_acyclic(E::Blocks));
        assert!(must_be_acyclic(E::DependsOn));
        assert!(must_be_acyclic(E::ReportsTo));
        assert!(!must_be_acyclic(E::RelatesTo));
    }
}
