//! Fog of war (spec 43, ADR-0019): how long ago what is in play was last touched or confirmed.

use std::collections::HashMap;

use minimap_core::fog::{build, FogInput};
use minimap_store::Connection;
use minimap_types::{AppError, Date, EdgeType, FogReport, NodeRef};
use tauri::State;

use crate::{error::store_error, state::AppState};

/// The items in play that have gone quiet, and how much of what is in play is clear. Read-only;
/// the thresholds come from Settings.
#[tauri::command]
pub async fn get_fog(state: State<'_, AppState>) -> Result<FogReport, AppError> {
    state.run(|conn| fog_at(conn, minimap_store::today())).await
}

/// [`get_fog`] as of `today` (the tests and the demo snapshots fix it).
pub(crate) fn fog_at(conn: &Connection, today: Date) -> Result<FogReport, AppError> {
    let settings = minimap_store::settings::get(conn).map_err(store_error)?;
    let tasks = minimap_store::tasks::list(conn, false).map_err(store_error)?;
    let projects = minimap_store::projects::list(conn, false).map_err(store_error)?;
    let objectives = minimap_store::objectives::list(conn, false).map_err(store_error)?;
    let contributes: Vec<_> =
        minimap_store::edges::list_active_of_type(conn, EdgeType::ContributesTo)
            .map_err(store_error)?
            .into_iter()
            .map(|e| (e.from_id, e.to_id))
            .collect();
    let links: HashMap<_, _> =
        minimap_store::views::last_link_changes(conn).map_err(store_error)?;
    let mentions = minimap_store::views::last_mentions(conn).map_err(store_error)?;
    Ok(build(&FogInput {
        today,
        thresholds: settings.fog,
        tasks: &tasks,
        projects: &projects,
        objectives: &objectives,
        contributes: &contributes,
        last_link_change: &links,
        last_mention: &mentions,
    }))
}

/// Says that a task, project or objective is still accurate: it clears its fog without changing
/// anything. One logged step (Ctrl/Cmd+Z brings the fog back).
#[tauri::command]
pub async fn confirm_item(state: State<'_, AppState>, node: NodeRef) -> Result<(), AppError> {
    state
        .run(move |conn| confirm_at(conn, node, minimap_store::today()))
        .await
}

pub(crate) fn confirm_at(
    conn: &mut Connection,
    node: NodeRef,
    today: Date,
) -> Result<(), AppError> {
    minimap_store::fog::confirm(conn, node, today).map_err(store_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{
        AssigneeChoice, CreateProject, CreateTask, FogLevel, FogReason, NodeType, ProjectStatus,
        TaskStatus, Uuid,
    };
    use time::{macros::date, Duration};

    // Wednesday 2027-03-03, as in the demo snapshots.
    const TODAY: Date = date!(2027 - 03 - 03);

    fn backdate(conn: &Connection, table: &str, id: Uuid, days_ago: i64) {
        let at = format!("{}T09:00:00.000Z", TODAY - Duration::days(days_ago));
        conn.execute(
            &format!("UPDATE {table} SET updated_at = ?2 WHERE id = ?1"),
            [id.to_string(), at],
        )
        .unwrap();
    }

    fn task(conn: &mut Connection, title: &str, status: TaskStatus, project: Option<Uuid>) -> Uuid {
        let project = Some(project.unwrap_or_else(|| minimap_store::fixtures::project(conn)));
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
                project_id: project,
                status: Some(status),
                estimate_days: None,
                start_date: None,
                due_date: None,
                priority: None,
                recurrence: None,
            },
        )
        .unwrap()
        .id
    }

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
                status: Some(ProjectStatus::Active),
                priority: None,
            },
        )
        .unwrap()
        .id
    }

    /// A small world at TODAY: a project whose only moving task is old (fog), a fresh task, a
    /// task 9 days old (haze), a far-off task nobody touched (not in play).
    fn world() -> (Connection, Uuid, Uuid, Uuid, Uuid) {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let p = project(&mut conn, "Platform");
        let old = task(&mut conn, "Old and moving", TaskStatus::InProgress, Some(p));
        let fresh = task(&mut conn, "Fresh", TaskStatus::InProgress, None);
        let hazy = task(&mut conn, "Hazy", TaskStatus::Blocked, None);
        let far = task(&mut conn, "Far off", TaskStatus::Todo, None);
        backdate(&conn, "projects", p, 25);
        backdate(&conn, "tasks", old, 19);
        backdate(&conn, "tasks", fresh, 1);
        backdate(&conn, "tasks", hazy, 9);
        backdate(&conn, "tasks", far, 90);
        // The tasks' own creation also left activity rows; the fog reads `updated_at`, so these
        // are what it sees.
        (conn, p, old, hazy, fresh)
    }

    #[test]
    fn the_fog_at_a_fixed_date_lists_what_has_gone_quiet() {
        let (conn, p, old, hazy, fresh) = world();
        let report = fog_at(&conn, TODAY).unwrap();
        let marks: Vec<(Uuid, FogLevel, u32)> = report
            .marks
            .iter()
            .map(|m| (m.node.id, m.level, m.age_days))
            .collect();
        // Fog first, then haze (oldest first). The project was touched 25 days ago, but its only
        // task in play is the old one (19 days): that is how long ago it was last watched, and a
        // project hazes after 10 days and fogs after 21.
        assert_eq!(
            marks,
            [
                (old, FogLevel::Fog, 19),
                (p, FogLevel::Haze, 19),
                (hazy, FogLevel::Haze, 9)
            ]
        );
        assert!(report.mark(fresh).is_none());
        // In play: two projects and three tasks; the far-off one is not.
        // (The project of the three loose tasks counts too: five in play, two of them clear.)
        assert_eq!((report.in_play, report.clear), (5, 2));
        assert_eq!(report.marks[1].node.node_type, NodeType::Project);
        assert_eq!(report.marks[0].reason, FogReason::NoChange);
        assert!(report.visibility.is_some());
    }

    #[test]
    fn confirming_clears_an_item_and_a_threshold_change_moves_the_levels() {
        let (mut conn, _, old, hazy, _) = world();
        confirm_at(&mut conn, NodeRef::new(NodeType::Task, old), TODAY).unwrap();
        let report = fog_at(&conn, TODAY).unwrap();
        assert!(report.mark(old).is_none());
        assert!(report.mark(hazy).is_some());
        // Settings: task haze 30 clears the hazy one at once.
        minimap_store::settings::update(
            &mut conn,
            minimap_types::UpdateSettings {
                fog: Some(minimap_types::FogThresholds {
                    task_haze: 30,
                    task_fog: 60,
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(fog_at(&conn, TODAY).unwrap().mark(hazy).is_none());
    }

    #[test]
    fn confirming_the_wrong_kind_of_node_is_invalid() {
        let (mut conn, ..) = world();
        let me = minimap_store::people::get_self(&conn)
            .unwrap()
            .map(|p| p.id);
        let bogus = NodeRef::new(NodeType::Person, me.unwrap_or_else(Uuid::nil));
        let err = confirm_at(&mut conn, bogus, TODAY).unwrap_err();
        assert_eq!(err.code, "invalid");
    }
}
