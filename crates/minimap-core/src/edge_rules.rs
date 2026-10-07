//! The edge-type matrix (CLAUDE.md section 4.2): which edge may connect which node
//! types, and which attributes it may carry.

use minimap_types::{AttrKind, AttrSpec, EdgeType, LinkOption, NodeRef, NodeType};
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
        EdgeType::Blocks
            | EdgeType::DependsOn
            | EdgeType::ReportsTo
            | EdgeType::Supersedes
            | EdgeType::SubtaskOf
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
        E::Supersedes => (from, to) == (N::Decision, N::Decision),
        E::SubtaskOf => (from, to) == (N::Task, N::Task),
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

fn spec(key: &str, label: &str, kind: AttrKind, hint: &str) -> AttrSpec {
    AttrSpec {
        key: key.into(),
        label: label.into(),
        kind,
        hint: hint.into(),
    }
}

/// The attributes each edge type may carry. All are optional; anything else is rejected.
pub fn attr_schema(edge_type: EdgeType) -> Vec<AttrSpec> {
    let note = || spec("note", "Note", AttrKind::Text, "text");
    match edge_type {
        EdgeType::Blocks => vec![spec(
            "lag_days",
            "Lag (days)",
            AttrKind::WholeNumber { min: 0, max: None },
            "a whole number of days, 0 or more",
        )],
        EdgeType::DependsOn | EdgeType::RelatesTo => vec![note()],
        EdgeType::ContributesTo => vec![spec(
            "weight",
            "Weight",
            AttrKind::Number { min: 0.0, max: 1.0 },
            "between 0 and 1",
        )],
        EdgeType::AssignedTo => vec![spec(
            "allocation_pct",
            "Allocation %",
            AttrKind::WholeNumber {
                min: 1,
                max: Some(100),
            },
            "a whole number from 1 to 100",
        )],
        EdgeType::MemberOf => vec![spec(
            "role",
            "Role",
            AttrKind::Choice {
                options: vec!["lead".into(), "member".into()],
            },
            "'lead' or 'member'",
        )],
        EdgeType::ReportsTo
        | EdgeType::Mentions
        | EdgeType::Affects
        | EdgeType::About
        | EdgeType::Supersedes
        | EdgeType::SubtaskOf => vec![],
    }
}

fn value_fits(kind: &AttrKind, value: &Value) -> bool {
    match kind {
        AttrKind::WholeNumber { min, max } => value
            .as_u64()
            .is_some_and(|n| n >= u64::from(*min) && max.is_none_or(|m| n <= u64::from(m))),
        AttrKind::Number { min, max } => value.as_f64().is_some_and(|n| (*min..=*max).contains(&n)),
        AttrKind::Choice { options } => value
            .as_str()
            .is_some_and(|s| options.iter().any(|o| o == s)),
        AttrKind::Text => value.is_string(),
    }
}

/// Checks attribute names and values against [`attr_schema`].
pub fn validate_attrs(edge_type: EdgeType, attrs: &Value) -> Result<(), EdgeRuleError> {
    let Some(map) = attrs.as_object() else {
        return Err(bad("attributes must be an object"));
    };
    let schema = attr_schema(edge_type);
    for (key, value) in map {
        let Some(spec) = schema.iter().find(|s| &s.key == key) else {
            return Err(bad(format!("{edge_type} links have no '{key}' attribute")));
        };
        if !value_fits(&spec.kind, value) {
            return Err(bad(format!("{key} must be {}", spec.hint)));
        }
    }
    Ok(())
}

/// Every relation a user can add from a node of `node` type: both directions, with the node
/// types allowed on the other end. `relates_to` is symmetric, so it is offered once.
pub fn link_options(node: NodeType) -> Vec<LinkOption> {
    let mut out = Vec::new();
    for &edge_type in EdgeType::ALL {
        let others = |outgoing: bool| -> Vec<NodeType> {
            NodeType::ALL
                .iter()
                .copied()
                .filter(|&o| {
                    if outgoing {
                        is_allowed(edge_type, node, o)
                    } else {
                        is_allowed(edge_type, o, node)
                    }
                })
                .collect()
        };
        for outgoing in [true, false] {
            if !outgoing && edge_type == EdgeType::RelatesTo {
                continue;
            }
            let others = others(outgoing);
            if !others.is_empty() {
                out.push(LinkOption {
                    edge_type,
                    outgoing,
                    others,
                    attrs: attr_schema(edge_type),
                });
            }
        }
    }
    out
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
            (E::Supersedes, N::Decision, N::Decision),
            (E::SubtaskOf, N::Task, N::Task),
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
        assert!(must_be_acyclic(E::Supersedes));
        assert!(must_be_acyclic(E::SubtaskOf));
        assert!(!must_be_acyclic(E::RelatesTo));
    }

    #[test]
    fn schema_matches_the_matrix_documentation() {
        let keys = |e| {
            attr_schema(e)
                .into_iter()
                .map(|s| s.key)
                .collect::<Vec<_>>()
        };
        assert_eq!(keys(E::Blocks), ["lag_days"]);
        assert_eq!(keys(E::ContributesTo), ["weight"]);
        assert_eq!(keys(E::AssignedTo), ["allocation_pct"]);
        assert_eq!(keys(E::MemberOf), ["role"]);
        assert_eq!(keys(E::DependsOn), ["note"]);
        assert_eq!(keys(E::RelatesTo), ["note"]);
        for e in [
            E::ReportsTo,
            E::Mentions,
            E::Affects,
            E::About,
            E::Supersedes,
            E::SubtaskOf,
        ] {
            assert!(keys(e).is_empty(), "{e}");
        }
        // Boundaries come from the schema.
        assert!(validate_attrs(E::AssignedTo, &json!({"allocation_pct": 1})).is_ok());
        assert!(validate_attrs(E::AssignedTo, &json!({"allocation_pct": 100})).is_ok());
        assert!(validate_attrs(E::ContributesTo, &json!({"weight": 0})).is_ok());
        assert!(validate_attrs(E::ContributesTo, &json!({"weight": 1})).is_ok());
        let err = validate_attrs(E::ContributesTo, &json!({"weight": 2})).unwrap_err();
        assert_eq!(
            err.to_string(),
            "invalid link attribute: weight must be between 0 and 1"
        );
    }

    #[test]
    fn link_options_are_exactly_what_the_matrix_allows() {
        // Every offered (direction, other type) is allowed, and every allowed pairing is offered.
        for &node in NodeType::ALL {
            let options = link_options(node);
            for &e in EdgeType::ALL {
                for &other in NodeType::ALL {
                    let out_allowed = is_allowed(e, node, other);
                    let in_allowed = is_allowed(e, other, node) && e != E::RelatesTo;
                    let offered = |outgoing| {
                        options.iter().any(|o| {
                            o.edge_type == e && o.outgoing == outgoing && o.others.contains(&other)
                        })
                    };
                    assert_eq!(offered(true), out_allowed, "{node} {e} -> {other}");
                    assert_eq!(offered(false), in_allowed, "{other} {e} -> {node}");
                }
            }
        }
    }

    #[test]
    fn link_options_for_common_nodes() {
        let find = |node, e, out| {
            link_options(node)
                .into_iter()
                .find(|o| o.edge_type == e && o.outgoing == out)
        };
        let blocks = find(N::Task, E::Blocks, true).unwrap();
        assert_eq!(blocks.others, vec![N::Task]);
        assert_eq!(blocks.attrs[0].key, "lag_days");
        assert!(find(N::Task, E::Blocks, false).is_some(), "blocked by");
        assert_eq!(
            find(N::Task, E::AssignedTo, true).unwrap().others,
            vec![N::Person]
        );
        assert_eq!(
            find(N::Person, E::AssignedTo, false).unwrap().others,
            vec![N::Task]
        );
        assert!(find(N::Project, E::ContributesTo, true).is_some());
        assert!(find(N::Task, E::ContributesTo, true).is_some());
        assert!(find(N::Person, E::ContributesTo, true).is_none());
        // `relates_to` is symmetric: offered once, to anything.
        let rel = find(N::Decision, E::RelatesTo, true).unwrap();
        assert_eq!(rel.others.len(), NodeType::ALL.len());
        assert!(find(N::Decision, E::RelatesTo, false).is_none());
        // Only notes mention; everything can be mentioned.
        assert!(find(N::Note, E::Mentions, true).is_some());
        assert!(find(N::Task, E::Mentions, true).is_none());
        assert_eq!(
            find(N::Task, E::Mentions, false).unwrap().others,
            vec![N::Note]
        );
    }
}
