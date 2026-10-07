//! Reference links on tasks (spec 28): stored with the task, tidied and checked on the way in,
//! carried to the next task of a repeating series.

use minimap_store::*;
use minimap_types::*;
use time::macros::date;

fn db() -> Connection {
    open_in_memory().unwrap()
}

fn link(title: &str, url: &str) -> RefLink {
    RefLink {
        title: title.into(),
        url: url.into(),
    }
}

fn task(conn: &mut Connection, links: Vec<RefLink>) -> Result<Task> {
    tasks::create(
        conn,
        CreateTask {
            links,
            title: "Fetch company details".into(),
            assignee: AssigneeChoice::Nobody,
            description: String::new(),
            project_id: None,
            status: None,
            estimate_days: None,
            start_date: None,
            due_date: Some(date!(2027 - 03 - 01)),
            priority: None,
            recurrence: None,
        },
    )
}

fn set_links(conn: &mut Connection, id: uuid::Uuid, links: Vec<RefLink>) -> Result<Task> {
    tasks::update(
        conn,
        id,
        UpdateTask {
            links: Some(links),
            ..Default::default()
        },
    )
}

#[test]
fn a_new_task_has_no_links_and_links_survive_a_reload() {
    let mut conn = db();
    let plain = task(&mut conn, Vec::new()).unwrap();
    assert!(tasks::get(&conn, plain.id).unwrap().links.is_empty());

    let t = task(
        &mut conn,
        vec![link("Spec", "docs.google.com/document/d/1")],
    )
    .unwrap();
    assert_eq!(
        tasks::get(&conn, t.id).unwrap().links,
        vec![link("Spec", "https://docs.google.com/document/d/1")]
    );
}

#[test]
fn updating_replaces_the_list_and_other_edits_leave_it_alone() {
    let mut conn = db();
    let t = task(&mut conn, vec![link("A", "https://a.co")]).unwrap();
    let after = set_links(
        &mut conn,
        t.id,
        vec![link("A", "https://a.co"), link(" B  site ", "b.co/x")],
    )
    .unwrap();
    assert_eq!(
        after.links,
        vec![link("A", "https://a.co"), link("B site", "https://b.co/x")]
    );
    // A different edit does not touch them.
    let renamed = tasks::update(
        &mut conn,
        t.id,
        UpdateTask {
            title: Some("New title".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(renamed.links, after.links);
    // Clearing the list is a plain empty list.
    assert!(set_links(&mut conn, t.id, Vec::new())
        .unwrap()
        .links
        .is_empty());
}

#[test]
fn addresses_that_are_not_web_or_email_are_refused_and_nothing_changes() {
    let mut conn = db();
    let t = task(&mut conn, vec![link("A", "https://a.co")]).unwrap();
    for bad in ["javascript:alert(1)", "file:///etc/passwd", "nonsense"] {
        let err = set_links(&mut conn, t.id, vec![link("x", bad)]).unwrap_err();
        assert!(matches!(err, StoreError::Invalid(_)), "{bad}");
        assert!(task(&mut conn, vec![link("x", bad)]).is_err(), "{bad}");
    }
    assert_eq!(
        tasks::get(&conn, t.id).unwrap().links,
        vec![link("A", "https://a.co")]
    );
}

#[test]
fn changing_the_links_is_written_to_the_activity_log() {
    let mut conn = db();
    let t = task(&mut conn, Vec::new()).unwrap();
    set_links(&mut conn, t.id, vec![link("A", "https://a.co")]).unwrap();
    let rows = activity::list_for_node(&conn, t.id).unwrap();
    assert!(rows
        .iter()
        .any(|a| a.action == ActivityAction::Updated && a.diff.get("links").is_some()));
}

#[test]
fn the_next_task_of_a_repeating_series_keeps_the_links() {
    let mut conn = db();
    let t = task(&mut conn, vec![link("Runbook", "https://a.co/runbook")]).unwrap();
    tasks::update(
        &mut conn,
        t.id,
        UpdateTask {
            recurrence: Patch::Set(
                Cadence::Weekly {
                    every: 1,
                    weekday: 0,
                }
                .into(),
            ),
            ..Default::default()
        },
    )
    .unwrap();
    tasks::update(
        &mut conn,
        t.id,
        UpdateTask {
            status: Some(TaskStatus::Done),
            ..Default::default()
        },
    )
    .unwrap();
    let next = tasks::list(&conn, false)
        .unwrap()
        .into_iter()
        .find(|n| n.id != t.id)
        .unwrap();
    assert_eq!(next.links, vec![link("Runbook", "https://a.co/runbook")]);
}
