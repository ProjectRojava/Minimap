use minimap_core::tasks::{filter, parse_estimate, parse_lines, sort};
use minimap_store::Connection;
use minimap_types::{
    AppError, AssigneeChoice, CreateTask, EdgeType, NewEdge, NodeRef, NodeType, Patch, Task,
    TaskDetail, TaskFilter, TaskRow, UpdateTask, Uuid,
};
use tauri::State;

use crate::{
    error::{app_error, store_error},
    state::AppState,
};

/// A task can only be put in a project that exists and isn't archived.
fn check_project_active(conn: &Connection, project: Uuid) -> Result<(), AppError> {
    let s = minimap_store::nodes::summary(conn, NodeRef::new(NodeType::Project, project))
        .map_err(store_error)?;
    if s.archived {
        return Err(app_error("invalid", "that project is archived"));
    }
    Ok(())
}

/// Active tasks, filtered, due date first then priority.
#[tauri::command]
pub async fn list_tasks(
    state: State<'_, AppState>,
    filter_by: TaskFilter,
) -> Result<Vec<TaskRow>, AppError> {
    state
        .run(move |conn| {
            let rows = minimap_store::views::task_rows(conn).map_err(store_error)?;
            let mut rows = filter(rows, &filter_by);
            sort(&mut rows);
            Ok(rows)
        })
        .await
}

#[tauri::command]
pub async fn get_task(state: State<'_, AppState>, id: Uuid) -> Result<Task, AppError> {
    state
        .run(move |conn| minimap_store::tasks::get(conn, id).map_err(store_error))
        .await
}

#[tauri::command]
pub async fn get_task_detail(state: State<'_, AppState>, id: Uuid) -> Result<TaskDetail, AppError> {
    state
        .run(move |conn| minimap_store::views::task_detail(conn, id).map_err(store_error))
        .await
}

/// Assigned to the self person unless the input says otherwise.
#[tauri::command]
pub async fn create_task(state: State<'_, AppState>, input: CreateTask) -> Result<Task, AppError> {
    state
        .run(move |conn| {
            if let Some(p) = input.project_id {
                check_project_active(conn, p)?;
            }
            minimap_store::tasks::create(conn, input).map_err(store_error)
        })
        .await
}

#[tauri::command]
pub async fn update_task(
    state: State<'_, AppState>,
    id: Uuid,
    patch: UpdateTask,
) -> Result<Task, AppError> {
    state
        .run(move |conn| {
            if let Patch::Set(p) = patch.project_id {
                check_project_active(conn, p)?;
            }
            minimap_store::tasks::update(conn, id, patch).map_err(store_error)
        })
        .await
}

/// Sets the estimate from text like `3d` or `4h` (hours use the hours-per-day setting);
/// empty text clears it.
#[tauri::command]
pub async fn set_task_estimate(
    state: State<'_, AppState>,
    id: Uuid,
    text: String,
) -> Result<Task, AppError> {
    state
        .run(move |conn| set_task_estimate_impl(conn, id, &text))
        .await
}

pub(crate) fn set_task_estimate_impl(
    conn: &mut Connection,
    id: Uuid,
    text: &str,
) -> Result<Task, AppError> {
    let estimate = if text.trim().is_empty() {
        Patch::Clear
    } else {
        let hours = minimap_store::settings::get(conn)
            .map_err(store_error)?
            .hours_per_day;
        Patch::Set(parse_estimate(text, hours).map_err(|e| app_error("invalid", e))?)
    };
    let patch = UpdateTask {
        estimate_days: estimate,
        ..Default::default()
    };
    minimap_store::tasks::update(conn, id, patch).map_err(store_error)
}

/// Makes `person_id` the only assignee; `None` unassigns.
#[tauri::command]
pub async fn set_assignee(
    state: State<'_, AppState>,
    task_id: Uuid,
    person_id: Option<Uuid>,
) -> Result<(), AppError> {
    state
        .run(move |conn| {
            minimap_store::tasks::set_assignee(conn, task_id, person_id).map_err(store_error)
        })
        .await
}

/// Makes `parent_id` the parent of `task_id` (the task becomes its subtask), replacing any
/// parent it had; `None` takes it out. A loop is refused with the path.
#[tauri::command]
pub async fn set_parent(
    state: State<'_, AppState>,
    task_id: Uuid,
    parent_id: Option<Uuid>,
) -> Result<(), AppError> {
    state
        .run(move |conn| set_parent_impl(conn, task_id, parent_id))
        .await
}

pub(crate) fn set_parent_impl(
    conn: &mut Connection,
    task_id: Uuid,
    parent_id: Option<Uuid>,
) -> Result<(), AppError> {
    if let Some(p) = parent_id {
        let new = NewEdge {
            edge_type: EdgeType::SubtaskOf,
            from: NodeRef::new(NodeType::Task, task_id),
            to: NodeRef::new(NodeType::Task, p),
            attrs: serde_json::json!({}),
        };
        super::edges::check_new_edge(conn, &new)?;
    }
    minimap_store::tasks::set_parent(conn, task_id, parent_id).map_err(store_error)
}

/// Creates a task that is a subtask of `parent_id`, in the same project.
#[tauri::command]
pub async fn create_subtask(
    state: State<'_, AppState>,
    parent_id: Uuid,
    title: String,
) -> Result<Task, AppError> {
    state
        .run(move |conn| create_subtask_impl(conn, parent_id, title))
        .await
}

pub(crate) fn create_subtask_impl(
    conn: &mut Connection,
    parent_id: Uuid,
    title: String,
) -> Result<Task, AppError> {
    let title = title.trim().to_owned();
    if title.is_empty() {
        return Err(app_error("invalid", "A subtask needs a title"));
    }
    minimap_store::tasks::create_subtask(conn, parent_id, title).map_err(store_error)
}

/// Archives the task and its links.
#[tauri::command]
pub async fn archive_task(state: State<'_, AppState>, id: Uuid) -> Result<(), AppError> {
    state
        .run(move |conn| {
            minimap_store::nodes::archive(conn, NodeRef::new(NodeType::Task, id))
                .map_err(store_error)
        })
        .await
}

/// Preview of a pasted list: one title per non-empty line, list markers removed.
#[tauri::command]
pub async fn parse_task_lines(text: String) -> Result<Vec<String>, AppError> {
    Ok(parse_lines(&text))
}

/// Creates one task per title, all or none.
#[tauri::command]
pub async fn create_tasks_bulk(
    state: State<'_, AppState>,
    titles: Vec<String>,
    project_id: Option<Uuid>,
    assignee: AssigneeChoice,
) -> Result<Vec<Task>, AppError> {
    state
        .run(move |conn| create_tasks_bulk_impl(conn, titles, project_id, assignee))
        .await
}

pub(crate) fn create_tasks_bulk_impl(
    conn: &mut Connection,
    titles: Vec<String>,
    project_id: Option<Uuid>,
    assignee: AssigneeChoice,
) -> Result<Vec<Task>, AppError> {
    if titles.iter().all(|t| t.trim().is_empty()) {
        return Err(app_error("invalid", "there are no tasks to create"));
    }
    if let Some(p) = project_id {
        check_project_active(conn, p)?;
    }
    let inputs = titles
        .into_iter()
        .filter(|t| !t.trim().is_empty())
        .map(|title| CreateTask {
            links: Vec::new(),
            title,
            assignee,
            description: String::new(),
            project_id,
            status: None,
            estimate_days: None,
            start_date: None,
            due_date: None,
            priority: None,
            recurrence: None,
        })
        .collect();
    minimap_store::tasks::create_many(conn, inputs).map_err(store_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{CreateProject, UpdateSettings};

    fn conn() -> Connection {
        minimap_store::open_in_memory().unwrap()
    }

    fn task(conn: &mut Connection, title: &str) -> Task {
        create_tasks_bulk_impl(conn, vec![title.into()], None, AssigneeChoice::Nobody)
            .unwrap()
            .remove(0)
    }

    #[test]
    fn a_subtask_loop_is_refused_with_the_path_and_changes_nothing() {
        let mut conn = conn();
        let (a, b, c) = (
            task(&mut conn, "Ship"),
            task(&mut conn, "Build"),
            task(&mut conn, "Test"),
        );
        set_parent_impl(&mut conn, b.id, Some(a.id)).unwrap(); // Build is a subtask of Ship
        set_parent_impl(&mut conn, c.id, Some(b.id)).unwrap(); // Test of Build
        let err = set_parent_impl(&mut conn, a.id, Some(c.id)).unwrap_err();
        assert_eq!(err.code, "cycle");
        assert!(
            err.message.contains("Ship → Test → Build → Ship"),
            "{}",
            err.message
        );
        assert!(minimap_store::views::task_detail(&conn, a.id)
            .unwrap()
            .parent
            .is_none());
        // A task cannot be its own parent, and a parent of the wrong kind is refused.
        assert!(set_parent_impl(&mut conn, a.id, Some(a.id)).is_err());
    }

    fn block(conn: &mut Connection, from: Uuid, to: Uuid) -> Result<(), AppError> {
        super::super::edges::add_edge_impl(
            conn,
            NewEdge {
                edge_type: EdgeType::Blocks,
                from: NodeRef::new(NodeType::Task, from),
                to: NodeRef::new(NodeType::Task, to),
                attrs: serde_json::json!({}),
            },
        )
        .map(|_| ())
    }

    #[test]
    fn a_group_cannot_block_or_wait_for_its_own_part_either_way_round() {
        let mut conn = conn();
        let (group, part, other) = (
            task(&mut conn, "Group"),
            task(&mut conn, "Part"),
            task(&mut conn, "Other"),
        );
        set_parent_impl(&mut conn, part.id, Some(group.id)).unwrap();
        for (a, b) in [(group.id, part.id), (part.id, group.id)] {
            let err = block(&mut conn, a, b).unwrap_err();
            assert_eq!(err.code, "invalid");
            assert!(
                err.message.contains("subtask of the other"),
                "{}",
                err.message
            );
        }
        // Between the group and something else is fine, and so is ordering the parts.
        block(&mut conn, other.id, group.id).unwrap();
        let second = create_subtask_impl(&mut conn, group.id, "Second".into()).unwrap();
        block(&mut conn, part.id, second.id).unwrap();
    }

    #[test]
    fn a_loop_that_only_exists_once_a_group_is_read_as_its_parts_is_refused() {
        let mut conn = conn();
        let (group, part, approval) = (
            task(&mut conn, "Group"),
            task(&mut conn, "Part"),
            task(&mut conn, "Approval"),
        );
        set_parent_impl(&mut conn, part.id, Some(group.id)).unwrap();
        block(&mut conn, approval.id, group.id).unwrap(); // the group waits for Approval
        let err = block(&mut conn, part.id, approval.id).unwrap_err(); // ...so a part may not block it
        assert_eq!(err.code, "cycle");
        assert!(
            err.message.contains("Approval") && err.message.contains("Part"),
            "{}",
            err.message
        );
    }

    #[test]
    fn a_task_cannot_become_a_subtask_of_something_it_is_linked_to_by_blocks_or_loops_with() {
        let mut conn = conn();
        let (a, b, c) = (
            task(&mut conn, "A"),
            task(&mut conn, "B"),
            task(&mut conn, "C"),
        );
        block(&mut conn, a.id, b.id).unwrap();
        let err = set_parent_impl(&mut conn, b.id, Some(a.id)).unwrap_err();
        assert_eq!(err.code, "invalid");
        assert!(err.message.contains("blocks"), "{}", err.message);
        assert!(minimap_store::views::task_detail(&conn, b.id)
            .unwrap()
            .parent
            .is_none());
        // C waits for the group A would hold B in; B (to be a part of A's group via C) loops.
        block(&mut conn, b.id, c.id).unwrap();
        let loop_err = set_parent_impl(&mut conn, a.id, Some(c.id)).unwrap_err();
        assert!(
            matches!(loop_err.code.as_str(), "cycle" | "invalid"),
            "{}",
            loop_err.message
        );
    }

    #[test]
    fn a_subtask_needs_a_title_and_a_live_parent() {
        let mut conn = conn();
        let parent = task(&mut conn, "Parent");
        assert_eq!(
            create_subtask_impl(&mut conn, parent.id, "  ".into())
                .unwrap_err()
                .code,
            "invalid"
        );
        let made = create_subtask_impl(&mut conn, parent.id, " Draft the plan ".into()).unwrap();
        assert_eq!(made.title, "Draft the plan");
        assert!(create_subtask_impl(&mut conn, Uuid::now_v7(), "x".into()).is_err());
    }

    #[test]
    fn estimates_follow_the_hours_per_day_setting() {
        let mut conn = conn();
        let t = task(&mut conn, "t");
        let days = |conn: &mut Connection, text: &str| {
            set_task_estimate_impl(conn, t.id, text).map(|t| t.estimate_days)
        };
        assert_eq!(days(&mut conn, "3d").unwrap(), Some(3.0));
        assert_eq!(days(&mut conn, "4h").unwrap(), Some(0.5)); // default 8 h/day
        minimap_store::settings::update(
            &mut conn,
            UpdateSettings {
                hours_per_day: Some(6.0),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(days(&mut conn, "6h").unwrap(), Some(1.0));
        // Existing estimates are not rewritten when the setting changes.
        assert_eq!(days(&mut conn, "3d").unwrap(), Some(3.0));
        // Empty clears; nonsense is rejected and leaves the estimate alone.
        assert_eq!(days(&mut conn, "  ").unwrap(), None);
        days(&mut conn, "2d").unwrap();
        let err = days(&mut conn, "soon").unwrap_err();
        assert_eq!(err.code, "invalid");
        assert!(err.message.contains("3d or 4h"));
        assert_eq!(
            minimap_store::tasks::get(&conn, t.id)
                .unwrap()
                .estimate_days,
            Some(2.0)
        );
    }

    #[test]
    fn bulk_create_assigns_and_files_into_a_project() {
        let mut conn = conn();
        let me = minimap_store::people::ensure_self(&mut conn, "Me").unwrap();
        let project = minimap_store::projects::create(
            &mut conn,
            CreateProject {
                title: "P".into(),
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
        let made = create_tasks_bulk_impl(
            &mut conn,
            vec!["one".into(), "  ".into(), "two".into()],
            Some(project.id),
            AssigneeChoice::Me,
        )
        .unwrap();
        assert_eq!(made.len(), 2, "blank lines are skipped");
        assert!(made.iter().all(|t| t.project_id == Some(project.id)));
        let assigned =
            minimap_store::edges::list_active_of_type(&conn, minimap_types::EdgeType::AssignedTo)
                .unwrap();
        assert_eq!(assigned.len(), 2);
        assert!(assigned.iter().all(|e| e.to_id == me.id));

        let none = create_tasks_bulk_impl(&mut conn, vec![" ".into()], None, AssigneeChoice::Me);
        assert_eq!(none.unwrap_err().code, "invalid");
    }

    #[test]
    fn archived_projects_are_refused() {
        let mut conn = conn();
        let project = minimap_store::projects::create(
            &mut conn,
            CreateProject {
                title: "Old".into(),
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
        minimap_store::nodes::archive(&mut conn, NodeRef::new(NodeType::Project, project.id))
            .unwrap();
        let err = create_tasks_bulk_impl(
            &mut conn,
            vec!["x".into()],
            Some(project.id),
            AssigneeChoice::Nobody,
        )
        .unwrap_err();
        assert_eq!(err.code, "invalid");
        assert!(minimap_store::tasks::list(&conn, true).unwrap().is_empty());
        assert_eq!(
            check_project_active(&conn, project.id).unwrap_err().code,
            "invalid"
        );
    }
}
