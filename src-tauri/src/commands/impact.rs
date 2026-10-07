use minimap_core::impact::{analyze, apply_plan, ImpactError, World};
use minimap_store::Connection;
use minimap_types::{
    AppError, ApplyPreview, ApplyResult, Date, ImpactReport, NodeType, Patch, Slip, UpdateTask,
};
use tauri::State;

use crate::{
    error::{app_error, store_error},
    state::AppState,
};

fn impact_error(e: ImpactError) -> AppError {
    let code = match e {
        ImpactError::Cycle => "cycle",
        ImpactError::Unknown(_) => "not_found",
        _ => "invalid",
    };
    app_error(code, e.to_string())
}

/// Loads the active plan and runs `f` on it.
fn with_world<T>(
    conn: &Connection,
    f: impl FnOnce(&World) -> Result<T, ImpactError>,
) -> Result<T, AppError> {
    with_world_at(conn, minimap_store::today(), f)
}

/// [`with_world`] as of `today` (the demo-data snapshot tests fix it).
pub(crate) fn with_world_at<T>(
    conn: &Connection,
    today: Date,
    f: impl FnOnce(&World) -> Result<T, ImpactError>,
) -> Result<T, AppError> {
    let tasks = minimap_store::tasks::list(conn, false).map_err(store_error)?;
    let edges = minimap_store::edges::list_active(conn).map_err(store_error)?;
    let projects = minimap_store::projects::list(conn, false).map_err(store_error)?;
    let objectives = minimap_store::objectives::list(conn, false).map_err(store_error)?;
    let people = minimap_store::people::list(conn, false).map_err(store_error)?;
    let work_week = minimap_store::settings::get(conn)
        .map_err(store_error)?
        .work_week;
    let world = World {
        tasks: &tasks,
        edges: &edges,
        projects: &projects,
        objectives: &objectives,
        people: &people,
        today,
        work_week,
    };
    f(&world).map_err(impact_error)
}

/// "What if these slip?": what each slip ripples into (tasks, projects, objectives, people).
/// A slip is a task or project running some working days late. Writes nothing.
#[tauri::command]
pub async fn run_impact_analysis(
    state: State<'_, AppState>,
    slips: Vec<Slip>,
) -> Result<ImpactReport, AppError> {
    state
        .run(move |conn| with_world(conn, |w| analyze(w, &slips)))
        .await
}

/// The date changes "apply" would make for these slips. Writes nothing.
#[tauri::command]
pub async fn preview_apply_slips(
    state: State<'_, AppState>,
    slips: Vec<Slip>,
) -> Result<ApplyPreview, AppError> {
    state
        .run(move |conn| with_world(conn, |w| apply_plan(w, &slips)))
        .await
}

/// Records the scenario in the plan: the slipped tasks get a start date and their due dates
/// move; targets are untouched. All or nothing, with an activity entry per task.
#[tauri::command]
pub async fn apply_slips(
    state: State<'_, AppState>,
    slips: Vec<Slip>,
) -> Result<ApplyResult, AppError> {
    state.run(move |conn| apply_impl(conn, &slips)).await
}

pub(crate) fn apply_impl(conn: &mut Connection, slips: &[Slip]) -> Result<ApplyResult, AppError> {
    if slips
        .iter()
        .any(|s| !matches!(s.node.node_type, NodeType::Task | NodeType::Project))
    {
        return Err(app_error("invalid", "Only tasks and projects can slip"));
    }
    // Re-derived here from the plan as it is now, never trusted from the screen.
    let plan = with_world(conn, |w| apply_plan(w, slips))?;
    let updates = plan
        .changes
        .iter()
        .map(|c| {
            (
                c.task_id,
                UpdateTask {
                    start_date: Patch::Set(c.new_start),
                    due_date: c.new_due.map_or(Patch::Keep, Patch::Set),
                    ..Default::default()
                },
            )
        })
        .collect();
    let changed = minimap_store::tasks::update_many(conn, updates).map_err(store_error)?;
    Ok(ApplyResult {
        tasks_changed: changed.len() as u32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::schedule::schedule_impl;
    use minimap_types::{
        AssigneeChoice, CreateProject, CreateTask, EdgeType, NewEdge, NodeRef, ScheduleScope, Uuid,
    };

    fn project(conn: &mut Connection, title: &str) -> Uuid {
        minimap_store::projects::create(
            conn,
            CreateProject {
                title: title.into(),
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

    fn task(conn: &mut Connection, title: &str, project: Uuid, days: f64) -> Uuid {
        minimap_store::tasks::create(
            conn,
            CreateTask {
                links: Vec::new(),
                title: title.into(),
                assignee: AssigneeChoice::Nobody,
                description: String::new(),
                project_id: Some(project),
                status: None,
                estimate_days: Some(days),
                start_date: None,
                due_date: None,
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

    fn slip(task: Uuid, days: u32) -> Slip {
        Slip {
            node: NodeRef::new(NodeType::Task, task),
            days,
        }
    }

    #[test]
    fn analysis_reads_the_plan_and_changes_nothing() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let p = project(&mut conn, "API");
        let a = task(&mut conn, "Design", p, 2.0);
        let b = task(&mut conn, "Build", p, 3.0);
        blocks(&mut conn, a, b);
        let activity_before = minimap_store::activity::count(&conn).unwrap();

        let r = with_world(&conn, |w| analyze(w, &[slip(a, 2)])).unwrap();
        assert_eq!(r.tasks.len(), 2);
        assert_eq!(r.projects[0].title, "API");
        assert_eq!(r.projects[0].delay_days, 2.0);
        assert_eq!(
            minimap_store::activity::count(&conn).unwrap(),
            activity_before
        );
        let t = minimap_store::tasks::get(&conn, a).unwrap();
        assert_eq!(t.start_date, None);
    }

    #[test]
    fn errors_have_codes_the_screen_can_show() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let p = project(&mut conn, "API");
        let a = task(&mut conn, "Design", p, 1.0);
        let err =
            |s: Vec<Slip>, conn: &Connection| with_world(conn, |w| analyze(w, &s)).unwrap_err();
        assert_eq!(err(vec![], &conn).code, "invalid");
        assert_eq!(err(vec![slip(Uuid::nil(), 1)], &conn).code, "not_found");
        assert_eq!(err(vec![slip(a, 0)], &conn).code, "invalid");
        assert!(apply_impl(&mut conn, &[]).is_err());
    }

    #[test]
    fn applying_records_the_slip_and_the_schedule_then_matches_the_what_if() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let p = project(&mut conn, "API");
        let a = task(&mut conn, "Design", p, 2.0);
        let b = task(&mut conn, "Build", p, 3.0);
        blocks(&mut conn, a, b);
        minimap_store::tasks::update(
            &mut conn,
            a,
            UpdateTask {
                due_date: Patch::Set(minimap_store::today() + time::Duration::days(14)),
                ..Default::default()
            },
        )
        .unwrap();

        let slips = [slip(a, 3)];
        let predicted = with_world(&conn, |w| analyze(w, &slips)).unwrap();
        let preview = with_world(&conn, |w| apply_plan(w, &slips)).unwrap();
        assert_eq!(preview.changes.len(), 1);
        assert_eq!(preview.changes[0].title, "Design");

        let done = apply_impl(&mut conn, &slips).unwrap();
        assert_eq!(done.tasks_changed, 1);
        let after = minimap_store::tasks::get(&conn, a).unwrap();
        assert_eq!(after.start_date, Some(preview.changes[0].new_start));
        assert_eq!(after.due_date, preview.changes[0].new_due);
        // Build wasn't pinned; the schedule moved it.
        assert_eq!(
            minimap_store::tasks::get(&conn, b).unwrap().start_date,
            None
        );

        // The plain schedule now shows exactly what the what-if predicted.
        let s = schedule_impl(&conn, ScheduleScope::Project(p)).unwrap();
        let build = s.tasks.iter().find(|t| t.title == "Build").unwrap();
        let predicted_build = predicted.tasks.iter().find(|t| t.title == "Build").unwrap();
        assert_eq!(build.finish, predicted_build.new_finish);
        // Recorded in the history.
        let history = minimap_store::activity::list_for_node(&conn, a).unwrap();
        assert!(history.iter().any(|h| h.diff.get("start_date").is_some()));
    }

    #[test]
    fn a_failed_apply_changes_nothing() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let p = project(&mut conn, "API");
        let a = task(&mut conn, "Design", p, 2.0);
        let bad = Slip {
            node: NodeRef::new(NodeType::Person, a),
            days: 1,
        };
        assert!(apply_impl(&mut conn, &[slip(a, 2), bad]).is_err());
        assert_eq!(
            minimap_store::tasks::get(&conn, a).unwrap().start_date,
            None
        );
    }
}
