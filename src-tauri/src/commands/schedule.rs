use minimap_core::schedule::{compute, critical_path};
use minimap_store::Connection;
use minimap_types::{AppError, Date, Schedule, ScheduleScope, ScheduledTask};
use tauri::State;

use crate::{
    error::{app_error, store_error},
    state::AppState,
};

/// When every open task will finish, working days only, with slack and the critical tasks.
/// `Project(id)` shows one project (blocked by other projects' tasks where linked);
/// `Portfolio` shows everything, each project against its own target or projected finish.
#[tauri::command]
pub async fn get_schedule(
    state: State<'_, AppState>,
    scope: ScheduleScope,
) -> Result<Schedule, AppError> {
    state.run(move |conn| schedule_impl(conn, scope)).await
}

/// The tasks that decide the finish date, earliest first.
#[tauri::command]
pub async fn get_critical_path(
    state: State<'_, AppState>,
    scope: ScheduleScope,
) -> Result<Vec<ScheduledTask>, AppError> {
    state
        .run(move |conn| Ok(critical_path(&schedule_impl(conn, scope)?)))
        .await
}

pub(crate) fn schedule_impl(conn: &Connection, scope: ScheduleScope) -> Result<Schedule, AppError> {
    schedule_at(conn, minimap_store::today(), scope)
}

/// [`schedule_impl`] as of `today` (the demo-data snapshot tests fix it).
pub(crate) fn schedule_at(
    conn: &Connection,
    today: Date,
    scope: ScheduleScope,
) -> Result<Schedule, AppError> {
    if let ScheduleScope::Project(id) = scope {
        minimap_store::projects::get(conn, id).map_err(store_error)?;
    }
    let tasks = minimap_store::tasks::list(conn, false).map_err(store_error)?;
    // Blocks and subtask links (a group is scheduled through the tasks in it).
    let edges = minimap_store::edges::list_active(conn).map_err(store_error)?;
    let projects = minimap_store::projects::list(conn, false).map_err(store_error)?;
    let work_week = minimap_store::settings::get(conn)
        .map_err(store_error)?
        .work_week;
    compute(&tasks, &edges, &projects, today, work_week, scope)
        .map_err(|e| app_error("cycle", e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{
        AssigneeChoice, CreateProject, CreateTask, EdgeType, NewEdge, NodeRef, NodeType, Uuid,
    };

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

    #[test]
    fn the_schedule_follows_the_work_week_in_settings_at_once() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let p = minimap_store::projects::create(
            &mut conn,
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
        .id;
        task(&mut conn, "Long", p, 20.0);
        let days_off = |s: &minimap_types::Schedule, week: minimap_types::WorkWeek| {
            s.days
                .iter()
                .filter(|d| !week.contains_weekday(d.weekday()))
                .count()
        };
        let default = schedule_impl(&conn, ScheduleScope::Portfolio).unwrap();
        assert_eq!(days_off(&default, minimap_types::WorkWeek::MON_FRI), 0);
        let four = minimap_types::WorkWeek::from_days(&[0, 1, 2, 3]).unwrap();
        minimap_store::settings::update(
            &mut conn,
            minimap_types::UpdateSettings {
                work_week: Some(four),
                ..Default::default()
            },
        )
        .unwrap();
        let s = schedule_impl(&conn, ScheduleScope::Portfolio).unwrap();
        assert_eq!(days_off(&s, four), 0, "no Friday on the axis");
        // Twenty days of work take five weeks of four days instead of four weeks of five.
        let finish = s.projects[0].projected_finish.unwrap();
        let before = default.projects[0].projected_finish.unwrap();
        assert!(finish > before, "{finish} should be later than {before}");
    }

    #[test]
    fn a_projects_schedule_follows_its_blocks_links() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let p = minimap_store::projects::create(
            &mut conn,
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
        .id;
        let a = task(&mut conn, "Design", p, 2.0);
        let b = task(&mut conn, "Build", p, 3.0);
        task(&mut conn, "Docs", p, 1.0);
        blocks(&mut conn, a, b);

        let s = schedule_impl(&conn, ScheduleScope::Project(p)).unwrap();
        assert_eq!(s.tasks.len(), 3);
        let build = s.tasks.iter().find(|t| t.title == "Build").unwrap();
        assert_eq!((build.es, build.ef), (2.0, 5.0));
        let critical: Vec<&str> = s
            .tasks
            .iter()
            .filter(|t| t.critical)
            .map(|t| t.title.as_str())
            .collect();
        assert_eq!(critical, ["Design", "Build"]);
        assert_eq!(s.projects[0].open_tasks, 3);
        assert!(s.projects[0].projected_finish.is_some());

        let all = schedule_impl(&conn, ScheduleScope::Portfolio).unwrap();
        assert_eq!(all.tasks.len(), 3);
        // Archived tasks drop out.
        minimap_store::nodes::archive(&mut conn, NodeRef::new(NodeType::Task, a)).unwrap();
        let after = schedule_impl(&conn, ScheduleScope::Project(p)).unwrap();
        assert_eq!(after.tasks.len(), 2);
        assert_eq!(
            after.tasks.iter().find(|t| t.title == "Build").unwrap().es,
            0.0
        );
    }

    #[test]
    fn an_unknown_project_is_not_found() {
        let conn = minimap_store::open_in_memory().unwrap();
        let e = schedule_impl(&conn, ScheduleScope::Project(Uuid::nil())).unwrap_err();
        assert_eq!(e.code, "not_found");
    }
}
