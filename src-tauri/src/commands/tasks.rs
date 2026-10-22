use minimap_core::{
    focus,
    tasks::{filter, parse_estimate, parse_lines, sort},
};
use minimap_store::Connection;
use minimap_types::{
    AppError, AssigneeChoice, BackburnerResult, CreateTask, Date, Decision, FocusChoice,
    LinkRelation, NodeRef, NodeType, Patch, Task, TaskDetail, TaskFilter, TaskRow, UpdateTask,
    Uuid,
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

/// Puts a task in focus, or takes it out (spec 37): kept on This week every day until taken out,
/// for today only, or through a date. A day that has passed is refused.
#[tauri::command]
pub async fn set_task_focus(
    state: State<'_, AppState>,
    id: Uuid,
    choice: FocusChoice,
) -> Result<Task, AppError> {
    state
        .run(move |conn| set_task_focus_impl(conn, id, choice, minimap_store::today()))
        .await
}

pub(crate) fn set_task_focus_impl(
    conn: &mut Connection,
    id: Uuid,
    choice: FocusChoice,
    today: Date,
) -> Result<Task, AppError> {
    let patch = UpdateTask {
        focus: match focus::resolve(choice, today).map_err(|m| app_error("invalid", m))? {
            Some(f) => Patch::Set(f),
            None => Patch::Clear,
        },
        ..Default::default()
    };
    minimap_store::tasks::update(conn, id, patch).map_err(store_error)
}

/// Puts a task on the backburner (`on`), or brings it back (spec 59). It keeps its status, dates
/// and links; it leaves This week, capacity, the health counts and the fog until it is back. The
/// schedule still counts it, so open tasks that wait on it are counted and returned. A finished,
/// cancelled or meeting task is refused.
#[tauri::command]
pub async fn set_task_backburner(
    state: State<'_, AppState>,
    id: Uuid,
    on: bool,
) -> Result<BackburnerResult, AppError> {
    state
        .run(move |conn| set_task_backburner_impl(conn, id, on, minimap_store::today()))
        .await
}

pub(crate) fn set_task_backburner_impl(
    conn: &mut Connection,
    id: Uuid,
    on: bool,
    today: Date,
) -> Result<BackburnerResult, AppError> {
    let before = minimap_store::tasks::get(conn, id).map_err(store_error)?;
    if on && minimap_core::tasks::is_closed(before.status) {
        return Err(app_error(
            "invalid",
            "A finished or cancelled task is not set aside. Reopen it first.",
        ));
    }
    let task = if on == before.is_parked() {
        before
    } else {
        let patch = UpdateTask {
            backburner_since: if on { Patch::Set(today) } else { Patch::Clear },
            ..Default::default()
        };
        minimap_store::tasks::update(conn, id, patch).map_err(store_error)?
    };
    let held_up = if on {
        let tasks = minimap_store::tasks::list(conn, false).map_err(store_error)?;
        let edges = minimap_store::edges::list_active(conn).map_err(store_error)?;
        minimap_core::tasks::held_up_by(id, &tasks, &edges).len() as u32
    } else {
        0
    };
    Ok(BackburnerResult { task, held_up })
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

/// Creates a task linked to `task_id`: it blocks it, is blocked by it, is related, is its parent or
/// is its sub-task. It joins the same project.
#[tauri::command]
pub async fn create_linked_task(
    state: State<'_, AppState>,
    task_id: Uuid,
    relation: LinkRelation,
    title: String,
    task_type: Option<String>,
) -> Result<Task, AppError> {
    state
        .run(move |conn| create_linked_task_impl(conn, task_id, relation, title, task_type))
        .await
}

pub(crate) fn create_linked_task_impl(
    conn: &mut Connection,
    task_id: Uuid,
    relation: LinkRelation,
    title: String,
    task_type: Option<String>,
) -> Result<Task, AppError> {
    let title = title.trim().to_owned();
    if title.is_empty() {
        return Err(app_error("invalid", "The new task needs a title"));
    }
    minimap_store::tasks::create_linked(conn, task_id, relation, title, task_type)
        .map_err(store_error)
}

/// The decision record of a task of type Decision (spec 41), made if the task has none yet (a
/// task typed Decision before records existed, or whose record was archived).
#[tauri::command]
pub async fn ensure_task_decision(
    state: State<'_, AppState>,
    task_id: Uuid,
) -> Result<Decision, AppError> {
    state
        .run(move |conn| {
            minimap_store::decisions::ensure_for_task(conn, task_id).map_err(store_error)
        })
        .await
}

/// The *Finish decision* dialog (spec 42): writes what was decided and why on the task's record
/// and finishes the task, which decides the record, in one step.
#[tauri::command]
pub async fn finish_decision_task(
    state: State<'_, AppState>,
    task_id: Uuid,
    decision: String,
    rationale: String,
) -> Result<Task, AppError> {
    state
        .run(move |conn| {
            minimap_store::decisions::finish_task(conn, task_id, decision, rationale)
                .map_err(store_error)
        })
        .await
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
    task_type: Option<String>,
) -> Result<Vec<Task>, AppError> {
    state
        .run(move |conn| create_tasks_bulk_impl(conn, titles, project_id, assignee, task_type))
        .await
}

pub(crate) fn create_tasks_bulk_impl(
    conn: &mut Connection,
    titles: Vec<String>,
    project_id: Option<Uuid>,
    assignee: AssigneeChoice,
    task_type: Option<String>,
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
            task_type: task_type.clone(),
            focus: None,
            backburner_since: None,
            start_minute: None,
            length_minutes: None,
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
    use minimap_types::Focus;
    use minimap_types::{CreateProject, UpdateSettings};

    fn conn() -> Connection {
        minimap_store::open_in_memory().unwrap()
    }

    fn task(conn: &mut Connection, title: &str) -> Task {
        let project = minimap_store::fixtures::project(conn);
        create_tasks_bulk_impl(
            conn,
            vec![title.into()],
            Some(project),
            AssigneeChoice::Nobody,
            Some("build".into()),
        )
        .unwrap()
        .remove(0)
    }

    #[test]
    fn the_backburner_command_parks_tells_what_waits_and_refuses_closed_tasks() {
        use time::macros::date;
        let today = date!(2027 - 03 - 03);
        let mut conn = conn();
        let blocker = task(&mut conn, "Blocker");
        let waits = task(&mut conn, "Waits");
        let done = task(&mut conn, "Done");
        minimap_store::edges::add(
            &mut conn,
            minimap_types::NewEdge {
                edge_type: minimap_types::EdgeType::Blocks,
                from: NodeRef::new(NodeType::Task, blocker.id),
                to: NodeRef::new(NodeType::Task, waits.id),
                attrs: serde_json::json!({}),
            },
        )
        .unwrap();
        let parked = set_task_backburner_impl(&mut conn, blocker.id, true, today).unwrap();
        assert_eq!(parked.task.backburner_since, Some(today));
        assert_eq!(parked.held_up, 1);
        // Asking again changes nothing, not even the day.
        let later =
            set_task_backburner_impl(&mut conn, blocker.id, true, date!(2027 - 03 - 09)).unwrap();
        assert_eq!(later.task.backburner_since, Some(today));
        // A task that waits on nothing says so; bringing one back clears it.
        assert_eq!(
            set_task_backburner_impl(&mut conn, waits.id, true, today)
                .unwrap()
                .held_up,
            0
        );
        let back = set_task_backburner_impl(&mut conn, blocker.id, false, today).unwrap();
        assert_eq!(back.task.backburner_since, None);
        assert_eq!(back.held_up, 0);
        // A finished task is not set aside.
        minimap_store::tasks::update(
            &mut conn,
            done.id,
            UpdateTask {
                status: Some(minimap_types::TaskStatus::Done),
                ..Default::default()
            },
        )
        .unwrap();
        let err = set_task_backburner_impl(&mut conn, done.id, true, today).unwrap_err();
        assert_eq!(err.code, "invalid");
    }

    #[test]
    fn finishing_an_unwritten_decision_task_has_its_own_error_code() {
        let mut conn = conn();
        let source = task(&mut conn, "Source");
        let made = create_linked_task_impl(
            &mut conn,
            source.id,
            LinkRelation::RelatesTo,
            "Pick the store".into(),
            Some("decision".into()),
        )
        .unwrap();
        let done = UpdateTask {
            status: Some(minimap_types::TaskStatus::Done),
            ..Default::default()
        };
        let err = minimap_store::tasks::update(&mut conn, made.id, done)
            .map_err(store_error)
            .unwrap_err();
        // The UI opens the *Finish decision* dialog on this code (`state::DECISION_INCOMPLETE`).
        assert_eq!(err.code, "decision_incomplete");
        assert!(err.message.contains("Pick the store"));
    }

    #[test]
    fn a_linked_task_needs_a_title_and_a_live_source() {
        let mut conn = conn();
        let source = task(&mut conn, "Source");
        assert_eq!(
            create_linked_task_impl(
                &mut conn,
                source.id,
                LinkRelation::Blocks,
                "  ".into(),
                None
            )
            .unwrap_err()
            .code,
            "invalid"
        );
        let made = create_linked_task_impl(
            &mut conn,
            source.id,
            LinkRelation::BlockedBy,
            " Send the PO ".into(),
            Some("decision".into()),
        )
        .unwrap();
        assert_eq!(made.title, "Send the PO");
        assert_eq!(made.task_type.as_deref(), Some("decision"));
        assert!(create_linked_task_impl(
            &mut conn,
            source.id,
            LinkRelation::RelatesTo,
            "x".into(),
            Some("no-such-type".into())
        )
        .is_err());
        assert!(create_linked_task_impl(
            &mut conn,
            Uuid::now_v7(),
            LinkRelation::Blocks,
            "x".into(),
            None
        )
        .is_err());
    }

    #[test]
    fn focus_is_set_for_today_or_a_date_cleared_and_a_past_day_refused() {
        let mut conn = conn();
        let t = task(&mut conn, "Keep in sight");
        let today = minimap_store::today();
        let set = |conn: &mut Connection, choice| set_task_focus_impl(conn, t.id, choice, today);
        assert_eq!(t.focus, None);
        assert_eq!(
            set(&mut conn, FocusChoice::Pinned).unwrap().focus,
            Some(Focus::PINNED)
        );
        assert_eq!(
            set(&mut conn, FocusChoice::Today).unwrap().focus,
            Some(Focus { until: Some(today) })
        );
        let later = today + time::Duration::days(30);
        assert_eq!(
            set(&mut conn, FocusChoice::Until { date: later })
                .unwrap()
                .focus,
            Some(Focus { until: Some(later) })
        );
        // Refused, and the focus stays as it was.
        let err = set(
            &mut conn,
            FocusChoice::Until {
                date: today - time::Duration::days(1),
            },
        )
        .unwrap_err();
        assert_eq!(err.code, "invalid");
        assert_eq!(
            minimap_store::tasks::get(&conn, t.id).unwrap().focus,
            Some(Focus { until: Some(later) })
        );
        assert_eq!(set(&mut conn, FocusChoice::Off).unwrap().focus, None);
        // It is an ordinary logged change: the history and undo see it.
        let history = minimap_store::activity::list_for_node(&conn, t.id).unwrap();
        assert!(history.iter().any(|a| a.diff.to_string().contains("focus")));
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
            Some("build".into()),
        )
        .unwrap();
        assert_eq!(made.len(), 2, "blank lines are skipped");
        assert!(made.iter().all(|t| t.project_id == Some(project.id)));
        let assigned =
            minimap_store::edges::list_active_of_type(&conn, minimap_types::EdgeType::AssignedTo)
                .unwrap();
        assert_eq!(assigned.len(), 2);
        assert!(assigned.iter().all(|e| e.to_id == me.id));

        let none = create_tasks_bulk_impl(
            &mut conn,
            vec![" ".into()],
            None,
            AssigneeChoice::Me,
            Some("build".into()),
        );
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
            Some("build".into()),
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
