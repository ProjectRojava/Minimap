//! Sub-tasks (spec 33, ADR-0017): a task is part of one task. Rows carry the parent and the
//! parent's progress; the one-parent, one-level rule refuses the rest; nothing about the
//! schedule changes.

use minimap_store::*;
use minimap_types::*;

fn task(conn: &mut Connection, title: &str) -> Task {
    tasks::create(
        conn,
        CreateTask {
            links: Vec::new(),
            task_type: None,
            title: title.into(),
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
    .unwrap()
}

fn part_of(conn: &mut Connection, child: &Task, parent: &Task) -> Result<Edge> {
    tasks::check_subtask(conn, child.id, parent.id)?;
    edges::add(
        conn,
        NewEdge {
            edge_type: EdgeType::SubtaskOf,
            from: NodeRef::new(NodeType::Task, child.id),
            to: NodeRef::new(NodeType::Task, parent.id),
            attrs: serde_json::json!({}),
        },
    )
}

fn row(conn: &Connection, id: uuid::Uuid) -> TaskRow {
    views::task_rows(conn)
        .unwrap()
        .into_iter()
        .find(|r| r.task.id == id)
        .unwrap()
}

fn status(conn: &mut Connection, t: &Task, s: TaskStatus) {
    tasks::update(
        conn,
        t.id,
        UpdateTask {
            status: Some(s),
            ..Default::default()
        },
    )
    .unwrap();
}

#[test]
fn a_parent_shows_its_progress_and_a_sub_task_its_parent() {
    let mut conn = open_in_memory().unwrap();
    let epic = task(&mut conn, "Epic");
    let (a, b, c, gone) = (
        task(&mut conn, "A"),
        task(&mut conn, "B"),
        task(&mut conn, "C"),
        task(&mut conn, "Dropped"),
    );
    for t in [&a, &b, &c, &gone] {
        part_of(&mut conn, t, &epic).unwrap();
    }
    status(&mut conn, &a, TaskStatus::Done);
    status(&mut conn, &gone, TaskStatus::Cancelled);
    let parent = row(&conn, epic.id);
    assert_eq!((parent.subtasks_done, parent.subtask_count), (1, 3));
    assert!(parent.parent.is_none());
    let child = row(&conn, b.id);
    assert_eq!(child.parent.unwrap().label, "Epic");
    assert_eq!(child.subtask_count, 0);
    // Parts are not blocking links: the board's link count stays at zero.
    assert_eq!((parent.link_count, child.link_count), (0, 0));
    // An archived sub-task no longer counts.
    nodes::archive(&mut conn, NodeRef::new(NodeType::Task, c.id)).unwrap();
    assert_eq!(row(&conn, epic.id).subtask_count, 2);
    // Archiving the parent frees its sub-tasks.
    nodes::archive(&mut conn, NodeRef::new(NodeType::Task, epic.id)).unwrap();
    assert!(row(&conn, b.id).parent.is_none());
}

#[test]
fn a_task_has_one_parent_and_the_tree_is_one_level_deep() {
    let mut conn = open_in_memory().unwrap();
    let (epic, other, a, b) = (
        task(&mut conn, "Epic"),
        task(&mut conn, "Other"),
        task(&mut conn, "A"),
        task(&mut conn, "B"),
    );
    part_of(&mut conn, &a, &epic).unwrap();
    // A second parent.
    let err = part_of(&mut conn, &a, &other).unwrap_err().to_string();
    assert!(err.contains("“A” is already part of “Epic”"), "{err}");
    // A sub-task as a parent.
    let err = part_of(&mut conn, &b, &a).unwrap_err().to_string();
    assert!(err.contains("one level"), "{err}");
    // A parent as a sub-task.
    let err = part_of(&mut conn, &epic, &other).unwrap_err().to_string();
    assert!(err.contains("“Epic” has sub-tasks"), "{err}");
    // Removing the link frees the task.
    let link = edges::list_active_of_type(&conn, EdgeType::SubtaskOf).unwrap()[0].id;
    edges::remove(&mut conn, link).unwrap();
    part_of(&mut conn, &a, &other).unwrap();
}

#[test]
fn a_new_parent_or_sub_task_is_made_and_linked_in_one_step() {
    let mut conn = open_in_memory().unwrap();
    let source = task(&mut conn, "Source");
    let sub = tasks::create_linked(
        &mut conn,
        source.id,
        LinkRelation::Subtask,
        "Step".into(),
        None,
    )
    .unwrap();
    assert_eq!(row(&conn, sub.id).parent.unwrap().label, "Source");
    assert_eq!(row(&conn, source.id).subtask_count, 1);
    // Source now has a sub-task, so it cannot get a parent; nothing is created by the refusal.
    let before = views::task_rows(&conn).unwrap().len();
    assert!(tasks::create_linked(
        &mut conn,
        source.id,
        LinkRelation::Parent,
        "Epic".into(),
        None
    )
    .is_err());
    // A sub-task cannot have sub-tasks.
    assert!(tasks::create_linked(
        &mut conn,
        sub.id,
        LinkRelation::Subtask,
        "Deeper".into(),
        None
    )
    .is_err());
    assert_eq!(views::task_rows(&conn).unwrap().len(), before);
    // A task with no family gets a parent.
    let lone = task(&mut conn, "Lone");
    let epic = tasks::create_linked(
        &mut conn,
        lone.id,
        LinkRelation::Parent,
        "Epic".into(),
        None,
    )
    .unwrap();
    assert_eq!(row(&conn, lone.id).parent.unwrap().label, "Epic");
    assert_eq!(row(&conn, epic.id).subtask_count, 1);
}

#[test]
fn a_task_can_be_part_of_one_task_and_blocked_by_another() {
    // Hierarchy and sequencing are independent: the same pair can have both, and neither affects
    // the other's rules.
    let mut conn = open_in_memory().unwrap();
    let (epic, a, b) = (
        task(&mut conn, "Epic"),
        task(&mut conn, "A"),
        task(&mut conn, "B"),
    );
    part_of(&mut conn, &a, &epic).unwrap();
    part_of(&mut conn, &b, &epic).unwrap();
    for (from, to) in [(&a, &b), (&b, &epic)] {
        edges::add(
            &mut conn,
            NewEdge {
                edge_type: EdgeType::Blocks,
                from: NodeRef::new(NodeType::Task, from.id),
                to: NodeRef::new(NodeType::Task, to.id),
                attrs: serde_json::json!({}),
            },
        )
        .unwrap();
    }
    assert_eq!(row(&conn, a.id).link_count, 1);
    assert_eq!(row(&conn, epic.id).subtask_count, 2);
}
