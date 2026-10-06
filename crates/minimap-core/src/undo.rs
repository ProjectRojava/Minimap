//! Undo (spec 25): from what a command wrote to the activity log, work out the steps that take
//! it back. Pure; the store applies them (`minimap_store::undo`), the app keeps the stack.
//!
//! One command is one undo step, however many rows it wrote (bulk create, archive with its
//! tasks and links, quick-add). The inverse of each row:
//!
//! | row | inverse |
//! |---|---|
//! | created | archive the node (a soft delete: nothing is ever hard-deleted by undo) |
//! | archived | restore it (and the links archived with it) |
//! | restored | archive it again |
//! | updated | put the old values back, if the fields still hold what the row recorded |
//! | edge added | remove the link |
//! | edge removed | add the link back (rules and loops are checked again) |
//!
//! Links that touch a node created, archived or restored in the same step are not listed: the
//! archive or restore of the node already takes them along.
//!
//! What is *not* part of undo, and how it is told apart:
//! - **Ignored** (no step at all): edits to a note (its text is never in the log, and the editor
//!   has its own undo), `mentions` links (derived from the note text), and the first-run "me".
//! - **Irreversible** (a step that says so when it comes up, so Ctrl+Z never silently undoes
//!   something older than what the user just did): a hard delete, an attachment added or
//!   removed, a link's attributes changed.

use minimap_types::{Activity, ActivityAction, EdgeType, NodeRef, NodeType, Uuid};
use serde_json::{Map, Value};

/// One thing to do to take a step back.
#[derive(Debug, Clone, PartialEq)]
pub enum Inverse {
    Archive(NodeRef),
    Unarchive(NodeRef),
    /// Put `old` back in these fields; `new` is what they must hold now (else something else
    /// changed them since and the step is refused).
    Restore {
        node: NodeRef,
        old: Map<String, Value>,
        new: Map<String, Value>,
    },
    RemoveEdge {
        edge_id: Uuid,
    },
    AddEdge {
        edge_type: EdgeType,
        from: NodeRef,
        to: NodeRef,
        attrs: Value,
    },
}

/// What a command's rows mean for undo.
#[derive(Debug, Clone, PartialEq)]
pub enum Plan {
    /// Nothing to remember.
    Ignore,
    /// Remember that it happened and can't be undone.
    Irreversible(String),
    /// Apply these, in order, to undo it.
    Steps(Vec<Inverse>),
}

/// Fields that follow from others and are never restored by themselves.
const DERIVED: &[&str] = &["completed_at"];

fn node_of(a: &Activity) -> NodeRef {
    NodeRef::new(a.node_type, a.node_id)
}

/// The edge described by an `edge_added` / `edge_removed` row: (id, type, from, to, attrs).
fn edge_of(a: &Activity) -> Option<(Uuid, EdgeType, NodeRef, NodeRef, Value)> {
    let side = usize::from(a.action == ActivityAction::EdgeAdded);
    let desc = a.diff.get("edge")?.get(side)?;
    let id: Uuid = desc.get("id")?.as_str()?.parse().ok()?;
    let edge_type: EdgeType = desc.get("edge_type")?.as_str()?.parse().ok()?;
    let to_type: NodeType = desc.get("to_type")?.as_str()?.parse().ok()?;
    let to_id: Uuid = desc.get("to_id")?.as_str()?.parse().ok()?;
    let attrs = desc.get("attrs").cloned().unwrap_or(Value::Null);
    Some((
        id,
        edge_type,
        node_of(a),
        NodeRef::new(to_type, to_id),
        attrs,
    ))
}

fn is_edge_row(a: &Activity) -> bool {
    matches!(
        a.action,
        ActivityAction::EdgeAdded | ActivityAction::EdgeRemoved
    )
}

/// Rows that are not something the user did.
fn is_ignored(a: &Activity) -> bool {
    if a.node_type == NodeType::Note && a.action == ActivityAction::Updated {
        return true;
    }
    if is_edge_row(a) {
        return edge_of(a).is_some_and(|(_, t, ..)| t == EdgeType::Mentions);
    }
    // First run's "me".
    a.action == ActivityAction::Created
        && a.node_type == NodeType::Person
        && a.diff.get("is_self").and_then(|v| v.get(1)) == Some(&Value::Bool(true))
}

/// `{field: [old, new]}` as two maps, without the derived fields. `None` when the diff isn't in
/// that shape.
fn changes(a: &Activity) -> Option<(Map<String, Value>, Map<String, Value>)> {
    let diff = a.diff.as_object()?;
    let (mut old, mut new) = (Map::new(), Map::new());
    for (field, pair) in diff {
        let pair = pair.as_array().filter(|p| p.len() == 2)?;
        if DERIVED.contains(&field.as_str()) {
            continue;
        }
        old.insert(field.clone(), pair[0].clone());
        new.insert(field.clone(), pair[1].clone());
    }
    Some((old, new))
}

/// Why an `updated` row can't be reversed, if it can't.
fn irreversible_update(a: &Activity) -> Option<String> {
    let diff = a.diff.as_object()?;
    if diff.contains_key("attachment") {
        return Some("attachments can't be taken back".into());
    }
    if diff.keys().any(|k| k.ends_with(" link")) {
        return Some("a link's details can't be taken back".into());
    }
    None
}

/// What to do to undo the command that wrote `rows` (oldest first).
pub fn plan(rows: &[Activity]) -> Plan {
    let rows: Vec<&Activity> = rows.iter().filter(|a| !is_ignored(a)).collect();
    if rows.is_empty() {
        return Plan::Ignore;
    }
    for a in &rows {
        match a.action {
            ActivityAction::Deleted => {
                return Plan::Irreversible("deleted items can't be brought back".into())
            }
            ActivityAction::Updated => {
                if let Some(why) = irreversible_update(a) {
                    return Plan::Irreversible(why);
                }
            }
            _ => {}
        }
    }

    // Nodes whose archive or restore also takes their links along.
    let carried: Vec<Uuid> = rows
        .iter()
        .filter(|a| {
            matches!(
                a.action,
                ActivityAction::Created | ActivityAction::Archived | ActivityAction::Unarchived
            )
        })
        .map(|a| a.node_id)
        .collect();

    let mut inverse = Vec::new();
    for a in rows.iter().rev() {
        match a.action {
            ActivityAction::Created | ActivityAction::Unarchived => {
                inverse.push(Inverse::Archive(node_of(a)))
            }
            ActivityAction::Archived => inverse.push(Inverse::Unarchive(node_of(a))),
            ActivityAction::Updated => {
                let Some((old, new)) = changes(a) else {
                    return Plan::Irreversible(
                        "it was recorded in a form that can't be reversed".into(),
                    );
                };
                if !old.is_empty() {
                    inverse.push(Inverse::Restore {
                        node: node_of(a),
                        old,
                        new,
                    });
                }
            }
            ActivityAction::EdgeAdded | ActivityAction::EdgeRemoved => {
                let Some((edge_id, edge_type, from, to, attrs)) = edge_of(a) else {
                    return Plan::Irreversible(
                        "it was recorded in a form that can't be reversed".into(),
                    );
                };
                if carried.contains(&from.id) || carried.contains(&to.id) {
                    continue;
                }
                inverse.push(if a.action == ActivityAction::EdgeAdded {
                    Inverse::RemoveEdge { edge_id }
                } else {
                    Inverse::AddEdge {
                        edge_type,
                        from,
                        to,
                        attrs,
                    }
                });
            }
            ActivityAction::Deleted => {}
        }
    }
    if inverse.is_empty() {
        Plan::Ignore
    } else {
        Plan::Steps(inverse)
    }
}

// --------------------------------------------------------------------------- wording

/// What a command did, in a form [`headline_text`] can phrase once the names are known.
#[derive(Debug, Clone, PartialEq)]
pub struct Headline {
    pub verb: Verb,
    pub node: NodeRef,
    /// The other end of a link.
    pub to: Option<NodeRef>,
    pub edge_type: Option<EdgeType>,
    /// Changed fields, for an update.
    pub fields: Vec<String>,
    /// How many other items the same command touched.
    pub more: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verb {
    Created,
    Archived,
    Restored,
    Updated,
    Linked,
    Unlinked,
    Deleted,
}

/// The main thing a command did: its last row about a node (what a command that touches
/// several things does last is the thing itself: a project is archived after its tasks, a task
/// is created after the people it needed), else its first link. `None` when the rows are all
/// ignorable.
pub fn headline(rows: &[Activity]) -> Option<Headline> {
    let rows: Vec<&Activity> = rows.iter().filter(|a| !is_ignored(a)).collect();
    let is_node_row = |a: &&&Activity| !is_edge_row(a);
    let primary = rows
        .iter()
        .rev()
        .find(is_node_row)
        .or_else(|| rows.first())?;
    let more = rows
        .iter()
        .filter(is_node_row)
        .count()
        .saturating_sub(usize::from(!is_edge_row(primary)));
    let mut h = Headline {
        verb: Verb::Updated,
        node: node_of(primary),
        to: None,
        edge_type: None,
        fields: Vec::new(),
        more,
    };
    match primary.action {
        ActivityAction::Created => h.verb = Verb::Created,
        ActivityAction::Archived => h.verb = Verb::Archived,
        ActivityAction::Unarchived => h.verb = Verb::Restored,
        ActivityAction::Deleted => h.verb = Verb::Deleted,
        ActivityAction::Updated => {
            h.fields = primary
                .diff
                .as_object()
                .map(|d| {
                    d.keys()
                        .filter(|k| !DERIVED.contains(&k.as_str()))
                        .map(|k| k.replace('_', " "))
                        .collect()
                })
                .unwrap_or_default();
        }
        ActivityAction::EdgeAdded | ActivityAction::EdgeRemoved => {
            h.verb = if primary.action == ActivityAction::EdgeAdded {
                Verb::Linked
            } else {
                Verb::Unlinked
            };
            if let Some((_, edge_type, _, to, _)) = edge_of(primary) {
                h.to = Some(to);
                h.edge_type = Some(edge_type);
            }
        }
    }
    Some(h)
}

fn type_word(t: NodeType) -> String {
    t.as_str().replace('_', " ")
}

/// "archived task Fix login", "changed due date of task Fix login", "added a blocks link from
/// A to B", with " and 3 more changes" when a command touched several items. `name` gives the
/// current name of a node.
pub fn headline_text(h: &Headline, name: impl Fn(NodeRef) -> String) -> String {
    let subject = |n: NodeRef| format!("{} {}", type_word(n.node_type), name(n));
    let mut text = match h.verb {
        Verb::Created => format!("created {}", subject(h.node)),
        Verb::Archived => format!("archived {}", subject(h.node)),
        Verb::Restored => format!("restored {}", subject(h.node)),
        Verb::Deleted => format!("deleted {}", type_word(h.node.node_type)),
        Verb::Updated => match h.fields.as_slice() {
            [one] => format!("changed {one} of {}", subject(h.node)),
            _ => format!("edited {}", subject(h.node)),
        },
        Verb::Linked | Verb::Unlinked => {
            let kind = h
                .edge_type
                .map_or("a".to_owned(), |t| t.as_str().replace('_', " "));
            let to = h.to.map_or("something".to_owned(), &name);
            let verb = if h.verb == Verb::Linked {
                "added a"
            } else {
                "removed the"
            };
            format!("{verb} {kind} link from {} to {to}", name(h.node))
        }
    };
    if h.more > 0 {
        text.push_str(&format!(
            " and {} more change{}",
            h.more,
            if h.more == 1 { "" } else { "s" }
        ));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use time::OffsetDateTime;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn row(action: ActivityAction, node_type: NodeType, node: u128, diff: Value) -> Activity {
        Activity {
            id: Uuid::from_u128(1000 + node),
            at: OffsetDateTime::UNIX_EPOCH,
            node_type,
            node_id: id(node),
            action,
            diff,
        }
    }

    fn task(n: u128) -> NodeRef {
        NodeRef::new(NodeType::Task, id(n))
    }

    fn edge_row(
        action: ActivityAction,
        edge: u128,
        edge_type: &str,
        from: (NodeType, u128),
        to: (NodeType, u128),
    ) -> Activity {
        let desc = json!({
            "id": id(edge), "edge_type": edge_type, "to_type": to.0.as_str(),
            "to_id": id(to.1), "attrs": {"lag_days": 1},
        });
        let diff = if action == ActivityAction::EdgeAdded {
            json!({"edge": [null, desc]})
        } else {
            json!({"edge": [desc, null]})
        };
        row(action, from.0, from.1, diff)
    }

    fn steps(rows: &[Activity]) -> Vec<Inverse> {
        match plan(rows) {
            Plan::Steps(s) => s,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn creating_is_undone_by_archiving() {
        let created = row(
            ActivityAction::Created,
            NodeType::Task,
            1,
            json!({"title": [null, "A"]}),
        );
        assert_eq!(steps(&[created]), vec![Inverse::Archive(task(1))]);
    }

    #[test]
    fn archiving_is_undone_by_restoring_and_back() {
        let archived = row(
            ActivityAction::Archived,
            NodeType::Task,
            1,
            json!({"archived_at": [null, "2027-03-01T00:00:00.000Z"]}),
        );
        assert_eq!(steps(&[archived]), vec![Inverse::Unarchive(task(1))]);
        let restored = row(
            ActivityAction::Unarchived,
            NodeType::Task,
            1,
            json!({"archived_at": ["2027-03-01T00:00:00.000Z", null]}),
        );
        assert_eq!(steps(&[restored]), vec![Inverse::Archive(task(1))]);
    }

    #[test]
    fn an_update_puts_the_old_values_back_and_remembers_what_it_expects() {
        let updated = row(
            ActivityAction::Updated,
            NodeType::Task,
            1,
            json!({
                "status": ["todo", "done"],
                "due_date": ["2027-03-05", null],
                "completed_at": [null, "2027-03-03T10:00:00.000Z"],
            }),
        );
        let s = steps(&[updated]);
        let Inverse::Restore { node, old, new } = &s[0] else {
            panic!("{s:?}")
        };
        assert_eq!(*node, task(1));
        assert_eq!(old.get("status"), Some(&json!("todo")));
        assert_eq!(old.get("due_date"), Some(&json!("2027-03-05")));
        assert_eq!(new.get("due_date"), Some(&Value::Null));
        // completed_at follows from the status, so it is not restored by itself.
        assert!(!old.contains_key("completed_at") && !new.contains_key("completed_at"));
    }

    #[test]
    fn links_are_removed_or_put_back() {
        let added = edge_row(
            ActivityAction::EdgeAdded,
            7,
            "blocks",
            (NodeType::Task, 1),
            (NodeType::Task, 2),
        );
        assert_eq!(
            steps(&[added]),
            vec![Inverse::RemoveEdge { edge_id: id(7) }]
        );
        let removed = edge_row(
            ActivityAction::EdgeRemoved,
            7,
            "blocks",
            (NodeType::Task, 1),
            (NodeType::Task, 2),
        );
        assert_eq!(
            steps(&[removed]),
            vec![Inverse::AddEdge {
                edge_type: EdgeType::Blocks,
                from: task(1),
                to: task(2),
                attrs: json!({"lag_days": 1}),
            }]
        );
    }

    #[test]
    fn a_step_is_undone_in_reverse_order() {
        // Supersede: a link, then the older decision's status.
        let link = edge_row(
            ActivityAction::EdgeAdded,
            7,
            "supersedes",
            (NodeType::Decision, 1),
            (NodeType::Decision, 2),
        );
        let status = row(
            ActivityAction::Updated,
            NodeType::Decision,
            2,
            json!({"status": ["decided", "superseded"]}),
        );
        let s = steps(&[link, status]);
        assert!(matches!(s[0], Inverse::Restore { .. }));
        assert_eq!(s[1], Inverse::RemoveEdge { edge_id: id(7) });
    }

    #[test]
    fn links_that_an_archive_or_create_carries_are_left_to_it() {
        // Creating a task with an assignee: the node and its link. Undo archives the task, and
        // the archive takes the link.
        let created = row(
            ActivityAction::Created,
            NodeType::Task,
            1,
            json!({"title": [null, "A"]}),
        );
        let assigned = edge_row(
            ActivityAction::EdgeAdded,
            7,
            "assigned_to",
            (NodeType::Task, 1),
            (NodeType::Person, 9),
        );
        assert_eq!(steps(&[created, assigned]), vec![Inverse::Archive(task(1))]);
        // Archiving a node and the links it took.
        let archived = row(ActivityAction::Archived, NodeType::Task, 1, json!({}));
        let dropped = edge_row(
            ActivityAction::EdgeRemoved,
            8,
            "blocks",
            (NodeType::Task, 2),
            (NodeType::Task, 1),
        );
        assert_eq!(
            steps(&[archived, dropped]),
            vec![Inverse::Unarchive(task(1))]
        );
        // A link between two other nodes in the same command is still its own step.
        let other = edge_row(
            ActivityAction::EdgeAdded,
            9,
            "blocks",
            (NodeType::Task, 3),
            (NodeType::Task, 4),
        );
        let created = row(ActivityAction::Created, NodeType::Task, 1, json!({}));
        assert_eq!(
            steps(&[created, other]),
            vec![
                Inverse::RemoveEdge { edge_id: id(9) },
                Inverse::Archive(task(1))
            ]
        );
    }

    #[test]
    fn note_edits_mentions_and_the_first_run_me_are_not_steps() {
        let edit = row(
            ActivityAction::Updated,
            NodeType::Note,
            1,
            json!({"body": ["10 chars", "12 chars"]}),
        );
        assert_eq!(plan(&[edit]), Plan::Ignore);
        let mention = edge_row(
            ActivityAction::EdgeAdded,
            7,
            "mentions",
            (NodeType::Note, 1),
            (NodeType::Person, 2),
        );
        assert_eq!(plan(&[mention]), Plan::Ignore);
        let me = row(
            ActivityAction::Created,
            NodeType::Person,
            1,
            json!({"name": [null, "Me"], "is_self": [null, true]}),
        );
        assert_eq!(plan(&[me]), Plan::Ignore);
        // But a note's creation and archiving are.
        let created = row(ActivityAction::Created, NodeType::Note, 1, json!({}));
        assert!(matches!(plan(&[created]), Plan::Steps(_)));
        // An edit that came with something else keeps that something else.
        let edit = row(
            ActivityAction::Updated,
            NodeType::Note,
            1,
            json!({"body": ["a", "b"]}),
        );
        let task_row = row(ActivityAction::Created, NodeType::Task, 2, json!({}));
        assert_eq!(steps(&[edit, task_row]), vec![Inverse::Archive(task(2))]);
    }

    #[test]
    fn what_cannot_be_undone_says_so() {
        let deleted = row(ActivityAction::Deleted, NodeType::Task, 1, json!({}));
        assert!(matches!(plan(&[deleted]), Plan::Irreversible(m) if m.contains("deleted")));
        let attached = row(
            ActivityAction::Updated,
            NodeType::Task,
            1,
            json!({"attachment": [null, "plan.pdf"]}),
        );
        assert!(matches!(plan(&[attached]), Plan::Irreversible(_)));
        let weight = row(
            ActivityAction::Updated,
            NodeType::Project,
            1,
            json!({"contributes_to link": [{"weight": 1}, {"weight": 0.5}]}),
        );
        assert!(matches!(plan(&[weight]), Plan::Irreversible(_)));
        // One irreversible row makes the whole command irreversible.
        let created = row(ActivityAction::Created, NodeType::Task, 2, json!({}));
        let deleted = row(ActivityAction::Deleted, NodeType::Task, 1, json!({}));
        assert!(matches!(plan(&[created, deleted]), Plan::Irreversible(_)));
        // A row in a shape we don't know is not guessed at.
        let odd = row(
            ActivityAction::Updated,
            NodeType::Task,
            1,
            json!({"title": "x"}),
        );
        assert!(matches!(plan(&[odd]), Plan::Irreversible(_)));
    }

    #[test]
    fn nothing_written_is_nothing_to_undo() {
        assert_eq!(plan(&[]), Plan::Ignore);
        assert_eq!(headline(&[]), None);
    }

    #[test]
    fn headlines_name_what_was_done() {
        let name = |n: NodeRef| match n.id.as_u128() {
            1 => "Fix login".to_owned(),
            2 => "Release".to_owned(),
            _ => "?".to_owned(),
        };
        let text = |rows: &[Activity]| headline_text(&headline(rows).unwrap(), name);

        let archived = row(ActivityAction::Archived, NodeType::Task, 1, json!({}));
        assert_eq!(
            text(std::slice::from_ref(&archived)),
            "archived task Fix login"
        );
        let created = row(ActivityAction::Created, NodeType::WaitingOn, 1, json!({}));
        assert_eq!(text(&[created]), "created waiting on Fix login");
        let one = row(
            ActivityAction::Updated,
            NodeType::Task,
            1,
            json!({"due_date": ["a", "b"], "completed_at": [null, "x"]}),
        );
        assert_eq!(text(&[one]), "changed due date of task Fix login");
        let two = row(
            ActivityAction::Updated,
            NodeType::Task,
            1,
            json!({"due_date": ["a", "b"], "title": ["a", "b"]}),
        );
        assert_eq!(text(&[two]), "edited task Fix login");
        let linked = edge_row(
            ActivityAction::EdgeAdded,
            7,
            "depends_on",
            (NodeType::Project, 1),
            (NodeType::Project, 2),
        );
        assert_eq!(
            text(&[linked]),
            "added a depends on link from Fix login to Release"
        );
        // Several items: the last (the thing itself) is named, the rest counted. Links don't
        // count.
        let project = row(ActivityAction::Archived, NodeType::Project, 2, json!({}));
        let link = edge_row(
            ActivityAction::EdgeRemoved,
            8,
            "blocks",
            (NodeType::Task, 2),
            (NodeType::Task, 1),
        );
        assert_eq!(
            text(&[archived, project, link]),
            "archived project Release and 1 more change"
        );
    }
}
