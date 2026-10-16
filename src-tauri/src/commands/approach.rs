//! Approaching (spec 45): trouble in other projects that will reach what you are looking at.

use std::collections::HashSet;

use minimap_core::approach::{approaching, ApproachWorld};
use minimap_store::Connection;
use minimap_types::{
    AppError, ApproachReport, ApproachScope, Date, HealthLevel, ScheduleScope, Uuid,
};
use tauri::State;

use crate::{
    commands::{overview::overview_at, schedule::schedule_at},
    error::store_error,
    state::AppState,
};

/// The trouble outside `scope` that will reach it along `blocks` and `depends_on` links, nearest
/// first. Read-only. An unknown project or objective is `not_found`.
#[tauri::command]
pub async fn get_approaching(
    state: State<'_, AppState>,
    scope: ApproachScope,
) -> Result<ApproachReport, AppError> {
    state
        .run(move |conn| approaching_at(conn, scope, minimap_store::today()))
        .await
}

/// [`get_approaching`] as of `today` (the tests and the demo snapshots fix it).
pub(crate) fn approaching_at(
    conn: &mut Connection,
    scope: ApproachScope,
    today: Date,
) -> Result<ApproachReport, AppError> {
    match scope {
        ApproachScope::Project(id) => {
            minimap_store::projects::get(conn, id).map_err(store_error)?;
        }
        ApproachScope::Objective(id) => {
            minimap_store::objectives::get(conn, id).map_err(store_error)?;
        }
        ApproachScope::Me => {}
    }
    let schedule = match schedule_at(conn, today, ScheduleScope::Portfolio) {
        Ok(s) => s,
        // A loop in the links (which sync repairs) leaves nothing to say rather than an error.
        Err(e) if e.code == "cycle" => return Ok(ApproachReport::default()),
        Err(e) => return Err(e),
    };
    let tasks = minimap_store::tasks::list_planned(conn).map_err(store_error)?;
    let edges = minimap_store::edges::list_active(conn).map_err(store_error)?;
    let projects = minimap_store::projects::list(conn, false).map_err(store_error)?;
    let self_id: Option<Uuid> = minimap_store::people::get_self(conn)
        .map_err(store_error)?
        .map(|p| p.id);
    // Red projects come from the same health the Overview shows.
    let overview = overview_at(conn, today)?;
    let red: HashSet<Uuid> = overview
        .objectives
        .iter()
        .flat_map(|o| o.projects.iter())
        .chain(overview.unlinked_projects.iter())
        .filter(|p| p.health.level == HealthLevel::Red)
        .map(|p| p.project.node.id)
        .collect();
    Ok(approaching(
        &ApproachWorld {
            tasks: &tasks,
            edges: &edges,
            projects: &projects,
            schedule: &schedule,
            red_projects: &red,
            self_id,
            today,
        },
        scope,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{
        AssigneeChoice, CreateObjective, CreatePerson, CreateProject, CreateTask, EdgeType,
        NewEdge, NodeRef, NodeType, Reason, Severity, TaskStatus,
    };
    use time::macros::date;

    // Monday 2027-03-01.
    const TODAY: Date = date!(2027 - 03 - 01);

    fn project(conn: &mut Connection, title: &str, target: Option<Date>) -> Uuid {
        minimap_store::projects::create(
            conn,
            CreateProject {
                title: title.into(),
                slug: None,
                description: String::new(),
                owner_person_id: None,
                start_date: None,
                target_date: target,
                status: Some(minimap_types::ProjectStatus::Active),
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
        status: TaskStatus,
        assignee: AssigneeChoice,
    ) -> Uuid {
        minimap_store::tasks::create(
            conn,
            CreateTask {
                links: Vec::new(),
                task_type: None,
                focus: None,
                start_minute: None,
                length_minutes: None,
                title: title.into(),
                assignee,
                description: String::new(),
                project_id: Some(project),
                status: Some(status),
                estimate_days: Some(2.0),
                start_date: None,
                due_date: None,
                priority: None,
                recurrence: None,
            },
        )
        .unwrap()
        .id
    }

    fn link(conn: &mut Connection, edge_type: EdgeType, from: NodeRef, to: NodeRef) {
        minimap_store::edges::add(
            conn,
            NewEdge {
                edge_type,
                from,
                to,
                attrs: serde_json::json!({}),
            },
        )
        .unwrap();
    }

    fn t(id: Uuid) -> NodeRef {
        NodeRef::new(NodeType::Task, id)
    }

    /// There's blocked task "Gateway auth" blocks Here's "Pen test"; Here has a tight target.
    fn world() -> (Connection, Uuid, Uuid, Uuid, Uuid) {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let here = project(&mut conn, "Here", Some(date!(2027 - 03 - 03)));
        let there = project(&mut conn, "There", None);
        let gate = task(
            &mut conn,
            "Gateway auth",
            there,
            TaskStatus::Blocked,
            AssigneeChoice::Nobody,
        );
        let pen = task(
            &mut conn,
            "Pen test",
            here,
            TaskStatus::Todo,
            AssigneeChoice::Nobody,
        );
        link(&mut conn, EdgeType::Blocks, t(gate), t(pen));
        (conn, here, there, gate, pen)
    }

    #[test]
    fn a_project_hears_about_blocked_work_in_another_that_reaches_it() {
        let (mut conn, here, there, gate, pen) = world();
        let r = approaching_at(&mut conn, ApproachScope::Project(here), TODAY).unwrap();
        let headline = r.headline.expect("a headline");
        assert_eq!(headline.count, 1);
        assert_eq!(r.groups.len(), 1);
        assert_eq!(r.groups[0].source_project.as_ref().unwrap().node.id, there);
        let a = &r.groups[0].items[0];
        assert_eq!((a.source.node.id, a.target.node.id), (gate, pen));
        // Blocked, and (because what it blocks cannot meet Here's target) with no slack either.
        assert_eq!(a.reasons, [Reason::Blocked, Reason::NoSlack]);
        // The blocked task drives a task that cannot meet a three-day target: it hits.
        assert_eq!(a.severity, Severity::Hits);
        // The other project hears nothing from the first: nothing blocks its work.
        let none = approaching_at(&mut conn, ApproachScope::Project(there), TODAY).unwrap();
        assert!(none.groups.is_empty() && none.headline.is_none());
    }

    #[test]
    fn finishing_the_blocker_clears_it() {
        let (mut conn, here, _, gate, _) = world();
        minimap_store::tasks::update(
            &mut conn,
            gate,
            minimap_types::UpdateTask {
                status: Some(TaskStatus::Done),
                ..Default::default()
            },
        )
        .unwrap();
        let r = approaching_at(&mut conn, ApproachScope::Project(here), TODAY).unwrap();
        assert!(r.groups.is_empty());
    }

    #[test]
    fn unknown_projects_and_objectives_are_not_found() {
        let (mut conn, ..) = world();
        for scope in [
            ApproachScope::Project(Uuid::from_u128(77)),
            ApproachScope::Objective(Uuid::from_u128(78)),
        ] {
            let err = approaching_at(&mut conn, scope, TODAY).unwrap_err();
            assert_eq!(err.code, "not_found", "{scope:?}");
        }
    }

    #[test]
    fn my_week_hears_about_trouble_ahead_of_the_tasks_assigned_to_me() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        // No "me" yet: nothing, and no error.
        let r = approaching_at(&mut conn, ApproachScope::Me, TODAY).unwrap();
        assert!(r.groups.is_empty());
        let me = minimap_store::people::create(
            &mut conn,
            CreatePerson {
                name: "Me".into(),
                role_title: String::new(),
                email: None,
                weekly_capacity_hours: None,
                is_self: true,
                notes: String::new(),
            },
        )
        .unwrap()
        .id;
        let here = project(&mut conn, "Here", Some(date!(2027 - 03 - 03)));
        let there = project(&mut conn, "There", None);
        let gate = task(
            &mut conn,
            "Gateway auth",
            there,
            TaskStatus::Blocked,
            AssigneeChoice::Nobody,
        );
        let mine = task(
            &mut conn,
            "Mine",
            here,
            TaskStatus::Todo,
            AssigneeChoice::Person(me),
        );
        let theirs = task(
            &mut conn,
            "Theirs",
            here,
            TaskStatus::Todo,
            AssigneeChoice::Nobody,
        );
        link(&mut conn, EdgeType::Blocks, t(gate), t(mine));
        link(&mut conn, EdgeType::Blocks, t(gate), t(theirs));
        let r = approaching_at(&mut conn, ApproachScope::Me, TODAY).unwrap();
        let targets: Vec<&str> = r
            .groups
            .iter()
            .flat_map(|g| g.items.iter())
            .map(|a| a.target.label.as_str())
            .collect();
        assert_eq!(targets, ["Mine"]);
    }

    #[test]
    fn an_objective_hears_about_trouble_ahead_of_its_projects() {
        let (mut conn, here, _, _, _) = world();
        let objective = minimap_store::objectives::create(
            &mut conn,
            CreateObjective {
                title: "Launch".into(),
                description: String::new(),
                target_date: None,
                status: None,
                priority: None,
                ongoing: false,
                review_every_days: None,
            },
        )
        .unwrap()
        .id;
        let r = approaching_at(&mut conn, ApproachScope::Objective(objective), TODAY).unwrap();
        assert!(r.groups.is_empty(), "nothing feeds it yet");
        link(
            &mut conn,
            EdgeType::ContributesTo,
            NodeRef::new(NodeType::Project, here),
            NodeRef::new(NodeType::Objective, objective),
        );
        let r = approaching_at(&mut conn, ApproachScope::Objective(objective), TODAY).unwrap();
        assert_eq!(r.headline.map(|h| h.count), Some(1));
    }
}
