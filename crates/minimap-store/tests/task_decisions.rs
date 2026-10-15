//! Decision tasks and the decision log (spec 41): a task of type Decision has a record in the log,
//! made with it, kept when the task is finished, archived or changed to another type.

use minimap_store::*;
use minimap_types::*;
use time::macros::date;

fn db() -> Connection {
    open_in_memory().unwrap()
}

fn project(conn: &mut Connection) -> Project {
    projects::create(
        conn,
        CreateProject {
            title: "API launch".into(),
            slug: None,
            description: String::new(),
            owner_person_id: None,
            start_date: None,
            target_date: None,
            status: None,
            priority: None,
        },
    )
    .unwrap()
}

fn task(conn: &mut Connection, task_type: Option<&str>, project_id: Option<uuid::Uuid>) -> Task {
    let project_id = Some(project_id.unwrap_or_else(|| minimap_store::fixtures::project(conn)));
    tasks::create(
        conn,
        CreateTask {
            links: Vec::new(),
            task_type: task_type.map(str::to_owned),
            focus: None,
            start_minute: None,
            length_minutes: None,
            title: "Pick the data store".into(),
            assignee: AssigneeChoice::Nobody,
            description: "Postgres or Mongo?".into(),
            project_id,
            status: None,
            estimate_days: None,
            start_date: None,
            due_date: Some(date!(2027 - 03 - 01)),
            priority: None,
            recurrence: None,
        },
    )
    .unwrap()
}

fn set_type(conn: &mut Connection, id: uuid::Uuid, t: Patch<String>) {
    tasks::update(
        conn,
        id,
        UpdateTask {
            task_type: t,
            ..Default::default()
        },
    )
    .unwrap();
}

fn set_status(conn: &mut Connection, id: uuid::Uuid, status: TaskStatus) {
    tasks::update(
        conn,
        id,
        UpdateTask {
            status: Some(status),
            ..Default::default()
        },
    )
    .unwrap();
}

fn affects(conn: &Connection, decision: uuid::Uuid) -> Vec<Edge> {
    edges::list_for_node(conn, decision, false)
        .unwrap()
        .into_iter()
        .filter(|e| e.edge_type == EdgeType::Affects && e.from_id == decision)
        .collect()
}

#[test]
fn a_new_decision_task_gets_a_proposed_record_that_affects_it_and_its_project() {
    let mut conn = db();
    let p = project(&mut conn);
    let t = task(&mut conn, Some("decision"), Some(p.id));

    let d = decisions::for_task(&conn, t.id).unwrap().expect("a record");
    assert_eq!(d.title, "Pick the data store");
    assert_eq!(d.context, "Postgres or Mongo?");
    assert_eq!(d.status, DecisionStatus::Proposed);
    assert!(d.decided_on.is_none());

    let links = affects(&conn, d.id);
    assert_eq!(links.len(), 2);
    let to_task = links.iter().find(|e| e.to_id == t.id).unwrap();
    assert_eq!(to_task.attrs["origin"], "task");
    let to_project = links.iter().find(|e| e.to_id == p.id).unwrap();
    assert!(to_project.attrs.get("origin").is_none());

    // The ticket shows it.
    let detail = views::task_detail(&conn, t.id).unwrap();
    assert_eq!(detail.decision.map(|d| d.id), Some(d.id));
}

#[test]
fn other_types_and_no_type_make_no_record() {
    let mut conn = db();
    let a = task(&mut conn, None, None);
    let b = task(&mut conn, Some("design"), None);
    assert!(decisions::for_task(&conn, a.id).unwrap().is_none());
    assert!(decisions::for_task(&conn, b.id).unwrap().is_none());
    assert!(decisions::list(&conn, false).unwrap().is_empty());
    assert!(views::task_detail(&conn, a.id).unwrap().decision.is_none());
}

#[test]
fn choosing_the_type_later_makes_one_record_and_never_a_second() {
    let mut conn = db();
    let t = task(&mut conn, None, None);
    set_type(&mut conn, t.id, Patch::Set("decision".into()));
    let first = decisions::for_task(&conn, t.id).unwrap().unwrap();

    set_type(&mut conn, t.id, Patch::Set("design".into()));
    // Changing the type away keeps the record.
    assert_eq!(
        decisions::for_task(&conn, t.id).unwrap().map(|d| d.id),
        Some(first.id)
    );
    set_type(&mut conn, t.id, Patch::Set("decision".into()));
    set_type(&mut conn, t.id, Patch::Clear);
    set_type(&mut conn, t.id, Patch::Set("decision".into()));

    assert_eq!(decisions::list(&conn, false).unwrap().len(), 1);
    assert_eq!(
        decisions::for_task(&conn, t.id).unwrap().map(|d| d.id),
        Some(first.id)
    );
}

fn write_up(conn: &mut Connection, id: uuid::Uuid) {
    decisions::update(
        conn,
        id,
        UpdateDecision {
            decision: Some("Postgres".into()),
            rationale: Some("We know it".into()),
            ..Default::default()
        },
    )
    .unwrap();
}

#[test]
fn the_record_stays_when_the_task_is_finished_or_archived() {
    let mut conn = db();
    let t = task(&mut conn, Some("decision"), None);
    let d = decisions::for_task(&conn, t.id).unwrap().unwrap();
    write_up(&mut conn, d.id);

    set_status(&mut conn, t.id, TaskStatus::Done);
    let detail = views::task_detail(&conn, t.id).unwrap();
    assert_eq!(detail.decision.map(|d| d.id), Some(d.id));

    nodes::archive(&mut conn, NodeRef::new(NodeType::Task, t.id)).unwrap();
    let log = decisions::list(&conn, false).unwrap();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].id, d.id);
    assert_eq!(log[0].status, DecisionStatus::Decided);
    assert_eq!(log[0].decision, "Postgres");
}

#[test]
fn a_decision_task_cannot_be_finished_until_the_decision_and_the_reason_are_written() {
    let mut conn = db();
    let t = task(&mut conn, Some("decision"), None);
    let finish = |c: &mut Connection| {
        tasks::update(
            c,
            t.id,
            UpdateTask {
                status: Some(TaskStatus::Done),
                ..Default::default()
            },
        )
    };
    assert!(matches!(
        finish(&mut conn),
        Err(StoreError::DecisionIncomplete(m)) if m.contains("Pick the data store")
    ));
    let d = decisions::for_task(&conn, t.id).unwrap().unwrap();
    // One of the two is not enough, and blanks do not count.
    decisions::update(
        &mut conn,
        d.id,
        UpdateDecision {
            decision: Some("Postgres".into()),
            rationale: Some("   ".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(finish(&mut conn).is_err());
    // Nothing changed: the task is still open and the record still proposed.
    assert_eq!(tasks::get(&conn, t.id).unwrap().status, TaskStatus::Todo);
    // Cancelling is not held up, and neither is a task of another type.
    set_status(&mut conn, t.id, TaskStatus::Cancelled);
    let other = task(&mut conn, Some("design"), None);
    set_status(&mut conn, other.id, TaskStatus::Done);
}

#[test]
fn finishing_decides_the_record_and_says_it_was_automatic() {
    let mut conn = db();
    let t = task(&mut conn, Some("decision"), None);
    let d = decisions::for_task(&conn, t.id).unwrap().unwrap();
    write_up(&mut conn, d.id);
    set_status(&mut conn, t.id, TaskStatus::Done);

    let d = decisions::get(&conn, d.id).unwrap();
    assert_eq!(d.status, DecisionStatus::Decided);
    assert_eq!(d.decided_on, Some(today()));
    assert!(d.auto_status && d.auto_decided_on);

    // Setting either by hand takes the automatic mark off it.
    decisions::update(
        &mut conn,
        d.id,
        UpdateDecision {
            decided_on: Patch::Set(date!(2027 - 03 - 01)),
            ..Default::default()
        },
    )
    .unwrap();
    let d = decisions::get(&conn, d.id).unwrap();
    assert!(d.auto_status && !d.auto_decided_on);
    decisions::update(
        &mut conn,
        d.id,
        UpdateDecision {
            status: Some(DecisionStatus::Superseded),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!decisions::get(&conn, d.id).unwrap().auto_status);
}

#[test]
fn what_the_user_set_is_not_overwritten_when_the_task_is_finished() {
    let mut conn = db();
    // A date given by the user stays theirs; the status is still decided automatically.
    let t = task(&mut conn, Some("decision"), None);
    let d = decisions::for_task(&conn, t.id).unwrap().unwrap();
    write_up(&mut conn, d.id);
    decisions::update(
        &mut conn,
        d.id,
        UpdateDecision {
            decided_on: Patch::Set(date!(2027 - 02 - 20)),
            ..Default::default()
        },
    )
    .unwrap();
    set_status(&mut conn, t.id, TaskStatus::Done);
    let d = decisions::get(&conn, d.id).unwrap();
    assert_eq!(d.decided_on, Some(date!(2027 - 02 - 20)));
    assert!(d.auto_status && !d.auto_decided_on);

    // A record the user already decided is left entirely alone.
    let t2 = task(&mut conn, Some("decision"), None);
    let d2 = decisions::for_task(&conn, t2.id).unwrap().unwrap();
    write_up(&mut conn, d2.id);
    decisions::update(
        &mut conn,
        d2.id,
        UpdateDecision {
            status: Some(DecisionStatus::Decided),
            decided_on: Patch::Set(date!(2027 - 01 - 05)),
            ..Default::default()
        },
    )
    .unwrap();
    set_status(&mut conn, t2.id, TaskStatus::Done);
    let d2 = decisions::get(&conn, d2.id).unwrap();
    assert_eq!(d2.decided_on, Some(date!(2027 - 01 - 05)));
    assert!(!d2.auto_status && !d2.auto_decided_on);
}

#[test]
fn deciding_without_a_date_stamps_today_and_says_so() {
    let mut conn = db();
    let d = decisions::create(
        &mut conn,
        CreateDecision {
            title: "Use Postgres".into(),
            context: String::new(),
            decision: String::new(),
            rationale: String::new(),
            decided_on: None,
            status: None,
        },
    )
    .unwrap();
    assert!(!d.auto_decided_on);
    let d = decisions::update(
        &mut conn,
        d.id,
        UpdateDecision {
            status: Some(DecisionStatus::Decided),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(d.decided_on, Some(today()));
    assert!(d.auto_decided_on && !d.auto_status);
}

#[test]
fn the_finish_dialog_writes_the_record_and_finishes_the_task_in_one_step() {
    let mut conn = db();
    let t = task(&mut conn, Some("decision"), None);
    assert!(decisions::finish_task(&mut conn, t.id, " ".into(), "why".into()).is_err());
    assert_eq!(tasks::get(&conn, t.id).unwrap().status, TaskStatus::Todo);

    let done =
        decisions::finish_task(&mut conn, t.id, "Postgres".into(), "We know it".into()).unwrap();
    assert_eq!(done.status, TaskStatus::Done);
    let d = decisions::for_task(&conn, t.id).unwrap().unwrap();
    assert_eq!(d.decision, "Postgres");
    assert_eq!(d.status, DecisionStatus::Decided);
    assert!(d.auto_status);

    // A task that is not a decision is refused.
    let plain = task(&mut conn, Some("design"), None);
    assert!(decisions::finish_task(&mut conn, plain.id, "a".into(), "b".into()).is_err());
}

#[test]
fn a_decision_task_with_no_record_is_finished_through_the_dialog() {
    let mut conn = db();
    let t = task(&mut conn, Some("decision"), None);
    let d = decisions::for_task(&conn, t.id).unwrap().unwrap();
    nodes::archive(&mut conn, NodeRef::new(NodeType::Decision, d.id)).unwrap();
    assert!(tasks::update(
        &mut conn,
        t.id,
        UpdateTask {
            status: Some(TaskStatus::Done),
            ..Default::default()
        }
    )
    .is_err());
    decisions::finish_task(&mut conn, t.id, "Postgres".into(), "We know it".into()).unwrap();
    assert_eq!(
        decisions::for_task(&conn, t.id).unwrap().unwrap().status,
        DecisionStatus::Decided
    );
}

#[test]
fn an_archived_record_is_replaced_by_the_button_and_other_tasks_are_refused() {
    let mut conn = db();
    let t = task(&mut conn, Some("decision"), None);
    let old = decisions::for_task(&conn, t.id).unwrap().unwrap();
    // Already there: the button returns it.
    assert_eq!(
        decisions::ensure_for_task(&mut conn, t.id).unwrap().id,
        old.id
    );

    nodes::archive(&mut conn, NodeRef::new(NodeType::Decision, old.id)).unwrap();
    assert!(decisions::for_task(&conn, t.id).unwrap().is_none());
    let fresh = decisions::ensure_for_task(&mut conn, t.id).unwrap();
    assert_ne!(fresh.id, old.id);

    let plain = task(&mut conn, Some("design"), None);
    assert!(decisions::ensure_for_task(&mut conn, plain.id).is_err());
}

#[test]
fn a_decision_that_merely_affects_a_task_is_not_its_record() {
    let mut conn = db();
    let t = task(&mut conn, Some("decision"), None);
    let other = decisions::create(
        &mut conn,
        CreateDecision {
            title: "Use Postgres".into(),
            context: String::new(),
            decision: String::new(),
            rationale: String::new(),
            decided_on: None,
            status: None,
        },
    )
    .unwrap();
    edges::add(
        &mut conn,
        NewEdge {
            edge_type: EdgeType::Affects,
            from: NodeRef::new(NodeType::Decision, other.id),
            to: NodeRef::new(NodeType::Task, t.id),
            attrs: serde_json::json!({}),
        },
    )
    .unwrap();
    let record = decisions::for_task(&conn, t.id).unwrap().unwrap();
    assert_ne!(record.id, other.id);
}
