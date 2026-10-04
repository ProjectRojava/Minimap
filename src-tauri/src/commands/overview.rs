use minimap_core::overview::{build, OverviewWorld};
use minimap_store::Connection;
use minimap_types::{AppError, PortfolioOverview, WaitingOnFilter};
use tauri::State;

use crate::{commands::waiting_on::list_impl, error::store_error, state::AppState};

/// The whole portfolio at a glance: objectives with computed health and their projects, the top
/// risks, overloaded people and stale waiting-ons. Read-only; thresholds come from Settings.
#[tauri::command]
pub async fn get_portfolio_overview(
    state: State<'_, AppState>,
) -> Result<PortfolioOverview, AppError> {
    state.run(overview_impl).await
}

pub(crate) fn overview_impl(conn: &mut Connection) -> Result<PortfolioOverview, AppError> {
    let conn: &Connection = conn;
    let settings = minimap_store::settings::get(conn).map_err(store_error)?;
    let tasks = minimap_store::tasks::list(conn, false).map_err(store_error)?;
    let edges = minimap_store::edges::list_active(conn).map_err(store_error)?;
    let projects = minimap_store::projects::list(conn, false).map_err(store_error)?;
    let objectives = minimap_store::objectives::list(conn, false).map_err(store_error)?;
    let people = minimap_store::people::list(conn, false).map_err(store_error)?;
    let today = minimap_store::today();
    let mut overview = build(&OverviewWorld {
        tasks: &tasks,
        edges: &edges,
        projects: &projects,
        objectives: &objectives,
        people: &people,
        today,
        hours_per_day: settings.hours_per_day,
        thresholds: settings.health,
    });
    // Stale waiting-ons follow their own rules (age setting, expected date); oldest first.
    overview.stale_waiting = list_impl(conn, &WaitingOnFilter::default(), today)?
        .into_iter()
        .filter(|w| w.stale)
        .collect();
    Ok(overview)
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{
        AssigneeChoice, CreateObjective, CreatePerson, CreateProject, CreateTask, CreateWaitingOn,
        EdgeType, HealthLevel, HealthThresholds, NewEdge, NodeRef, NodeType, UpdateSettings, Uuid,
    };
    use time::Duration;

    fn project(conn: &mut Connection, title: &str, target: Option<time::Date>) -> Uuid {
        minimap_store::projects::create(
            conn,
            CreateProject {
                title: title.into(),
                slug: None,
                description: String::new(),
                owner_person_id: None,
                start_date: None,
                target_date: target,
                status: None,
                priority: None,
            },
        )
        .unwrap()
        .id
    }

    fn task(conn: &mut Connection, title: &str, project: Uuid, due: Option<time::Date>) -> Uuid {
        minimap_store::tasks::create(
            conn,
            CreateTask {
                title: title.into(),
                assignee: AssigneeChoice::Nobody,
                description: String::new(),
                project_id: Some(project),
                status: None,
                estimate_days: Some(2.0),
                start_date: None,
                due_date: due,
                priority: None,
            },
        )
        .unwrap()
        .id
    }

    fn person(conn: &mut Connection, name: &str) -> Uuid {
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
    }

    #[test]
    fn the_overview_gathers_health_risks_and_stale_waiting_ons() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let today = minimap_store::today();
        // A project whose target has already passed, with an overdue task.
        let p = project(&mut conn, "Launch", Some(today - Duration::days(14)));
        task(&mut conn, "Late work", p, Some(today - Duration::days(10)));
        task(&mut conn, "More work", p, None);
        let o = minimap_store::objectives::create(
            &mut conn,
            CreateObjective {
                title: "Go EU".into(),
                description: String::new(),
                target_date: None,
                status: None,
                priority: Some(1),
            },
        )
        .unwrap()
        .id;
        minimap_store::edges::add(
            &mut conn,
            NewEdge {
                edge_type: EdgeType::ContributesTo,
                from: NodeRef::new(NodeType::Project, p),
                to: NodeRef::new(NodeType::Objective, o),
                attrs: serde_json::json!({ "weight": 1 }),
            },
        )
        .unwrap();
        let raj = person(&mut conn, "Raj");
        let w = minimap_store::waiting_on::create(
            &mut conn,
            CreateWaitingOn {
                description: "Security sign-off".into(),
                person_id: raj,
                asked_on: Some(today - Duration::days(30)),
                expected_by: None,
                follow_up_on: None,
            },
        )
        .unwrap();

        let ov = overview_impl(&mut conn).unwrap();
        assert_eq!(ov.objectives.len(), 1);
        let row = &ov.objectives[0];
        assert_eq!(row.objective.label, "Go EU");
        assert_eq!(row.projects[0].health.level, HealthLevel::Red);
        assert!(row.projects[0].health.reasons[0].text.contains("late"));
        assert_eq!(ov.risks[0].node.label, "Launch");
        assert_eq!(ov.risks[0].priority, 1, "lent by the objective");
        assert_eq!(ov.stale_waiting.len(), 1);
        assert_eq!(ov.stale_waiting[0].waiting.id, w.id);
        assert_eq!(ov.counts.red, 1);
        assert!(ov.warnings.is_empty());
        assert_eq!(ov.thresholds, HealthThresholds::default());
    }

    #[test]
    fn thresholds_from_settings_change_the_verdict() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let today = minimap_store::today();
        // Roughly a week late: red by default (5 working days), amber with a tolerant setting.
        let p = project(&mut conn, "Slip", Some(today - Duration::days(14)));
        task(&mut conn, "work", p, None);
        let level = |conn: &mut Connection| {
            overview_impl(conn).unwrap().unlinked_projects[0]
                .health
                .level
        };
        assert_eq!(level(&mut conn), HealthLevel::Red);
        minimap_store::settings::update(
            &mut conn,
            UpdateSettings {
                health: Some(HealthThresholds {
                    late_amber_days: 30,
                    late_red_days: 60,
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(level(&mut conn), HealthLevel::Green);
    }

    #[test]
    fn an_empty_workspace_has_an_empty_overview() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let ov = overview_impl(&mut conn).unwrap();
        assert!(ov.objectives.is_empty() && ov.risks.is_empty() && ov.stale_waiting.is_empty());
        assert_eq!(
            ov.counts.red + ov.counts.amber + ov.counts.green + ov.counts.idle,
            0
        );
    }

    /// Timing on a deliberately large plan (spec 15: under 200 ms). Slow to build, so it only
    /// runs on request: `cargo test -p minimap --release overview_speed -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn overview_speed() {
        use std::time::Instant;
        let mut conn = minimap_store::open_in_memory().unwrap();
        let today = minimap_store::today();
        let people: Vec<Uuid> = (0..30)
            .map(|i| person(&mut conn, &format!("Person {i}")))
            .collect();
        let me_edge = |conn: &mut Connection,
                       from: Uuid,
                       from_type: NodeType,
                       to: Uuid,
                       to_type: NodeType,
                       kind: EdgeType| {
            minimap_store::edges::add(
                conn,
                NewEdge {
                    edge_type: kind,
                    from: NodeRef::new(from_type, from),
                    to: NodeRef::new(to_type, to),
                    attrs: serde_json::json!({}),
                },
            )
            .unwrap();
        };
        let objectives: Vec<Uuid> = (0..8)
            .map(|i| {
                minimap_store::objectives::create(
                    &mut conn,
                    CreateObjective {
                        title: format!("Objective {i}"),
                        description: String::new(),
                        target_date: Some(today + Duration::days(60 + i * 10)),
                        status: None,
                        priority: Some(1 + (i % 5) as u8),
                    },
                )
                .unwrap()
                .id
            })
            .collect();
        for p in 0..40i64 {
            let proj = project(
                &mut conn,
                &format!("Project {p}"),
                Some(today + Duration::days(20 + p)),
            );
            me_edge(
                &mut conn,
                proj,
                NodeType::Project,
                objectives[(p % 8) as usize],
                NodeType::Objective,
                EdgeType::ContributesTo,
            );
            let mut prev: Option<Uuid> = None;
            for t in 0..50i64 {
                let due = (t % 7 == 0).then(|| today - Duration::days(t));
                let id = task(&mut conn, &format!("p{p} t{t}"), proj, due);
                if let Some(q) = prev.filter(|_| t % 3 != 0) {
                    me_edge(
                        &mut conn,
                        q,
                        NodeType::Task,
                        id,
                        NodeType::Task,
                        EdgeType::Blocks,
                    );
                }
                me_edge(
                    &mut conn,
                    id,
                    NodeType::Task,
                    people[((p + t) % 30) as usize],
                    NodeType::Person,
                    EdgeType::AssignedTo,
                );
                prev = Some(id);
            }
        }
        let start = Instant::now();
        let ov = overview_impl(&mut conn).unwrap();
        println!(
            "overview of 40 projects / 2000 tasks / 30 people: {:?} ({} risks, {} overloaded, counts {:?})",
            start.elapsed(),
            ov.risks.len(),
            ov.overloaded.len(),
            ov.counts
        );
    }
}
