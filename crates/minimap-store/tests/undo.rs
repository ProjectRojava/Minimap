//! Undo (spec 25): every kind of write is taken back, and put back by redo, exactly.
//! The strong check is a fingerprint of everything active in the database before and after.

use minimap_core::undo::{plan, Plan};
use minimap_store::*;
use minimap_types::*;
use time::macros::date;
use uuid::Uuid;

const TODAY: time::Date = date!(2027 - 03 - 03);

fn demo() -> Connection {
    let mut conn = open_in_memory().unwrap();
    demo::seed(&mut conn, TODAY).unwrap();
    conn
}

/// Every active row of every node table (without bookkeeping times) and every active link.
fn fingerprint(conn: &Connection) -> Vec<String> {
    let mut out = Vec::new();
    for table in [
        "objectives",
        "projects",
        "tasks",
        "people",
        "teams",
        "notes",
        "decisions",
        "waiting_on",
    ] {
        let cols: Vec<String> = conn
            .prepare(&format!("PRAGMA table_info({table})"))
            .unwrap()
            .query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .map(|r| r.unwrap())
            .filter(|c| !matches!(c.as_str(), "updated_at" | "completed_at"))
            .collect();
        let expr = cols
            .iter()
            .map(|c| format!("quote({c})"))
            .collect::<Vec<_>>()
            .join(" || '|' || ");
        let extra = if table == "tasks" {
            " || '|' || (completed_at IS NOT NULL)"
        } else {
            ""
        };
        let sql = format!(
            "SELECT '{table}:' || {expr}{extra} FROM {table} WHERE archived_at IS NULL ORDER BY id"
        );
        out.extend(
            conn.prepare(&sql)
                .unwrap()
                .query_map([], |r| r.get::<_, String>(0))
                .unwrap()
                .map(|r| r.unwrap()),
        );
    }
    out.extend(
        conn.prepare(
            "SELECT 'edge:' || edge_type || from_id || to_id || attrs FROM edges
             WHERE archived_at IS NULL ORDER BY edge_type, from_id, to_id",
        )
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|r| r.unwrap()),
    );
    out
}

/// What `f` wrote.
fn wrote(conn: &mut Connection, f: impl FnOnce(&mut Connection)) -> Vec<Activity> {
    let marker = activity::latest_rowid(conn).unwrap();
    f(conn);
    activity::since(conn, marker).unwrap()
}

fn steps_of(rows: &[Activity]) -> Vec<minimap_core::undo::Inverse> {
    match plan(rows) {
        Plan::Steps(s) => s,
        other => panic!("expected steps, got {other:?}"),
    }
}

/// Undoes `rows` and returns the rows the undo wrote (what redo works from).
fn undo(conn: &mut Connection, rows: &[Activity]) -> Vec<Activity> {
    undo::apply(conn, &steps_of(rows)).unwrap()
}

/// The full cycle: do it, undo it (everything as before), redo it (everything as after), undo
/// again. Ends with everything as it was before.
fn round_trip(conn: &mut Connection, op: impl FnOnce(&mut Connection)) {
    let before = fingerprint(conn);
    let rows = wrote(conn, op);
    assert!(!rows.is_empty(), "the operation wrote nothing");
    let after = fingerprint(conn);
    assert_ne!(before, after, "the operation changed nothing");

    let undone = undo(conn, &rows);
    assert_eq!(fingerprint(conn), before, "undo did not restore everything");

    let redone = undo(conn, &undone);
    assert_eq!(fingerprint(conn), after, "redo did not put everything back");

    undo(conn, &redone);
    assert_eq!(
        fingerprint(conn),
        before,
        "a second undo did not restore everything"
    );
}

fn task(conn: &Connection, title: &str) -> Task {
    tasks::list(conn, false)
        .unwrap()
        .into_iter()
        .find(|t| t.title == title)
        .unwrap_or_else(|| panic!("no task {title}"))
}

fn project(conn: &Connection, title: &str) -> Project {
    projects::list(conn, false)
        .unwrap()
        .into_iter()
        .find(|p| p.title == title)
        .unwrap()
}

fn person(conn: &Connection, name: &str) -> Person {
    people::list(conn, false)
        .unwrap()
        .into_iter()
        .find(|p| p.name == name)
        .unwrap()
}

fn node(node_type: NodeType, id: Uuid) -> NodeRef {
    NodeRef::new(node_type, id)
}

#[test]
fn creating_a_task_with_its_assignee_is_undone_and_redone() {
    let mut conn = demo();
    let priya = person(&conn, "Priya Nair").id;
    round_trip(&mut conn, |c| {
        tasks::create(
            c,
            CreateTask {
                links: Vec::new(),
                title: "Write the runbook".into(),
                assignee: AssigneeChoice::Person(priya),
                description: String::new(),
                project_id: None,
                status: None,
                estimate_days: Some(2.0),
                start_date: None,
                due_date: Some(date!(2027 - 03 - 10)),
                priority: Some(2),
                recurrence: None,
            },
        )
        .unwrap();
    });
}

#[test]
fn creating_many_tasks_is_one_step() {
    let mut conn = demo();
    let before = fingerprint(&conn);
    let rows = wrote(&mut conn, |c| {
        tasks::create_many(
            c,
            ["One", "Two", "Three"]
                .iter()
                .map(|t| CreateTask {
                    links: Vec::new(),
                    title: (*t).into(),
                    assignee: AssigneeChoice::Me,
                    description: String::new(),
                    project_id: None,
                    status: None,
                    estimate_days: None,
                    start_date: None,
                    due_date: None,
                    priority: None,
                    recurrence: None,
                })
                .collect(),
        )
        .unwrap();
    });
    let after = fingerprint(&conn);
    undo(&mut conn, &rows);
    assert_eq!(fingerprint(&conn), before);
    let again = wrote(&mut conn, |c| {
        undo::apply(c, &steps_of(&rows)).ok();
    });
    // (Already undone: the second attempt is refused and writes nothing.)
    assert!(again.is_empty());
    assert_ne!(before, after);
}

#[test]
fn updating_fields_including_clearing_a_date_and_finishing_a_task() {
    let mut conn = demo();
    let t = task(&conn, "Rotate service credentials");
    round_trip(&mut conn, |c| {
        tasks::update(
            c,
            t.id,
            UpdateTask {
                title: Some("Rotate all service credentials".into()),
                due_date: Patch::Clear,
                estimate_days: Patch::Set(4.5),
                priority: Some(1),
                ..Default::default()
            },
        )
        .unwrap();
    });
    // Finishing sets completed_at and undoing clears it again.
    let t = task(&conn, "Savings review with finance");
    round_trip(&mut conn, |c| {
        tasks::update(
            c,
            t.id,
            UpdateTask {
                status: Some(TaskStatus::Done),
                ..Default::default()
            },
        )
        .unwrap();
    });
    let reopened = task(&conn, "Savings review with finance");
    assert!(reopened.completed_at.is_none() && reopened.status == TaskStatus::Todo);
}

#[test]
fn updating_every_other_kind_of_node() {
    let mut conn = demo();
    let objective = objectives::list(&conn, false).unwrap().remove(0);
    round_trip(&mut conn, |c| {
        objectives::update(
            c,
            objective.id,
            UpdateObjective {
                title: Some("Launch in the EU, properly".into()),
                target_date: Patch::Clear,
                status: Some(ObjectiveStatus::Done),
                ..Default::default()
            },
        )
        .unwrap();
    });
    let p = project(&conn, "EU Region");
    round_trip(&mut conn, |c| {
        projects::update(
            c,
            p.id,
            UpdateProject {
                slug: Some("eu".into()),
                owner_person_id: Patch::Clear,
                status: Some(ProjectStatus::Paused),
                ..Default::default()
            },
        )
        .unwrap();
    });
    let who = person(&conn, "Sam Okafor");
    round_trip(&mut conn, |c| {
        people::update(
            c,
            who.id,
            UpdatePerson {
                role_title: Some("Head of Product".into()),
                email: Patch::Clear,
                weekly_capacity_hours: Some(32.0),
                ..Default::default()
            },
        )
        .unwrap();
    });
    let team = teams::list(&conn, false)
        .unwrap()
        .into_iter()
        .find(|t| t.name == "Platform")
        .unwrap();
    round_trip(&mut conn, |c| {
        teams::update(
            c,
            team.id,
            UpdateTeam {
                parent_team_id: Patch::Clear,
                ..Default::default()
            },
        )
        .unwrap();
    });
    let decision = decisions::list(&conn, false)
        .unwrap()
        .into_iter()
        .find(|d| d.status == DecisionStatus::Proposed)
        .unwrap();
    round_trip(&mut conn, |c| {
        decisions::update(
            c,
            decision.id,
            UpdateDecision {
                status: Some(DecisionStatus::Decided),
                ..Default::default()
            },
        )
        .unwrap();
    });
    let waiting = waiting_on::list(&conn, false)
        .unwrap()
        .into_iter()
        .find(|w| w.resolved_on.is_none())
        .unwrap();
    round_trip(&mut conn, |c| {
        waiting_on::update(
            c,
            waiting.id,
            UpdateWaitingOn {
                resolved_on: Patch::Set(date!(2027 - 03 - 03)),
                follow_up_on: Patch::Set(date!(2027 - 03 - 08)),
                ..Default::default()
            },
        )
        .unwrap();
    });
}

#[test]
fn archiving_a_task_takes_its_links_away_and_brings_them_back() {
    let mut conn = demo();
    // "Provision EU network and clusters" has links: assignee, blocks, ...
    let t = task(&conn, "Provision EU network and clusters");
    let links_before = edges::links_for_node(&conn, t.id).unwrap().len();
    assert!(links_before >= 3);
    round_trip(&mut conn, |c| {
        nodes::archive(c, node(NodeType::Task, t.id)).unwrap();
    });
    assert_eq!(
        edges::links_for_node(&conn, t.id).unwrap().len(),
        links_before
    );
}

#[test]
fn archiving_a_project_with_its_tasks_or_moving_them_to_the_inbox() {
    for disposition in [TaskDisposition::Archive, TaskDisposition::Inbox] {
        let mut conn = demo();
        let p = project(&conn, "Platform Cost Reduction");
        let open_before = tasks::list(&conn, false)
            .unwrap()
            .iter()
            .filter(|t| t.project_id == Some(p.id))
            .count();
        round_trip(&mut conn, |c| {
            projects::archive(c, p.id, disposition).unwrap();
        });
        let open_after = tasks::list(&conn, false)
            .unwrap()
            .iter()
            .filter(|t| t.project_id == Some(p.id))
            .count();
        assert_eq!(open_before, open_after, "{disposition:?}");
    }
}

#[test]
fn archiving_a_person_and_restoring_them() {
    let mut conn = demo();
    let who = person(&conn, "Elena Rossi");
    round_trip(&mut conn, |c| {
        nodes::archive(c, node(NodeType::Person, who.id)).unwrap();
    });
}

#[test]
fn adding_and_removing_links() {
    let mut conn = demo();
    let a = task(&conn, "Book the leadership offsite");
    let b = task(&conn, "Review the Q2 headcount plan");
    round_trip(&mut conn, |c| {
        edges::add(
            c,
            NewEdge {
                edge_type: EdgeType::Blocks,
                from: node(NodeType::Task, a.id),
                to: node(NodeType::Task, b.id),
                attrs: serde_json::json!({"lag_days": 2}),
            },
        )
        .unwrap();
    });
    // Removing one that has details: they come back with it.
    let blocks = edges::list_active_of_type(&conn, EdgeType::Blocks)
        .unwrap()
        .into_iter()
        .find(|e| e.attrs.get("lag_days").is_some())
        .unwrap();
    round_trip(&mut conn, |c| edges::remove(c, blocks.id).unwrap());
}

#[test]
fn changing_a_manager_is_one_step_even_though_it_removes_and_adds() {
    let mut conn = demo();
    let tomas = person(&conn, "Tomás Alvarez");
    let raj = person(&conn, "Raj Patel");
    let old = edges::list_active_of_type(&conn, EdgeType::ReportsTo)
        .unwrap()
        .into_iter()
        .find(|e| e.from_id == tomas.id)
        .unwrap();
    round_trip(&mut conn, |c| {
        edges::remove(c, old.id).unwrap();
        edges::add(
            c,
            NewEdge {
                edge_type: EdgeType::ReportsTo,
                from: node(NodeType::Person, tomas.id),
                to: node(NodeType::Person, raj.id),
                attrs: serde_json::json!({}),
            },
        )
        .unwrap();
    });
}

#[test]
fn superseding_a_decision_is_undone_with_its_status() {
    let mut conn = demo();
    let list = decisions::list(&conn, false).unwrap();
    let new = list
        .iter()
        .find(|d| d.title == "Pause the spot-instance pilot")
        .unwrap();
    let old = list
        .iter()
        .find(|d| d.title == "Build the API gateway in-house")
        .unwrap();
    round_trip(&mut conn, |c| {
        decisions::supersede(c, new.id, old.id).unwrap();
    });
    let old_now = decisions::get(&conn, old.id).unwrap();
    assert_eq!(old_now.status, DecisionStatus::Decided);
}

#[test]
fn a_change_made_since_refuses_the_undo_and_writes_nothing() {
    let mut conn = demo();
    let t = task(&conn, "Savings review with finance");
    let rows = wrote(&mut conn, |c| {
        tasks::update(
            c,
            t.id,
            UpdateTask {
                title: Some("Access review, quarterly".into()),
                ..Default::default()
            },
        )
        .unwrap();
    });
    // Someone (another computer, a later edit) renames it again.
    tasks::update(
        &mut conn,
        t.id,
        UpdateTask {
            title: Some("Something else".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let before = fingerprint(&conn);
    let activity_before = activity::count(&conn).unwrap();
    let err = undo::apply(&mut conn, &steps_of(&rows)).unwrap_err();
    assert!(
        matches!(&err, StoreError::Invalid(m) if m.contains("was changed since") && m.contains("title")),
        "{err}"
    );
    assert_eq!(fingerprint(&conn), before);
    assert_eq!(activity::count(&conn).unwrap(), activity_before);
}

#[test]
fn a_step_that_fails_half_way_changes_nothing() {
    let mut conn = demo();
    let a = task(&conn, "Book the leadership offsite");
    let b = task(&conn, "Review the Q2 headcount plan");
    nodes::archive(&mut conn, node(NodeType::Task, b.id)).unwrap();
    let before = fingerprint(&conn);
    // Archive a (fine), then archive b (already archived: refused): a must stay active.
    let steps = vec![
        minimap_core::undo::Inverse::Archive(node(NodeType::Task, a.id)),
        minimap_core::undo::Inverse::Archive(node(NodeType::Task, b.id)),
    ];
    assert!(undo::apply(&mut conn, &steps).is_err());
    assert_eq!(fingerprint(&conn), before);
}

#[test]
fn putting_a_link_back_checks_for_loops_again() {
    let mut conn = demo();
    let a = task(&conn, "Book the leadership offsite");
    let b = task(&conn, "Review the Q2 headcount plan");
    let link = |from: Uuid, to: Uuid| NewEdge {
        edge_type: EdgeType::Blocks,
        from: node(NodeType::Task, from),
        to: node(NodeType::Task, to),
        attrs: serde_json::json!({}),
    };
    let edge = edges::add(&mut conn, link(a.id, b.id)).unwrap();
    let removed = wrote(&mut conn, |c| edges::remove(c, edge.id).unwrap());
    // The other way round is fine now...
    edges::add(&mut conn, link(b.id, a.id)).unwrap();
    // ...but bringing the first one back would close a loop.
    let err = undo::apply(&mut conn, &steps_of(&removed)).unwrap_err();
    assert!(
        matches!(&err, StoreError::Invalid(m) if m.contains("loop")),
        "{err}"
    );
}

#[test]
fn what_is_not_part_of_undo_is_told_apart() {
    let mut conn = demo();
    // A note edit and the mention links it syncs: nothing to remember.
    let note = notes::list(&conn, false)
        .unwrap()
        .into_iter()
        .find(|n| n.kind == NoteKind::General)
        .unwrap();
    let person = person(&conn, "Raj Patel");
    let rows = wrote(&mut conn, |c| {
        notes::update(
            c,
            note.id,
            UpdateNote {
                body: Some(format!(
                    "{} and more",
                    mention_token("Raj Patel", person.id)
                )),
                ..Default::default()
            },
        )
        .unwrap();
    });
    assert!(!rows.is_empty());
    assert_eq!(plan(&rows), Plan::Ignore);
    // A hard delete can't be taken back.
    let t = task(&conn, "Book the leadership offsite");
    let rows = wrote(&mut conn, |c| {
        nodes::archive(c, node(NodeType::Task, t.id)).unwrap();
    });
    assert!(matches!(plan(&rows), Plan::Steps(_)));
    let rows = wrote(&mut conn, |c| {
        nodes::delete(c, node(NodeType::Task, t.id)).unwrap();
    });
    assert!(matches!(plan(&rows), Plan::Irreversible(_)));
    // Changing a link's details can't be taken back either.
    let contribution = edges::list_active_of_type(&conn, EdgeType::ContributesTo)
        .unwrap()
        .remove(0);
    let rows = wrote(&mut conn, |c| {
        edges::update_attrs(c, contribution.id, serde_json::json!({"weight": 0.25})).unwrap();
    });
    assert!(matches!(plan(&rows), Plan::Irreversible(_)));
}

#[test]
fn undoing_a_creation_never_deletes_anything() {
    let mut conn = demo();
    let rows = wrote(&mut conn, |c| {
        tasks::create(
            c,
            CreateTask {
                links: Vec::new(),
                title: "Temp".into(),
                assignee: AssigneeChoice::Nobody,
                description: String::new(),
                project_id: None,
                status: None,
                estimate_days: None,
                start_date: None,
                due_date: None,
                priority: None,
                recurrence: None,
            },
        )
        .unwrap();
    });
    undo(&mut conn, &rows);
    // Still there, archived (so redo can bring it back and the history stays whole).
    let all = tasks::list(&conn, true).unwrap();
    assert!(all
        .iter()
        .any(|t| t.title == "Temp" && t.archived_at.is_some()));
}

#[test]
fn finishing_a_repeating_task_is_one_step_that_takes_the_next_one_back_too() {
    let mut conn = demo();
    let t = task(&conn, "Rotate service credentials");
    let weekly: Recurrence = Cadence::Weekly {
        every: 1,
        weekday: 0,
    }
    .into();
    tasks::update(
        &mut conn,
        t.id,
        UpdateTask {
            recurrence: Patch::Set(weekly.clone()),
            ..Default::default()
        },
    )
    .unwrap();
    let open_before = tasks::list(&conn, false).unwrap().len();
    round_trip(&mut conn, |c| {
        tasks::update(
            c,
            t.id,
            UpdateTask {
                status: Some(TaskStatus::Done),
                ..Default::default()
            },
        )
        .unwrap();
    });
    // Back to how it was: open, repeating again, and no extra task among the active ones.
    let back = tasks::get(&conn, t.id).unwrap();
    assert_eq!(
        (back.status, back.recurrence),
        (TaskStatus::Todo, Some(weekly))
    );
    assert_eq!(tasks::list(&conn, false).unwrap().len(), open_before);
}

#[test]
fn making_a_task_repeat_and_stopping_it_are_undone() {
    let mut conn = demo();
    let t = task(&conn, "Savings review with finance");
    let weekly: Recurrence = Cadence::Weekly {
        every: 1,
        weekday: 0,
    }
    .into();
    round_trip(&mut conn, |c| {
        tasks::update(
            c,
            t.id,
            UpdateTask {
                recurrence: Patch::Set(weekly.clone()),
                ..Default::default()
            },
        )
        .unwrap();
    });
    assert_eq!(tasks::get(&conn, t.id).unwrap().recurrence, None);
    // Stopping one that repeats.
    let monthly = task(&conn, "Monthly access review");
    assert!(monthly.recurrence.is_some());
    round_trip(&mut conn, |c| {
        tasks::update(
            c,
            monthly.id,
            UpdateTask {
                recurrence: Patch::Clear,
                ..Default::default()
            },
        )
        .unwrap();
    });
    assert_eq!(
        tasks::get(&conn, monthly.id).unwrap().recurrence,
        monthly.recurrence
    );
}

#[test]
fn adding_and_removing_reference_links_can_be_undone_and_redone() {
    let mut conn = demo();
    let t = task(&conn, "Rotate service credentials");
    let link = RefLink {
        title: "Runbook".into(),
        url: "https://docs.google.com/document/d/1".into(),
    };
    round_trip(&mut conn, |c| {
        tasks::update(
            c,
            t.id,
            UpdateTask {
                links: Some(vec![link.clone()]),
                ..Default::default()
            },
        )
        .unwrap();
    });
    assert!(tasks::get(&conn, t.id).unwrap().links.is_empty());
}

#[test]
fn creating_and_linking_subtasks_is_undone_and_redone() {
    let mut conn = demo();
    let parent = task(&conn, "Rotate service credentials");
    let other = task(&conn, "Monthly access review");
    round_trip(&mut conn, |c| {
        tasks::create_subtask(c, parent.id, "Audit old keys".into()).unwrap();
    });
    round_trip(&mut conn, |c| {
        tasks::set_parent(c, other.id, Some(parent.id)).unwrap();
    });
    // Moving it to another parent removes one link and adds one: still one step.
    tasks::set_parent(&mut conn, other.id, Some(parent.id)).unwrap();
    let third = task(&conn, "Go-live checklist");
    round_trip(&mut conn, |c| {
        tasks::set_parent(c, other.id, Some(third.id)).unwrap();
    });
}
