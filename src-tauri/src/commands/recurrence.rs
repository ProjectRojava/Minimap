//! Recurring items (spec 27): setting a task's or note's repeat rule, and making the notes of
//! repeating series on their dates. The rules and dates are `minimap_core::recurrence`; finishing
//! a repeating task (which makes the next one) is in `store::tasks`.

use minimap_core::recurrence::parse_every;
use minimap_store::Connection;
use minimap_types::{AppError, Date, NodeRef, NodeType, Patch, Recurrence, UpdateNote, UpdateTask};
use tauri::State;

use crate::{
    error::{app_error, store_error},
    state::AppState,
};

/// Makes the item repeat by `text` (`day`, `mon`, `2w`, `2w:fri`, `month`, `month:15`), or stops
/// it repeating when `text` is empty (or `none`). A task makes its next one when it is finished;
/// a note makes its next one on the rule's next date, starting from `template` (when given; else
/// the rule's current one, else this note's own text). Answers the rule now in force.
#[tauri::command]
pub async fn set_recurrence(
    state: State<'_, AppState>,
    node: NodeRef,
    text: String,
    template: Option<String>,
) -> Result<Option<Recurrence>, AppError> {
    state
        .run(move |conn| set_recurrence_impl(conn, node, &text, template, minimap_store::today()))
        .await
}

pub(crate) fn set_recurrence_impl(
    conn: &mut Connection,
    node: NodeRef,
    text: &str,
    template: Option<String>,
    today: Date,
) -> Result<Option<Recurrence>, AppError> {
    let text = text.trim();
    let clear = text.is_empty() || matches!(text.to_lowercase().as_str(), "none" | "off" | "no");
    let saved = match node.node_type {
        NodeType::Task => {
            let task = minimap_store::tasks::get(conn, node.id).map_err(store_error)?;
            let recurrence = if clear {
                Patch::Clear
            } else {
                let cadence = parse_every(text, task.due_date.unwrap_or(today))
                    .map_err(|e| app_error("invalid", e))?;
                Patch::Set(Recurrence::from(cadence))
            };
            minimap_store::tasks::update(
                conn,
                node.id,
                UpdateTask {
                    recurrence,
                    ..Default::default()
                },
            )
            .map_err(store_error)?
            .recurrence
        }
        NodeType::Note => {
            let note = minimap_store::notes::get(conn, node.id).map_err(store_error)?;
            let recurrence = if clear {
                Patch::Clear
            } else {
                let cadence =
                    parse_every(text, note.note_date).map_err(|e| app_error("invalid", e))?;
                // The template: what was given, else what the rule already had, else this
                // note's own text (a new repeating note starts like the first one).
                let template = match template {
                    Some(t) if t.trim().is_empty() => None,
                    Some(t) => Some(t),
                    None => note.recurrence.as_ref().map_or_else(
                        || Some(note.body.clone()).filter(|b| !b.trim().is_empty()),
                        |r| r.template.clone(),
                    ),
                };
                Patch::Set(Recurrence { cadence, template })
            };
            minimap_store::notes::update(
                conn,
                node.id,
                UpdateNote {
                    recurrence,
                    ..Default::default()
                },
            )
            .map_err(store_error)?
            .recurrence
        }
        other => {
            return Err(app_error(
                "invalid",
                format!("Only tasks and notes can repeat, not a {other}"),
            ))
        }
    };
    Ok(saved)
}

/// Makes the notes that repeating series have come due for. Run at start and every few
/// minutes: a note is made on its date, whether or not the app was open then.
pub(crate) fn generate_due_notes_tick(state: &AppState) {
    let Ok(mut vault) = state.db.lock() else {
        return;
    };
    let Ok((conn, _)) = vault.parts() else {
        return; // locked: try again later
    };
    match minimap_store::notes::generate_due(conn, minimap_store::today()) {
        Ok(0) => {}
        Ok(made) => {
            tracing::info!(made, "made the notes of repeating series");
            // A write like any other: leave it to be saved to Drive.
            state.sync.engine.note_change(conn);
        }
        Err(e) => tracing::warn!(error = %e, "repeating notes failed"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{AssigneeChoice, Cadence, CreateNote, CreateTask};
    use time::macros::date;

    const TODAY: Date = date!(2027 - 03 - 03);

    fn task(conn: &mut Connection, due: Option<Date>) -> NodeRef {
        let t = minimap_store::tasks::create(
            conn,
            CreateTask {
                title: "Board update".into(),
                assignee: AssigneeChoice::Nobody,
                description: String::new(),
                project_id: None,
                status: None,
                estimate_days: None,
                start_date: None,
                due_date: due,
                priority: None,
                recurrence: None,
            },
        )
        .unwrap();
        NodeRef::new(NodeType::Task, t.id)
    }

    fn note(conn: &mut Connection, body: &str) -> NodeRef {
        let n = minimap_store::notes::create(
            conn,
            CreateNote {
                title: "1:1".into(),
                body: body.into(),
                note_date: Some(date!(2027 - 03 - 01)),
                kind: None,
                recurrence: None,
            },
        )
        .unwrap();
        NodeRef::new(NodeType::Note, n.id)
    }

    fn rule_of_task(conn: &Connection, n: NodeRef) -> Option<Recurrence> {
        minimap_store::tasks::get(conn, n.id).unwrap().recurrence
    }

    fn rule_of_note(conn: &Connection, n: NodeRef) -> Option<Recurrence> {
        minimap_store::notes::get(conn, n.id).unwrap().recurrence
    }

    #[test]
    fn a_task_repeats_by_what_is_typed_and_stops_when_it_is_cleared() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        // Due on a Wednesday: "2w" repeats on Wednesdays.
        let t = task(&mut conn, Some(date!(2027 - 03 - 10)));
        set_recurrence_impl(&mut conn, t, "2w", None, TODAY).unwrap();
        assert_eq!(
            rule_of_task(&conn, t).map(|r| r.cadence),
            Some(Cadence::Weekly {
                every: 2,
                weekday: 2
            })
        );
        set_recurrence_impl(&mut conn, t, " MON ", None, TODAY).unwrap();
        assert_eq!(
            rule_of_task(&conn, t).map(|r| r.cadence),
            Some(Cadence::Weekly {
                every: 1,
                weekday: 0
            })
        );
        for off in ["", "none", "Off"] {
            set_recurrence_impl(&mut conn, t, "day", None, TODAY).unwrap();
            set_recurrence_impl(&mut conn, t, off, None, TODAY).unwrap();
            assert_eq!(rule_of_task(&conn, t), None, "{off:?}");
        }
    }

    #[test]
    fn a_bad_rule_says_what_is_accepted_and_changes_nothing() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let t = task(&mut conn, None);
        set_recurrence_impl(&mut conn, t, "mon", None, TODAY).unwrap();
        let e = set_recurrence_impl(&mut conn, t, "sometimes", None, TODAY).unwrap_err();
        assert_eq!(e.code, "invalid");
        assert!(e.message.contains("Repeats look like"), "{}", e.message);
        assert!(rule_of_task(&conn, t).is_some(), "the old rule stays");
    }

    #[test]
    fn only_tasks_and_notes_can_repeat() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let person = minimap_store::people::ensure_self(&mut conn, "Me").unwrap();
        let e = set_recurrence_impl(
            &mut conn,
            NodeRef::new(NodeType::Person, person.id),
            "mon",
            None,
            TODAY,
        )
        .unwrap_err();
        assert_eq!(e.code, "invalid");
        assert!(e.message.contains("Only tasks and notes"), "{}", e.message);
    }

    #[test]
    fn a_notes_template_is_given_kept_or_taken_from_its_text() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let n = note(&mut conn, "## Agenda\n\n- ");
        // Enabling it takes the note's own text as the template.
        set_recurrence_impl(&mut conn, n, "mon", None, TODAY).unwrap();
        let rule = rule_of_note(&conn, n).unwrap();
        assert_eq!(rule.template.as_deref(), Some("## Agenda\n\n- "));
        // Changing the rule keeps the template.
        set_recurrence_impl(&mut conn, n, "wed", None, TODAY).unwrap();
        let rule = rule_of_note(&conn, n).unwrap();
        assert_eq!(
            rule.cadence,
            Cadence::Weekly {
                every: 1,
                weekday: 2
            }
        );
        assert_eq!(rule.template.as_deref(), Some("## Agenda\n\n- "));
        // A given template replaces it; an empty one removes it.
        set_recurrence_impl(&mut conn, n, "wed", Some("New text".into()), TODAY).unwrap();
        assert_eq!(
            rule_of_note(&conn, n).unwrap().template.as_deref(),
            Some("New text")
        );
        set_recurrence_impl(&mut conn, n, "wed", Some("  ".into()), TODAY).unwrap();
        assert_eq!(rule_of_note(&conn, n).unwrap().template, None);
        // An empty note has nothing to take.
        let empty = note(&mut conn, "");
        set_recurrence_impl(&mut conn, empty, "mon", None, TODAY).unwrap();
        assert_eq!(rule_of_note(&conn, empty).unwrap().template, None);
    }

    #[test]
    fn finishing_a_task_through_the_command_layer_makes_the_next_one() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let t = task(&mut conn, Some(date!(2099 - 01 - 05)));
        set_recurrence_impl(&mut conn, t, "mon", None, TODAY).unwrap();
        minimap_store::tasks::update(
            &mut conn,
            t.id,
            UpdateTask {
                status: Some(minimap_types::TaskStatus::Done),
                ..Default::default()
            },
        )
        .unwrap();
        let open: Vec<_> = minimap_store::tasks::list(&conn, false)
            .unwrap()
            .into_iter()
            .filter(|x| x.status == minimap_types::TaskStatus::Todo)
            .collect();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].due_date, Some(date!(2099 - 01 - 12)));
        assert!(open[0].recurrence.is_some());
    }
}
