//! Undo (spec 25): applies the inverse steps `minimap_core::undo` worked out. All or nothing:
//! one transaction, and every step first checks that things are still as the step expects, so a
//! change made since (by another action, or merged from another computer) is never overwritten.
//!
//! The steps use the ordinary repository functions, so undoing is logged like any other write
//! (an archive writes `archived`, a restore `unarchived`, and so on). That is also what makes
//! redo free: the rows an undo wrote are turned into the steps that redo it.

use minimap_core::{cycles::find_cycle, edge_rules, undo::Inverse};
use minimap_types::{
    Activity, EdgeType, NodeRef, NodeType, UpdateDecision, UpdateObjective, UpdatePerson,
    UpdateProject, UpdateTask, UpdateTeam, UpdateWaitingOn,
};
use rusqlite::{Connection, Transaction};
use serde::de::DeserializeOwned;
use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::{
    activity,
    convert::now,
    decisions, edges,
    error::{Result, StoreError},
    nodes, objectives, people, projects, tasks, teams, waiting_on,
};

fn invalid(message: impl Into<String>) -> StoreError {
    StoreError::Invalid(message.into())
}

/// Applies `steps` in order and returns the activity rows they wrote (for the redo step).
pub fn apply(conn: &mut Connection, steps: &[Inverse]) -> Result<Vec<Activity>> {
    let tx = conn.transaction()?;
    let marker = activity::latest_rowid(&tx)?;
    for step in steps {
        apply_one(&tx, step)?;
    }
    let written = activity::since(&tx, marker)?;
    tx.commit()?;
    Ok(written)
}

fn name_of(tx: &Transaction, node: NodeRef) -> String {
    nodes::summary(tx, node).map_or_else(
        |_| format!("the {}", node.node_type.as_str().replace('_', " ")),
        |s| format!("\"{}\"", s.label),
    )
}

fn apply_one(tx: &Transaction, step: &Inverse) -> Result<()> {
    match step {
        Inverse::Archive(node) => {
            let summary = nodes::summary(tx, *node)?;
            if summary.archived {
                return Err(invalid(format!(
                    "\"{}\" is already archived",
                    summary.label
                )));
            }
            nodes::archive_in_tx(tx, *node)
        }
        Inverse::Unarchive(node) => {
            let summary = nodes::summary(tx, *node)?;
            if !summary.archived {
                return Err(invalid(format!(
                    "\"{}\" is no longer archived",
                    summary.label
                )));
            }
            nodes::unarchive_in_tx(tx, *node)
        }
        Inverse::Restore { node, old, new } => restore(tx, *node, old, new),
        Inverse::RemoveEdge { edge_id } => {
            let edge = edges::get(tx, *edge_id)?;
            if edge.archived_at.is_some() {
                return Err(invalid("that link was already removed"));
            }
            edges::archive_in_tx(tx, &edge, now())
        }
        Inverse::AddEdge {
            edge_type,
            from,
            to,
            attrs,
        } => add_edge(tx, *edge_type, *from, *to, attrs),
    }
}

/// Puts a removed link back, checking the rules again (the picture may have changed).
fn add_edge(
    tx: &Transaction,
    edge_type: EdgeType,
    from: NodeRef,
    to: NodeRef,
    attrs: &Value,
) -> Result<()> {
    let attrs = if attrs.is_null() {
        json!({})
    } else {
        attrs.clone()
    };
    edge_rules::validate(edge_type, from, to, &attrs).map_err(|e| invalid(e.to_string()))?;
    if edge_rules::must_be_acyclic(edge_type) {
        let existing: Vec<(Uuid, Uuid)> = edges::list_active_of_type(tx, edge_type)?
            .iter()
            .map(|e| (e.from_id, e.to_id))
            .collect();
        if find_cycle(&existing, from.id, to.id).is_some() {
            return Err(invalid(format!(
                "putting the link from {} to {} back would create a loop",
                name_of(tx, from),
                name_of(tx, to)
            )));
        }
    }
    match edges::add_in_tx(
        tx,
        minimap_types::NewEdge {
            edge_type,
            from,
            to,
            attrs,
        },
    ) {
        Err(StoreError::DuplicateEdge) => Err(invalid(format!(
            "{} and {} are already linked",
            name_of(tx, from),
            name_of(tx, to)
        ))),
        other => other.map(|_| ()),
    }
}

/// The node as the fields the activity log uses (`{field: value}`).
fn current_fields(tx: &Transaction, node: NodeRef) -> Result<Map<String, Value>> {
    let value = match node.node_type {
        NodeType::Objective => serde_json::to_value(objectives::get(tx, node.id)?)?,
        NodeType::Project => serde_json::to_value(projects::get(tx, node.id)?)?,
        NodeType::Task => serde_json::to_value(tasks::get(tx, node.id)?)?,
        NodeType::Person => serde_json::to_value(people::get(tx, node.id)?)?,
        NodeType::Team => serde_json::to_value(teams::get(tx, node.id)?)?,
        NodeType::Note => return Err(invalid("a note's edits aren't part of undo")),
        NodeType::Decision => serde_json::to_value(decisions::get(tx, node.id)?)?,
        NodeType::WaitingOn => serde_json::to_value(waiting_on::get(tx, node.id)?)?,
    };
    match value {
        Value::Object(map) => Ok(map),
        _ => Err(invalid("couldn't read the current values")),
    }
}

/// `Update*` from `{field: value}`: a nullable field (`Patch<T>`) is `{"set": v}`, or `"clear"`
/// for null; the rest are plain.
fn patch_from<T: DeserializeOwned>(old: &Map<String, Value>, nullable: &[&str]) -> Result<T> {
    let mut fields = Map::new();
    for (key, value) in old {
        let value = if nullable.contains(&key.as_str()) {
            if value.is_null() {
                json!("clear")
            } else {
                json!({ "set": value })
            }
        } else {
            value.clone()
        };
        fields.insert(key.clone(), value);
    }
    serde_json::from_value(Value::Object(fields)).map_err(StoreError::from)
}

/// The nullable (`Patch`) fields of each node type's update.
pub(crate) fn nullable_fields(node_type: NodeType) -> &'static [&'static str] {
    match node_type {
        NodeType::Objective => &["target_date", "review_every_days", "last_reviewed_on"],
        NodeType::Project => &["owner_person_id", "start_date", "target_date"],
        NodeType::Task => &[
            "project_id",
            "estimate_days",
            "start_date",
            "due_date",
            "recurrence",
            "task_type",
        ],
        NodeType::Person => &["email"],
        NodeType::Team => &["parent_team_id"],
        NodeType::Note => &["recurrence"],
        NodeType::Decision => &["decided_on"],
        NodeType::WaitingOn => &["expected_by", "follow_up_on", "resolved_on"],
    }
}

fn restore(
    tx: &Transaction,
    node: NodeRef,
    old: &Map<String, Value>,
    new: &Map<String, Value>,
) -> Result<()> {
    let current = current_fields(tx, node)?;
    for (field, expected) in new {
        if current.get(field).unwrap_or(&Value::Null) != expected {
            return Err(invalid(format!(
                "{}'s {} was changed since",
                name_of(tx, node),
                field.replace('_', " ")
            )));
        }
    }
    let nullable = nullable_fields(node.node_type);
    let id = node.id;
    match node.node_type {
        NodeType::Objective => {
            objectives::update_in_tx(tx, id, patch_from::<UpdateObjective>(old, nullable)?)?;
        }
        NodeType::Project => {
            projects::update_in_tx(tx, id, patch_from::<UpdateProject>(old, nullable)?)?;
        }
        NodeType::Task => {
            tasks::update_in_tx(tx, id, patch_from::<UpdateTask>(old, nullable)?)?;
        }
        NodeType::Person => {
            people::update_in_tx(tx, id, patch_from::<UpdatePerson>(old, nullable)?)?;
        }
        NodeType::Team => {
            teams::update_in_tx(tx, id, patch_from::<UpdateTeam>(old, nullable)?)?;
        }
        NodeType::Decision => {
            decisions::update_in_tx(tx, id, patch_from::<UpdateDecision>(old, nullable)?)?;
        }
        NodeType::WaitingOn => {
            waiting_on::update_in_tx(tx, id, patch_from::<UpdateWaitingOn>(old, nullable)?)?;
        }
        NodeType::Note => return Err(invalid("a note's edits aren't part of undo")),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{
        UpdateDecision, UpdateObjective, UpdatePerson, UpdateProject, UpdateTask, UpdateTeam,
        UpdateWaitingOn,
    };
    use std::collections::BTreeSet;

    /// The fields an `Update*` marks as nullable are the ones that serialize as `"keep"` when
    /// left alone: this keeps `nullable_fields` honest when a field is added.
    fn patch_fields<T: serde::Serialize + Default>() -> BTreeSet<String> {
        serde_json::to_value(T::default())
            .unwrap()
            .as_object()
            .unwrap()
            .iter()
            .filter(|(_, v)| *v == "keep")
            .map(|(k, _)| k.clone())
            .collect()
    }

    #[test]
    fn the_nullable_fields_match_the_update_types() {
        let listed = |t: NodeType| -> BTreeSet<String> {
            nullable_fields(t).iter().map(|s| (*s).to_owned()).collect()
        };
        assert_eq!(
            listed(NodeType::Objective),
            patch_fields::<UpdateObjective>()
        );
        assert_eq!(listed(NodeType::Project), patch_fields::<UpdateProject>());
        assert_eq!(listed(NodeType::Task), patch_fields::<UpdateTask>());
        assert_eq!(listed(NodeType::Person), patch_fields::<UpdatePerson>());
        assert_eq!(listed(NodeType::Team), patch_fields::<UpdateTeam>());
        assert_eq!(
            listed(NodeType::Note),
            patch_fields::<minimap_types::UpdateNote>()
        );
        assert_eq!(listed(NodeType::Decision), patch_fields::<UpdateDecision>());
        assert_eq!(
            listed(NodeType::WaitingOn),
            patch_fields::<UpdateWaitingOn>()
        );
    }
}
