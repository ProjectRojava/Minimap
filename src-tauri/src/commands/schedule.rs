use minimap_core::schedule::{compute, critical_path};
use minimap_store::Connection;
use minimap_types::{AppError, Schedule, ScheduleScope, ScheduledTask};
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
    if let ScheduleScope::Project(id) = scope {
        minimap_store::projects::get(conn, id).map_err(store_error)?;
    }
    let tasks = minimap_store::tasks::list(conn, false).map_err(store_error)?;
    let edges = minimap_store::edges::list_active_of_type(conn, minimap_types::EdgeType::Blocks)
        .map_err(store_error)?;
    let projects = minimap_store::projects::list(conn, false).map_err(store_error)?;
    compute(&tasks, &edges, &projects, minimap_store::today(), scope)
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
                title: title.into(),
                assignee: AssigneeChoice::Nobody,
                description: String::new(),
                project_id: Some(project),
                status: None,
                estimate_days: Some(days),
                start_date: None,
                due_date: None,
                priority: None,
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
