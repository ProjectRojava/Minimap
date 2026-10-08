//! Subtasks were removed (ADR-0015): links that still say `subtask_of` must keep loading, and the
//! migration archives them so nothing shows them.

use minimap_store::*;
use minimap_types::*;

fn two_tasks(conn: &mut Connection) -> (Task, Task) {
    let make = |conn: &mut Connection, title: &str| {
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
    };
    (make(conn, "Parent"), make(conn, "Child"))
}

fn legacy_link(conn: &Connection, child: &Task, parent: &Task) {
    conn.execute(
        "INSERT INTO edges (id, edge_type, from_type, from_id, to_type, to_id, attrs, created_at)
         VALUES (?1, 'subtask_of', 'task', ?2, 'task', ?3, '{}', '2027-01-01T00:00:00Z')",
        rusqlite::params![
            uuid::Uuid::now_v7().to_string(),
            child.id.to_string(),
            parent.id.to_string()
        ],
    )
    .unwrap();
}

#[test]
fn an_old_subtask_link_still_loads_and_the_migration_archives_it() {
    let mut conn = open_in_memory().unwrap();
    let (parent, child) = two_tasks(&mut conn);
    legacy_link(&conn, &child, &parent);
    // It reads (an unknown edge type would fail here), exports and lists.
    assert_eq!(edges::list_all(&conn).unwrap().len(), 1);
    assert_eq!(edges::list_active(&conn).unwrap().len(), 1);
    // The migration hides it.
    conn.execute_batch(include_str!("../migrations/0012_remove_subtasks.sql"))
        .unwrap();
    assert!(edges::list_active(&conn).unwrap().is_empty());
    let all = edges::list_all(&conn).unwrap();
    assert_eq!(all.len(), 1);
    assert!(all[0].archived_at.is_some());
    // Nothing else about the two tasks changed.
    assert!(views::task_detail(&conn, child.id).is_ok());
}

#[test]
fn no_new_subtask_link_can_be_made() {
    use minimap_core::edge_rules;
    for from in NodeType::ALL {
        for to in NodeType::ALL {
            assert!(!edge_rules::is_allowed(EdgeType::SubtaskOf, *from, *to));
        }
    }
}
