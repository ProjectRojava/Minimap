//! Two of a task's dates and estimate give the rest: a start date and an estimate give the due
//! date, a start date and a due date give the estimate, in working days. The start is never
//! filled in from the due date and the estimate. Only the person's own changes do it, in the
//! same change, and a field the change clears stays empty.

use minimap_store::*;
use minimap_types::*;
use time::macros::date;

fn make(
    conn: &mut Connection,
    start: Option<time::Date>,
    due: Option<time::Date>,
    estimate: Option<f64>,
) -> Task {
    let project = fixtures::project(conn);
    tasks::create(
        conn,
        CreateTask {
            links: Vec::new(),
            task_type: Some("build".into()),
            focus: None,
            backburner_since: None,
            start_minute: None,
            length_minutes: None,
            title: "Build it".into(),
            assignee: AssigneeChoice::Nobody,
            description: String::new(),
            project_id: Some(project),
            status: None,
            estimate_days: estimate,
            start_date: start,
            due_date: due,
            priority: None,
            recurrence: None,
        },
    )
    .unwrap()
}

// Monday 2027-03-01 and Friday 2027-03-05.
const MON: time::Date = date!(2027 - 03 - 01);
const FRI: time::Date = date!(2027 - 03 - 05);

#[test]
fn a_new_task_gets_its_due_date_or_estimate_from_the_other_two() {
    let mut conn = open_in_memory().unwrap();
    let t = make(&mut conn, Some(MON), None, Some(5.0));
    assert_eq!(t.due_date, Some(FRI));
    let t = make(&mut conn, Some(MON), Some(FRI), None);
    assert_eq!(t.estimate_days, Some(5.0));
    // Due date and estimate: the start stays empty, so it starts as soon as it can.
    let t = make(&mut conn, None, Some(FRI), Some(3.0));
    assert_eq!(t.start_date, None);
    // All three, or only one: nothing changes.
    let t = make(&mut conn, Some(MON), Some(FRI), Some(2.0));
    assert_eq!(t.estimate_days, Some(2.0));
    let t = make(&mut conn, Some(MON), None, None);
    assert_eq!((t.due_date, t.estimate_days), (None, None));
}

#[test]
fn typing_a_date_or_an_estimate_fills_the_other_in_the_same_change() {
    let mut conn = open_in_memory().unwrap();
    let t = make(&mut conn, Some(MON), None, None);
    let before = activity::list_for_node(&conn, t.id).unwrap().len();
    let t = tasks::update(
        &mut conn,
        t.id,
        UpdateTask {
            estimate_days: Patch::Set(3.0),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(t.due_date, Some(date!(2027 - 03 - 03)));
    // One activity row with both fields, so one undo takes both back.
    let rows = activity::list_for_node(&conn, t.id).unwrap();
    assert_eq!(rows.len(), before + 1);
    let diff = &rows.iter().max_by_key(|r| r.at).unwrap().diff;
    assert!(
        diff.get("estimate_days").is_some() && diff.get("due_date").is_some(),
        "{diff}"
    );

    // Start and due set, estimate empty: typing a new due date gives the estimate.
    let u = make(&mut conn, Some(MON), None, None);
    let u = tasks::update(
        &mut conn,
        u.id,
        UpdateTask {
            due_date: Patch::Set(date!(2027 - 03 - 09)),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(u.estimate_days, Some(7.0));
}

#[test]
fn clearing_a_field_or_an_unrelated_change_fills_nothing() {
    let mut conn = open_in_memory().unwrap();
    let t = make(&mut conn, Some(MON), Some(FRI), Some(5.0));
    // Clearing the estimate leaves it empty, though start and due could give it.
    let t = tasks::update(
        &mut conn,
        t.id,
        UpdateTask {
            estimate_days: Patch::Clear,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(t.estimate_days, None);
    // A change that sets none of the three fills nothing either.
    let t = tasks::update(
        &mut conn,
        t.id,
        UpdateTask {
            title: Some("Build it well".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(t.estimate_days, None);
    // Changing one when all three are set changes only that one.
    let t = tasks::update(
        &mut conn,
        t.id,
        UpdateTask {
            estimate_days: Patch::Set(2.0),
            ..Default::default()
        },
    )
    .unwrap();
    let t = tasks::update(
        &mut conn,
        t.id,
        UpdateTask {
            due_date: Patch::Set(date!(2027 - 03 - 12)),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        (t.start_date, t.due_date, t.estimate_days),
        (Some(MON), Some(date!(2027 - 03 - 12)), Some(2.0))
    );
}
