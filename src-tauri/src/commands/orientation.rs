//! Portfolio view and My view (docs/product-design/07-orientation.md, spec 50). Thin: the rules
//! are `minimap_core::orientation`. Screens whose contents are *computed* (the Overview, the
//! dependency graph, Capacity) take the view and compute in it; list screens ask for the scope
//! and filter by it.

use minimap_core::orientation::{my_scope, ScopeInput};
use minimap_store::Connection;
use minimap_types::{AppError, Date, MyScope};
use tauri::State;

use crate::{error::store_error, state::AppState};

/// Everything that touches you and why: your tasks, your projects, what reaches them, the
/// objectives you serve, your reports. Empty when there is no "me" yet.
#[tauri::command]
pub async fn get_my_scope(state: State<'_, AppState>) -> Result<MyScope, AppError> {
    state
        .run(|conn| my_scope_at(conn, minimap_store::today()))
        .await
}

/// [`get_my_scope`] as of `today` (the tests and the demo snapshots fix it).
pub(crate) fn my_scope_at(conn: &Connection, today: Date) -> Result<MyScope, AppError> {
    let me = minimap_store::people::get_self(conn).map_err(store_error)?;
    // Meetings count (when assigned to you), so every task, not only the planned ones.
    let tasks = minimap_store::tasks::list(conn, false).map_err(store_error)?;
    let projects = minimap_store::projects::list(conn, false).map_err(store_error)?;
    let objectives = minimap_store::objectives::list(conn, false).map_err(store_error)?;
    let people = minimap_store::people::list(conn, false).map_err(store_error)?;
    let edges = minimap_store::edges::list_active(conn).map_err(store_error)?;
    Ok(my_scope(&ScopeInput {
        self_id: me.map(|p| p.id),
        today,
        tasks: &tasks,
        projects: &projects,
        objectives: &objectives,
        people: &people,
        edges: &edges,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{
        AssigneeChoice, CreateProject, CreateTask, EdgeType, NewEdge, NodeRef, NodeType, Why,
    };
    use time::macros::date;

    // Wednesday 2027-03-03.
    const TODAY: Date = date!(2027 - 03 - 03);

    #[test]
    fn there_is_no_scope_before_first_run() {
        let conn = minimap_store::open_in_memory().unwrap();
        assert!(my_scope_at(&conn, TODAY).unwrap().is_empty());
    }

    #[test]
    fn my_tasks_and_what_blocks_them_are_in_and_everything_else_is_not() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        minimap_store::people::ensure_self(&mut conn, "Me").unwrap();
        let task = |conn: &mut Connection, title: &str, who: AssigneeChoice| {
            let __p = minimap_store::fixtures::project(conn);
            minimap_store::tasks::create(
                conn,
                CreateTask {
                    links: Vec::new(),
                    task_type: None,
                    focus: None,
                    start_minute: None,
                    length_minutes: None,
                    title: title.into(),
                    assignee: who,
                    description: String::new(),
                    project_id: Some(__p),
                    status: None,
                    estimate_days: Some(1.0),
                    start_date: None,
                    due_date: None,
                    priority: None,
                    recurrence: None,
                },
            )
            .unwrap()
            .id
        };
        let mine = task(&mut conn, "Mine", AssigneeChoice::Me);
        let blocker = task(&mut conn, "Blocker", AssigneeChoice::Nobody);
        let other = task(&mut conn, "Other", AssigneeChoice::Nobody);
        minimap_store::edges::add(
            &mut conn,
            NewEdge {
                edge_type: EdgeType::Blocks,
                from: NodeRef::new(NodeType::Task, blocker),
                to: NodeRef::new(NodeType::Task, mine),
                attrs: serde_json::json!({}),
            },
        )
        .unwrap();
        let scope = my_scope_at(&conn, TODAY).unwrap();
        assert_eq!(scope.tasks[&mine], vec![Why::Assigned]);
        assert_eq!(scope.tasks[&blocker], vec![Why::Reaches { via: mine }]);
        assert!(!scope.tasks.contains_key(&other));
        assert_eq!(scope.people.len(), 1);
    }
    /// You own project P (a task for Ann, one for you) and someone else owns Q (a task for Bob).
    fn two_projects() -> (Connection, [minimap_types::Uuid; 2]) {
        use minimap_types::CreatePerson;
        let mut conn = minimap_store::open_in_memory().unwrap();
        let me = minimap_store::people::ensure_self(&mut conn, "Me")
            .unwrap()
            .id;
        let person = |conn: &mut Connection, name: &str| {
            minimap_store::people::create(
                conn,
                CreatePerson {
                    name: name.into(),
                    role_title: String::new(),
                    email: None,
                    weekly_capacity_hours: None,
                    is_self: false,
                    notes: String::new(),
                },
            )
            .unwrap()
            .id
        };
        let (ann, bob) = (person(&mut conn, "Ann"), person(&mut conn, "Bob"));
        let project = |conn: &mut Connection, title: &str, owner: Option<minimap_types::Uuid>| {
            minimap_store::projects::create(
                conn,
                CreateProject {
                    title: title.into(),
                    slug: None,
                    description: String::new(),
                    owner_person_id: owner,
                    start_date: None,
                    target_date: None,
                    status: None,
                    priority: None,
                },
            )
            .unwrap()
            .id
        };
        let (p, q) = (
            project(&mut conn, "P", Some(me)),
            project(&mut conn, "Q", Some(bob)),
        );
        let task = |conn: &mut Connection, title: &str, project, who| {
            minimap_store::tasks::create(
                conn,
                CreateTask {
                    links: Vec::new(),
                    task_type: None,
                    focus: None,
                    start_minute: None,
                    length_minutes: None,
                    title: title.into(),
                    assignee: AssigneeChoice::Person(who),
                    description: String::new(),
                    project_id: Some(project),
                    status: None,
                    estimate_days: Some(2.0),
                    start_date: None,
                    due_date: None,
                    priority: None,
                    recurrence: None,
                },
            )
            .unwrap()
            .id
        };
        task(&mut conn, "Ann in P", p, ann);
        task(&mut conn, "Mine in P", p, me);
        task(&mut conn, "Bob in Q", q, bob);
        (conn, [p, q])
    }

    #[test]
    fn the_overview_in_my_view_lists_only_my_projects_and_portfolio_view_is_unchanged() {
        use minimap_types::View;
        let (conn, [p, q]) = two_projects();
        let all =
            crate::commands::overview::overview_in_view(&conn, TODAY, View::Portfolio).unwrap();
        let same = crate::commands::overview::overview_at(&conn, TODAY).unwrap();
        assert_eq!(all, same, "Portfolio view is the overview as it always was");
        let ids = |o: &minimap_types::PortfolioOverview| -> Vec<minimap_types::Uuid> {
            o.unlinked_projects
                .iter()
                .map(|r| r.project.node.id)
                .collect()
        };
        assert!(ids(&all).contains(&p) && ids(&all).contains(&q));
        let mine = crate::commands::overview::overview_in_view(&conn, TODAY, View::Mine).unwrap();
        assert_eq!(ids(&mine), vec![p]);
    }

    #[test]
    fn graph_and_capacity_in_my_view_follow_the_scope() {
        use minimap_core::capacity::in_my_view;
        use minimap_types::GraphFilter;
        let (conn, [p, _q]) = two_projects();
        let scope = my_scope_at(&conn, TODAY).unwrap();
        assert!(scope.projects.contains_key(&p));
        // Capacity: you, then Ann (on your projects); Bob works elsewhere.
        let mut capacity =
            crate::commands::capacity::capacity_at(&conn, TODAY, None, None, Some(2)).unwrap();
        assert_eq!(capacity.people.len(), 3);
        in_my_view(&mut capacity, &scope);
        let names: Vec<(&str, bool)> = capacity
            .people
            .iter()
            .map(|c| (c.person.label.as_str(), c.on_my_projects))
            .collect();
        assert_eq!(names, [("Me", false), ("Ann", true)]);
        assert!(capacity.mine);
        // The graph asks for the scope itself when the filter says mine (no links here, so
        // isolated nodes are included to see which tasks are in).
        let g = crate::commands::graph::graph_at(
            &conn,
            &GraphFilter {
                mine: true,
                include_isolated: true,
                ..Default::default()
            },
            TODAY,
        )
        .unwrap();
        let mut titles: Vec<&str> = g.nodes.iter().map(|n| n.label.as_str()).collect();
        titles.sort();
        assert_eq!(titles, ["Ann in P", "Mine in P"]);
    }
}
