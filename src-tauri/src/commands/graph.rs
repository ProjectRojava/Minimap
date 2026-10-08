use minimap_core::dependency_graph::{build, GraphError, GraphInput};
use minimap_store::Connection;
use minimap_types::{AppError, DependencyGraph, GraphFilter};
use tauri::State;

use crate::{
    error::{app_error, store_error},
    state::AppState,
};

/// The dependency graph, laid out left to right: tasks joined by `blocks`, or projects joined by
/// `depends_on`, narrowed by project, team or objective (what they link to is shown dimmed), with
/// the critical path marked. Read-only.
#[tauri::command]
pub async fn get_dependency_graph(
    state: State<'_, AppState>,
    filter_by: GraphFilter,
) -> Result<DependencyGraph, AppError> {
    state.run(move |conn| graph_impl(conn, &filter_by)).await
}

pub(crate) fn graph_impl(
    conn: &Connection,
    filter: &GraphFilter,
) -> Result<DependencyGraph, AppError> {
    let tasks = minimap_store::tasks::list(conn, false).map_err(store_error)?;
    let edges = minimap_store::edges::list_active(conn).map_err(store_error)?;
    let projects = minimap_store::projects::list(conn, false).map_err(store_error)?;
    let teams = minimap_store::teams::list(conn, false).map_err(store_error)?;
    let work_week = minimap_store::settings::get(conn)
        .map_err(store_error)?
        .work_week;
    build(
        &GraphInput {
            tasks: &tasks,
            edges: &edges,
            projects: &projects,
            teams: &teams,
            today: minimap_store::today(),
            work_week,
        },
        filter,
    )
    .map_err(|e| {
        let code = match e {
            GraphError::Cycle => "cycle",
            GraphError::TooLarge(_) => "invalid",
        };
        app_error(code, e.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{
        AssigneeChoice, CreateProject, CreateTask, EdgeType, GraphLevel, NewEdge, NodeRef,
        NodeType, Uuid,
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

    fn task(conn: &mut Connection, title: &str, days: f64, project: Uuid) -> Uuid {
        minimap_store::tasks::create(
            conn,
            CreateTask {
                links: Vec::new(),
                task_type: None,
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

    fn link(conn: &mut Connection, kind: EdgeType, nt: NodeType, a: Uuid, b: Uuid) {
        minimap_store::edges::add(
            conn,
            NewEdge {
                edge_type: kind,
                from: NodeRef::new(nt, a),
                to: NodeRef::new(nt, b),
                attrs: serde_json::json!({}),
            },
        )
        .unwrap();
    }

    #[test]
    fn the_graph_reads_the_plan_and_marks_the_critical_path() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let p = project(&mut conn, "API");
        let a = task(&mut conn, "Design", 2.0, p);
        let b = task(&mut conn, "Build", 3.0, p);
        task(&mut conn, "Lonely", 1.0, p);
        link(&mut conn, EdgeType::Blocks, NodeType::Task, a, b);
        let g = graph_impl(&conn, &GraphFilter::default()).unwrap();
        assert_eq!(g.nodes.len(), 2);
        assert_eq!(g.edges.len(), 1);
        assert_eq!(g.hidden_unlinked, 1);
        assert!(g.nodes.iter().all(|n| n.critical));
        assert!(
            g.nodes.iter().find(|n| n.label == "Design").unwrap().x
                < g.nodes.iter().find(|n| n.label == "Build").unwrap().x
        );
        // The project filter keeps the same two.
        let f = graph_impl(
            &conn,
            &GraphFilter {
                project_id: Some(p),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(f.nodes.len(), 2);
    }

    #[test]
    fn the_project_level_uses_depends_on() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let core = project(&mut conn, "Core");
        let app = project(&mut conn, "App");
        link(&mut conn, EdgeType::DependsOn, NodeType::Project, app, core);
        let g = graph_impl(
            &conn,
            &GraphFilter {
                level: GraphLevel::Projects,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(g.level, GraphLevel::Projects);
        assert_eq!(g.nodes.len(), 2);
        let core_x = g.nodes.iter().find(|n| n.label == "Core").unwrap().x;
        let app_x = g.nodes.iter().find(|n| n.label == "App").unwrap().x;
        assert!(core_x < app_x, "what is needed first is on the left");
    }

    #[test]
    fn an_empty_workspace_gives_an_empty_graph() {
        let conn = minimap_store::open_in_memory().unwrap();
        let g = graph_impl(&conn, &GraphFilter::default()).unwrap();
        assert!(g.nodes.is_empty() && g.edges.is_empty());
    }
}
