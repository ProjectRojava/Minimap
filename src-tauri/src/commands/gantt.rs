//! The Gantt screen (spec 57). Thin: the tree and the drag rules are `minimap_core::gantt`.
//! Dropping a bar is one command, so it is one undo step however many due dates it pushes.

use minimap_core::{
    gantt::{build, plan_drag, BuildInput},
    impact::ImpactError,
};
use minimap_store::Connection;
use minimap_types::{
    AppError, Date, DragApplied, DragPreview, GanttData, GanttFilter, Patch, TaskDrag, UpdateTask,
};
use tauri::State;

use crate::{
    error::{app_error, store_error},
    state::AppState,
};

/// The plan as the Gantt screen draws it: objectives, their projects, their tasks, and the links
/// between them, on one axis of working days. Read-only.
#[tauri::command]
pub async fn get_gantt(
    state: State<'_, AppState>,
    filter_by: GanttFilter,
) -> Result<GanttData, AppError> {
    state
        .run(move |conn| gantt_at(conn, minimap_store::today(), filter_by))
        .await
}

/// What dropping a bar here would do: the new dates, the bars that move, the due dates pushed, or
/// why it can't be dropped there. Writes nothing.
#[tauri::command]
pub async fn preview_task_drag(
    state: State<'_, AppState>,
    drag: TaskDrag,
) -> Result<DragPreview, AppError> {
    state
        .run(move |conn| preview_at(conn, minimap_store::today(), drag))
        .await
}

/// Drops the bar: writes the task's dates and estimate, and pushes the due dates of what follows
/// later by as much as it moved. All or nothing, one undo step.
#[tauri::command]
pub async fn apply_task_drag(
    state: State<'_, AppState>,
    drag: TaskDrag,
) -> Result<DragApplied, AppError> {
    state
        .run(move |conn| apply_at(conn, minimap_store::today(), drag))
        .await
}

pub(crate) fn gantt_at(
    conn: &Connection,
    today: Date,
    filter: GanttFilter,
) -> Result<GanttData, AppError> {
    let tasks = minimap_store::tasks::list_planned(conn).map_err(store_error)?;
    let edges = minimap_store::edges::list_active(conn).map_err(store_error)?;
    let projects = minimap_store::projects::list(conn, false).map_err(store_error)?;
    let objectives = minimap_store::objectives::list(conn, false).map_err(store_error)?;
    let people = minimap_store::people::list(conn, false).map_err(store_error)?;
    let settings = minimap_store::settings::get(conn).map_err(store_error)?;
    let scope = if filter.mine {
        Some(crate::commands::orientation::my_scope_at(conn, today)?)
    } else {
        None
    };
    build(&BuildInput {
        tasks: &tasks,
        edges: &edges,
        projects: &projects,
        objectives: &objectives,
        people: &people,
        today,
        work_week: settings.work_week,
        zone_narrow_days: settings.zone_narrow_days,
        scope: scope.as_ref(),
    })
    .map_err(|e| app_error("cycle", e.to_string()))
}

pub(crate) fn preview_at(
    conn: &Connection,
    today: Date,
    drag: TaskDrag,
) -> Result<DragPreview, AppError> {
    crate::commands::impact::with_world_at(conn, today, |w| {
        plan_drag(w, &drag).map_err(|_| ImpactError::Cycle)
    })
}

pub(crate) fn apply_at(
    conn: &mut Connection,
    today: Date,
    drag: TaskDrag,
) -> Result<DragApplied, AppError> {
    // Re-derived here from the plan as it is now, never trusted from the screen.
    let preview = preview_at(conn, today, drag)?;
    if let Some(why) = &preview.refusal {
        return Err(app_error("refused", why.text()));
    }
    if preview.is_noop() {
        return Ok(DragApplied { preview, pushed: 0 });
    }
    let mut updates = vec![(
        drag.task_id,
        UpdateTask {
            start_date: preview.start_date.map_or(Patch::Keep, Patch::Set),
            due_date: preview.due_date.map_or(Patch::Keep, Patch::Set),
            estimate_days: preview.estimate_days.map_or(Patch::Keep, Patch::Set),
            ..Default::default()
        },
    )];
    updates.extend(preview.pushes.iter().map(|p| {
        (
            p.task_id,
            UpdateTask {
                due_date: Patch::Set(p.new_due),
                ..Default::default()
            },
        )
    }));
    minimap_store::tasks::update_many(conn, updates).map_err(store_error)?;
    let pushed = preview.pushes.len() as u32;
    Ok(DragApplied { preview, pushed })
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{
        AssigneeChoice, CreateProject, CreateTask, DragEdge, EdgeType, NewEdge, NodeRef, NodeType,
        Uuid,
    };
    use time::macros::date;

    // Monday 2027-03-01.
    const TODAY: Date = date!(2027 - 03 - 01);

    fn project(conn: &mut Connection) -> Uuid {
        minimap_store::projects::create(
            conn,
            CreateProject {
                title: "API".into(),
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
        .id
    }

    fn task(
        conn: &mut Connection,
        title: &str,
        project: Uuid,
        days: f64,
        due: Option<Date>,
    ) -> Uuid {
        minimap_store::tasks::create(
            conn,
            CreateTask {
                links: Vec::new(),
                task_type: Some("build".into()),
                focus: None,
                backburner_since: None,
                start_minute: None,
                length_minutes: None,
                title: title.into(),
                assignee: AssigneeChoice::Nobody,
                description: String::new(),
                project_id: Some(project),
                status: None,
                estimate_days: Some(days),
                start_date: None,
                due_date: due,
                priority: None,
                recurrence: None,
            },
        )
        .unwrap()
        .id
    }

    fn blocks(conn: &mut Connection, a: Uuid, b: Uuid) {
        minimap_store::edges::add(
            conn,
            NewEdge {
                edge_type: EdgeType::Blocks,
                from: NodeRef::new(NodeType::Task, a),
                to: NodeRef::new(NodeType::Task, b),
                attrs: serde_json::json!({}),
            },
        )
        .unwrap();
    }

    fn move_to(task_id: Uuid, to: Date) -> TaskDrag {
        TaskDrag {
            task_id,
            edge: DragEdge::Move,
            to,
        }
    }

    #[test]
    fn dropping_a_bar_writes_its_dates_and_pushes_what_follows_in_one_go() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let p = project(&mut conn);
        let a = task(&mut conn, "Design", p, 2.0, None);
        let b = task(&mut conn, "Build", p, 2.0, Some(date!(2027 - 03 - 04)));
        let c = task(&mut conn, "Docs", p, 1.0, None);
        blocks(&mut conn, a, b);
        let activity = minimap_store::activity::count(&conn).unwrap();

        let done = apply_at(&mut conn, TODAY, move_to(a, date!(2027 - 03 - 03))).unwrap();
        assert_eq!(done.pushed, 1);
        let get = |conn: &Connection, id| minimap_store::tasks::get(conn, id).unwrap();
        assert_eq!(get(&conn, a).start_date, Some(date!(2027 - 03 - 03)));
        // Build is pushed two working days in its due date, never in its start.
        assert_eq!(get(&conn, b).due_date, Some(date!(2027 - 03 - 08)));
        assert_eq!(get(&conn, b).start_date, None);
        // Docs follows nothing and is untouched.
        assert_eq!(get(&conn, c).due_date, None);
        // One row for the dragged task, one for the pushed one.
        assert_eq!(minimap_store::activity::count(&conn).unwrap(), activity + 2);
        // The plan agrees with what the preview showed.
        let data = gantt_at(&conn, TODAY, GanttFilter::default()).unwrap();
        let build = data.groups[0].projects[0]
            .tasks
            .iter()
            .find(|t| t.schedule.title == "Build")
            .unwrap();
        assert_eq!(build.schedule.start, date!(2027 - 03 - 05));
        assert_eq!(build.due_date, Some(date!(2027 - 03 - 08)));
    }

    #[test]
    fn a_refused_drop_writes_nothing_and_says_why() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let p = project(&mut conn);
        let a = task(&mut conn, "Design", p, 2.0, None);
        let b = task(&mut conn, "Build", p, 2.0, None);
        blocks(&mut conn, a, b);
        let activity = minimap_store::activity::count(&conn).unwrap();
        let err = apply_at(&mut conn, TODAY, move_to(b, date!(2027 - 03 - 02))).unwrap_err();
        assert_eq!(err.code, "refused");
        assert!(
            err.message.contains("before Design is done"),
            "{}",
            err.message
        );
        assert_eq!(minimap_store::activity::count(&conn).unwrap(), activity);
        // Dropping a bar where it already is changes nothing either.
        let same = apply_at(&mut conn, TODAY, move_to(a, TODAY)).unwrap();
        assert_eq!((same.pushed, same.preview.is_noop()), (0, true));
        assert_eq!(minimap_store::activity::count(&conn).unwrap(), activity);
    }

    #[test]
    fn the_resize_edges_write_the_estimate_with_the_dates() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let p = project(&mut conn);
        let a = task(&mut conn, "Design", p, 2.0, None);
        let end = TaskDrag {
            task_id: a,
            edge: DragEdge::End,
            to: date!(2027 - 03 - 05),
        };
        apply_at(&mut conn, TODAY, end).unwrap();
        let t = minimap_store::tasks::get(&conn, a).unwrap();
        assert_eq!(
            (t.due_date, t.estimate_days, t.start_date),
            (Some(date!(2027 - 03 - 05)), Some(5.0), None)
        );
        let start = TaskDrag {
            task_id: a,
            edge: DragEdge::Start,
            to: date!(2027 - 03 - 03),
        };
        apply_at(&mut conn, TODAY, start).unwrap();
        let t = minimap_store::tasks::get(&conn, a).unwrap();
        assert_eq!(
            (t.start_date, t.estimate_days),
            (Some(date!(2027 - 03 - 03)), Some(3.0))
        );
    }

    #[test]
    fn the_screen_lists_projects_under_no_objective_and_a_my_view_without_me_is_empty() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let p = project(&mut conn);
        task(&mut conn, "Design", p, 2.0, None);
        let all = gantt_at(&conn, TODAY, GanttFilter::default()).unwrap();
        assert_eq!(all.groups.len(), 1);
        assert!(all.groups[0].objective.is_none());
        let mine = gantt_at(&conn, TODAY, GanttFilter { mine: true }).unwrap();
        assert!(mine.groups.is_empty());
    }
}
