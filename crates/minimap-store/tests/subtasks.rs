//! Subtasks (spec 29): a task can be the child of another, made from the parent or linked.

use minimap_store::*;
use minimap_types::*;

fn db() -> Connection {
    open_in_memory().unwrap()
}

fn task(conn: &mut Connection, title: &str) -> Task {
    tasks::create(
        conn,
        CreateTask {
            links: Vec::new(),
            title: title.into(),
            assignee: AssigneeChoice::Nobody,
            description: String::new(),
            project_id: None,
            status: None,
            estimate_days: None,
            start_date: None,
            due_date: None,
            priority: Some(2),
            recurrence: None,
        },
    )
    .unwrap()
}

fn finish(conn: &mut Connection, id: uuid::Uuid, status: TaskStatus) {
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

#[test]
fn a_created_subtask_joins_the_parents_project_and_priority() {
    let mut conn = db();
    let project = projects::create(
        &mut conn,
        CreateProject {
            title: "Launch".into(),
            slug: None,
            description: String::new(),
            owner_person_id: None,
            start_date: None,
            target_date: None,
            status: None,
            priority: None,
        },
    )
    .unwrap();
    let parent = tasks::create(
        &mut conn,
        CreateTask {
            links: Vec::new(),
            title: "Parent".into(),
            assignee: AssigneeChoice::Nobody,
            description: String::new(),
            project_id: Some(project.id),
            status: None,
            estimate_days: None,
            start_date: None,
            due_date: None,
            priority: Some(1),
            recurrence: None,
        },
    )
    .unwrap();
    let child = tasks::create_subtask(&mut conn, parent.id, "Child".into()).unwrap();
    assert_eq!(child.project_id, Some(project.id));
    assert_eq!(child.priority, 1);
    let detail = views::task_detail(&conn, parent.id).unwrap();
    assert_eq!(detail.subtasks.len(), 1);
    assert_eq!(detail.subtasks[0].node.label, "Child");
    let back = views::task_detail(&conn, child.id).unwrap();
    assert_eq!(back.parent.unwrap().label, "Parent");
}

#[test]
fn linking_replaces_the_parent_and_none_frees_the_task() {
    let mut conn = db();
    let (a, b, c) = (
        task(&mut conn, "A"),
        task(&mut conn, "B"),
        task(&mut conn, "C"),
    );
    tasks::set_parent(&mut conn, c.id, Some(a.id)).unwrap();
    tasks::set_parent(&mut conn, c.id, Some(b.id)).unwrap();
    assert!(views::task_detail(&conn, a.id).unwrap().subtasks.is_empty());
    assert_eq!(views::task_detail(&conn, b.id).unwrap().subtasks.len(), 1);
    // The same parent again changes nothing (and writes nothing).
    let before = activity::count(&conn).unwrap();
    tasks::set_parent(&mut conn, c.id, Some(b.id)).unwrap();
    assert_eq!(activity::count(&conn).unwrap(), before);
    tasks::set_parent(&mut conn, c.id, None).unwrap();
    assert!(views::task_detail(&conn, c.id).unwrap().parent.is_none());
}

#[test]
fn lists_show_the_parent_and_how_many_subtasks_are_done_cancelled_ones_not_counted() {
    let mut conn = db();
    let parent = task(&mut conn, "Parent");
    let kids: Vec<Task> = ["one", "two", "three", "four"]
        .iter()
        .map(|t| tasks::create_subtask(&mut conn, parent.id, (*t).into()).unwrap())
        .collect();
    finish(&mut conn, kids[0].id, TaskStatus::Done);
    finish(&mut conn, kids[1].id, TaskStatus::Done);
    finish(&mut conn, kids[3].id, TaskStatus::Cancelled);
    let rows = views::task_rows(&conn).unwrap();
    let row = |id| rows.iter().find(|r| r.task.id == id).unwrap();
    assert_eq!(
        row(parent.id).subtasks,
        SubtaskProgress { done: 2, total: 3 }
    );
    assert_eq!(row(kids[2].id).parent.as_ref().unwrap().label, "Parent");
    assert!(row(parent.id).parent.is_none());
    assert_eq!(row(kids[2].id).subtasks, SubtaskProgress::default());
}

#[test]
fn archiving_the_parent_frees_its_subtasks() {
    let mut conn = db();
    let parent = task(&mut conn, "Parent");
    let child = tasks::create_subtask(&mut conn, parent.id, "Child".into()).unwrap();
    nodes::archive(&mut conn, NodeRef::new(NodeType::Task, parent.id)).unwrap();
    assert!(views::task_detail(&conn, child.id)
        .unwrap()
        .parent
        .is_none());
}

#[test]
fn an_archived_task_cannot_get_subtasks() {
    let mut conn = db();
    let parent = task(&mut conn, "Parent");
    nodes::archive(&mut conn, NodeRef::new(NodeType::Task, parent.id)).unwrap();
    assert!(tasks::create_subtask(&mut conn, parent.id, "x".into()).is_err());
}

fn block(conn: &mut Connection, from: uuid::Uuid, to: uuid::Uuid) {
    edges::add(
        conn,
        NewEdge {
            edge_type: EdgeType::Blocks,
            from: NodeRef::new(NodeType::Task, from),
            to: NodeRef::new(NodeType::Task, to),
            attrs: serde_json::json!({}),
        },
    )
    .unwrap();
}

#[test]
fn subtasks_come_in_order_with_what_they_follow_and_the_next_step() {
    let mut conn = db();
    let parent = task(&mut conn, "Launch");
    let ship = tasks::create_subtask(&mut conn, parent.id, "Ship".into()).unwrap();
    let build = tasks::create_subtask(&mut conn, parent.id, "Build".into()).unwrap();
    let test = tasks::create_subtask(&mut conn, parent.id, "Test".into()).unwrap();
    // Ship is the oldest but goes last: Build, Test, Ship.
    block(&mut conn, build.id, test.id);
    block(&mut conn, test.id, ship.id);
    let d = views::task_detail(&conn, parent.id).unwrap();
    let names: Vec<&str> = d.subtasks.iter().map(|s| s.node.label.as_str()).collect();
    assert_eq!(names, ["Build", "Test", "Ship"]);
    let flags: Vec<(bool, bool)> = d.subtasks.iter().map(|s| (s.next, s.waiting)).collect();
    assert_eq!(flags, [(true, false), (false, true), (false, true)]);
    assert_eq!(d.subtasks[1].after.len(), 1);
    assert_eq!(d.subtasks[1].after[0].node.label, "Build");
    assert!(d.subtasks[0].after.is_empty());

    // Finishing Build moves the next step on.
    finish(&mut conn, build.id, TaskStatus::Done);
    let d = views::task_detail(&conn, parent.id).unwrap();
    let flags: Vec<(bool, bool)> = d.subtasks.iter().map(|s| (s.next, s.waiting)).collect();
    assert_eq!(flags, [(false, false), (true, false), (false, true)]);
}

#[test]
fn a_block_on_the_group_or_from_outside_makes_every_step_wait() {
    let mut conn = db();
    let parent = task(&mut conn, "Launch");
    let one = tasks::create_subtask(&mut conn, parent.id, "One".into()).unwrap();
    tasks::create_subtask(&mut conn, parent.id, "Two".into()).unwrap();
    let approval = task(&mut conn, "Approval");
    block(&mut conn, approval.id, parent.id);
    let d = views::task_detail(&conn, parent.id).unwrap();
    assert!(d.subtasks.iter().all(|s| s.waiting && !s.next));
    // Approval done: both are free, and the first is the next step.
    finish(&mut conn, approval.id, TaskStatus::Done);
    let d = views::task_detail(&conn, parent.id).unwrap();
    assert!(d.subtasks.iter().all(|s| !s.waiting));
    assert_eq!(d.subtasks.iter().filter(|s| s.next).count(), 1);
    assert!(d.subtasks.iter().find(|s| s.next).unwrap().node.node.id == one.id);
}

#[test]
fn the_group_gets_a_status_suggestion_from_its_subtasks() {
    let mut conn = db();
    let parent = task(&mut conn, "Launch");
    assert!(views::task_detail(&conn, parent.id)
        .unwrap()
        .status_hint
        .is_none());
    let a = tasks::create_subtask(&mut conn, parent.id, "A".into()).unwrap();
    let b = tasks::create_subtask(&mut conn, parent.id, "B".into()).unwrap();
    let hint = |conn: &Connection| views::task_detail(conn, parent.id).unwrap().status_hint;
    assert!(hint(&conn).is_none());
    finish(&mut conn, a.id, TaskStatus::InProgress);
    assert_eq!(hint(&conn).unwrap().status, TaskStatus::InProgress);
    finish(&mut conn, a.id, TaskStatus::Done);
    finish(&mut conn, b.id, TaskStatus::Done);
    let h = hint(&conn).unwrap();
    assert_eq!(
        (h.status, h.text.as_str()),
        (TaskStatus::Done, "All subtasks are done.")
    );
    // Taking the suggestion is the user's act: nothing changed by itself.
    assert_eq!(
        tasks::get(&conn, parent.id).unwrap().status,
        TaskStatus::Todo
    );
}
