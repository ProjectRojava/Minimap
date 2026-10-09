//! Focus on tasks (spec 37): stored with the task, kept by a repeating task's next one only when
//! it has no end, and never able to stop a task loading.

use minimap_store::*;
use minimap_types::*;
use time::macros::date;
use uuid::Uuid;

fn db() -> Connection {
    open_in_memory().unwrap()
}

fn task(conn: &mut Connection, f: impl FnOnce(&mut CreateTask)) -> Task {
    let mut input = CreateTask {
        links: Vec::new(),
        task_type: None,
        focus: None,
        start_minute: None,
        length_minutes: None,
        title: "Renew the licence".into(),
        assignee: AssigneeChoice::Nobody,
        description: String::new(),
        project_id: None,
        status: None,
        estimate_days: None,
        start_date: None,
        due_date: Some(date!(2027 - 09 - 30)),
        priority: None,
        recurrence: None,
    };
    f(&mut input);
    tasks::create(conn, input).unwrap()
}

fn set_focus(conn: &mut Connection, id: Uuid, focus: Patch<Focus>) -> Task {
    tasks::update(
        conn,
        id,
        UpdateTask {
            focus,
            ..Default::default()
        },
    )
    .unwrap()
}

fn finish(conn: &mut Connection, id: Uuid) {
    tasks::update(
        conn,
        id,
        UpdateTask {
            status: Some(TaskStatus::Done),
            ..Default::default()
        },
    )
    .unwrap();
}

#[test]
fn a_new_task_is_not_in_focus_and_can_be_made_so_when_created() {
    let mut conn = db();
    assert_eq!(task(&mut conn, |_| {}).focus, None);
    let t = task(&mut conn, |t| t.focus = Some(Focus::PINNED));
    assert_eq!(t.focus, Some(Focus::PINNED));
    assert_eq!(tasks::get(&conn, t.id).unwrap().focus, Some(Focus::PINNED));
}

#[test]
fn focus_is_set_changed_and_cleared_and_each_change_is_logged() {
    let mut conn = db();
    let t = task(&mut conn, |_| {});
    let dated = Focus {
        until: Some(date!(2027 - 04 - 30)),
    };
    assert_eq!(
        set_focus(&mut conn, t.id, Patch::Set(Focus::PINNED)).focus,
        Some(Focus::PINNED)
    );
    assert_eq!(
        set_focus(&mut conn, t.id, Patch::Set(dated)).focus,
        Some(dated)
    );
    assert_eq!(tasks::get(&conn, t.id).unwrap().focus, Some(dated));
    assert_eq!(set_focus(&mut conn, t.id, Patch::Clear).focus, None);
    assert_eq!(tasks::get(&conn, t.id).unwrap().focus, None);
    let changes = activity::list_for_node(&conn, t.id)
        .unwrap()
        .into_iter()
        .filter(|a| a.action == ActivityAction::Updated && a.diff.to_string().contains("focus"))
        .count();
    assert_eq!(changes, 3);
}

#[test]
fn focus_does_not_touch_the_dates_status_or_priority() {
    let mut conn = db();
    let t = task(&mut conn, |_| {});
    let focused = set_focus(&mut conn, t.id, Patch::Set(Focus::PINNED));
    assert_eq!(focused.due_date, t.due_date);
    assert_eq!(focused.start_date, t.start_date);
    assert_eq!(focused.status, t.status);
    assert_eq!(focused.priority, t.priority);
}

#[test]
fn a_repeating_task_hands_a_pin_on_but_not_a_focus_with_an_end() {
    let mut conn = db();
    let weekly: Recurrence = Cadence::Weekly {
        every: 1,
        weekday: 0,
    }
    .into();
    let pinned = task(&mut conn, |t| {
        t.recurrence = Some(weekly.clone());
        t.focus = Some(Focus::PINNED);
    });
    finish(&mut conn, pinned.id);
    let next = tasks::list(&conn, false)
        .unwrap()
        .into_iter()
        .find(|t| t.title == pinned.title && t.id != pinned.id)
        .expect("the next one");
    assert_eq!(next.focus, Some(Focus::PINNED));

    let ended = task(&mut conn, |t| {
        t.title = "Quarter close".into();
        t.recurrence = Some(weekly.clone());
        t.focus = Some(Focus {
            until: Some(date!(2027 - 04 - 30)),
        });
    });
    finish(&mut conn, ended.id);
    let next = tasks::list(&conn, false)
        .unwrap()
        .into_iter()
        .find(|t| t.title == "Quarter close" && t.id != ended.id)
        .expect("the next one");
    assert_eq!(next.focus, None);
}

#[test]
fn a_damaged_stored_focus_does_not_stop_the_task_loading() {
    let mut conn = db();
    let t = task(&mut conn, |_| {});
    conn.execute(
        "UPDATE tasks SET focus = '{\"until\":\"someday\"}' WHERE id = ?1",
        [t.id.to_string()],
    )
    .unwrap();
    assert_eq!(tasks::get(&conn, t.id).unwrap().focus, None);
}
