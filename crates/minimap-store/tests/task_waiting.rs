//! Waiting on, from the task (spec 56): a task's waiting-ons are the active ones about it, made
//! with their link in one step, resolved together when asked, and left standing when it goes.

use minimap_store::*;
use minimap_types::*;
use time::macros::date;
use uuid::Uuid;

const TODAY: time::Date = date!(2027 - 03 - 15);

fn db() -> Connection {
    open_in_memory().unwrap()
}

fn person(conn: &mut Connection, name: &str) -> Uuid {
    people::create(
        conn,
        CreatePerson {
            name: name.into(),
            role_title: String::new(),
            email: None,
            weekly_capacity_hours: None,
            is_self: false,
            notes: String::new(),
        },
    )
    .unwrap()
    .id
}

fn task(conn: &mut Connection, title: &str) -> Uuid {
    let project_id = Some(fixtures::project(conn));
    tasks::create(
        conn,
        CreateTask {
            links: Vec::new(),
            task_type: None,
            focus: None,
            start_minute: None,
            length_minutes: None,
            title: title.into(),
            assignee: AssigneeChoice::Nobody,
            description: String::new(),
            project_id,
            status: None,
            estimate_days: None,
            start_date: None,
            due_date: None,
            priority: None,
            recurrence: None,
        },
    )
    .unwrap()
    .id
}

fn wait(person_id: Uuid, what: &str) -> CreateWaitingOn {
    CreateWaitingOn {
        description: what.into(),
        person_id,
        asked_on: Some(date!(2027 - 03 - 01)),
        expected_by: None,
        follow_up_on: None,
    }
}

#[test]
fn creating_for_a_task_makes_the_waiting_on_and_its_link_together() {
    let mut conn = db();
    let raj = person(&mut conn, "Raj");
    let t = task(&mut conn, "Ship it");
    let w = waiting_on::create_for_task(&mut conn, t, wait(raj, "Sign-off")).unwrap();
    let links = edges::list_for_node(&conn, w.id, false).unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].edge_type, EdgeType::About);
    assert_eq!(links[0].to_id, t);
    let mine = waiting_on::for_task(&conn, t).unwrap();
    assert_eq!(mine.iter().map(|x| x.id).collect::<Vec<_>>(), vec![w.id]);
}

#[test]
fn a_blank_description_or_an_archived_task_makes_nothing() {
    let mut conn = db();
    let raj = person(&mut conn, "Raj");
    let t = task(&mut conn, "Ship it");
    assert!(waiting_on::create_for_task(&mut conn, t, wait(raj, "  ")).is_err());
    assert!(waiting_on::list(&conn, true).unwrap().is_empty());
    nodes::archive(&mut conn, NodeRef::new(NodeType::Task, t)).unwrap();
    assert!(waiting_on::create_for_task(&mut conn, t, wait(raj, "Sign-off")).is_err());
    assert!(waiting_on::list(&conn, true).unwrap().is_empty());
    assert!(waiting_on::create_for_task(&mut conn, Uuid::now_v7(), wait(raj, "x")).is_err());
}

#[test]
fn a_task_has_exactly_the_active_waiting_ons_about_it() {
    let mut conn = db();
    let raj = person(&mut conn, "Raj");
    let t = task(&mut conn, "Ship it");
    let other = task(&mut conn, "Other");
    let a = waiting_on::create_for_task(&mut conn, t, wait(raj, "A")).unwrap();
    let b = waiting_on::create_for_task(&mut conn, t, wait(raj, "B")).unwrap();
    waiting_on::create_for_task(&mut conn, other, wait(raj, "C")).unwrap();
    // One about a project, one with no link, one made from the Links editor (about the task).
    let project = fixtures::project(&conn);
    let about_project = waiting_on::create(&mut conn, wait(raj, "P")).unwrap();
    edges::add(
        &mut conn,
        NewEdge {
            edge_type: EdgeType::About,
            from: NodeRef::new(NodeType::WaitingOn, about_project.id),
            to: NodeRef::new(NodeType::Project, project),
            attrs: serde_json::json!({}),
        },
    )
    .unwrap();
    waiting_on::create(&mut conn, wait(raj, "loose")).unwrap();
    let by_hand = waiting_on::create(&mut conn, wait(raj, "by hand")).unwrap();
    edges::add(
        &mut conn,
        NewEdge {
            edge_type: EdgeType::About,
            from: NodeRef::new(NodeType::WaitingOn, by_hand.id),
            to: NodeRef::new(NodeType::Task, t),
            attrs: serde_json::json!({}),
        },
    )
    .unwrap();
    let ids = |conn: &Connection| -> Vec<Uuid> {
        let mut v: Vec<Uuid> = waiting_on::for_task(conn, t)
            .unwrap()
            .iter()
            .map(|w| w.id)
            .collect();
        v.sort();
        v
    };
    let mut expected = vec![a.id, b.id, by_hand.id];
    expected.sort();
    assert_eq!(ids(&conn), expected);

    // An archived waiting-on, and a removed link, are not the task's any more.
    nodes::archive(&mut conn, NodeRef::new(NodeType::WaitingOn, a.id)).unwrap();
    let link = edges::list_for_node(&conn, b.id, false).unwrap()[0].id;
    edges::remove(&mut conn, link).unwrap();
    assert_eq!(ids(&conn), vec![by_hand.id]);
}

#[test]
fn resolving_for_a_task_resolves_open_ones_and_keeps_earlier_dates() {
    let mut conn = db();
    let raj = person(&mut conn, "Raj");
    let t = task(&mut conn, "Ship it");
    let a = waiting_on::create_for_task(&mut conn, t, wait(raj, "A")).unwrap();
    let b = waiting_on::create_for_task(&mut conn, t, wait(raj, "B")).unwrap();
    waiting_on::update(
        &mut conn,
        b.id,
        UpdateWaitingOn {
            resolved_on: Patch::Set(date!(2027 - 03 - 05)),
            ..Default::default()
        },
    )
    .unwrap();
    let done = waiting_on::resolve_for_task(&mut conn, t, TODAY).unwrap();
    assert_eq!(done.iter().map(|w| w.id).collect::<Vec<_>>(), vec![a.id]);
    assert_eq!(
        waiting_on::get(&conn, a.id).unwrap().resolved_on,
        Some(TODAY)
    );
    assert_eq!(
        waiting_on::get(&conn, b.id).unwrap().resolved_on,
        Some(date!(2027 - 03 - 05))
    );
    // Nothing open: nothing to do.
    assert!(waiting_on::resolve_for_task(&mut conn, t, TODAY)
        .unwrap()
        .is_empty());
}

#[test]
fn archiving_the_task_leaves_its_waiting_ons_open_and_unlinked() {
    let mut conn = db();
    let raj = person(&mut conn, "Raj");
    let t = task(&mut conn, "Ship it");
    let w = waiting_on::create_for_task(&mut conn, t, wait(raj, "A")).unwrap();
    nodes::archive(&mut conn, NodeRef::new(NodeType::Task, t)).unwrap();
    let still = waiting_on::get(&conn, w.id).unwrap();
    assert!(still.archived_at.is_none() && still.resolved_on.is_none());
    assert!(edges::list_for_node(&conn, w.id, false).unwrap().is_empty());
}

#[test]
fn task_rows_carry_the_badge() {
    let mut conn = db();
    let raj = person(&mut conn, "Raj");
    let t = task(&mut conn, "Ship it");
    let quiet = task(&mut conn, "Quiet");
    let badge_of = |conn: &Connection, id: Uuid| {
        views::task_rows(conn)
            .unwrap()
            .into_iter()
            .find(|r| r.task.id == id)
            .unwrap()
            .waiting
    };
    assert!(badge_of(&conn, t).is_none());
    // The store reads the real clock, so the age is made relative to it.
    let mut old = wait(raj, "A");
    old.asked_on = Some(today() - time::Duration::days(30));
    let w = waiting_on::create_for_task(&mut conn, t, old).unwrap();
    let b = badge_of(&conn, t).unwrap();
    assert_eq!(b.entries.len(), 1);
    assert_eq!(b.entries[0].person, "Raj");
    assert!(b.stale, "asked 30 days ago is past the stale limit");
    assert!(badge_of(&conn, quiet).is_none());
    waiting_on::update(
        &mut conn,
        w.id,
        UpdateWaitingOn {
            resolved_on: Patch::Set(TODAY),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(badge_of(&conn, t).is_none());
}
