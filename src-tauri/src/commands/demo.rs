//! Demo data (spec 24): fills an empty database with a realistic dataset. Developer-only: the
//! command refuses in release builds and the Settings tab that offers it is not shown there.

use minimap_types::{AppError, Date, DemoSummary};
use tauri::State;

use crate::{
    error::{app_error, store_error},
    state::AppState,
};

/// Why demo data can't be added right now, if it can't: not a debug build, or Google Drive is
/// connected (the demo data would be saved to the user's Drive and reach their other computers).
fn refusal(debug_build: bool, drive_connected: bool) -> Option<AppError> {
    if !debug_build {
        return Some(app_error(
            "invalid",
            "Demo data is only available in debug builds",
        ));
    }
    if drive_connected {
        return Some(app_error(
            "invalid",
            "Disconnect Google Drive first: demo data would be saved to your Drive and copied to your other computers",
        ));
    }
    None
}

/// Adds the demo dataset (2 objectives, 3 projects, 40 tasks, 8 people, ...) to an empty
/// database, dated around today. Debug builds only.
#[tauri::command]
pub async fn seed_demo_data(state: State<'_, AppState>) -> Result<DemoSummary, AppError> {
    if let Some(e) = refusal(cfg!(debug_assertions), state.sync.engine.is_connected()) {
        return Err(e);
    }
    state
        .run(|conn| seed_impl(conn, minimap_store::today()))
        .await
}

pub(crate) fn seed_impl(
    conn: &mut minimap_store::Connection,
    today: Date,
) -> Result<DemoSummary, AppError> {
    minimap_store::demo::seed(conn, today).map_err(store_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_builds_and_connected_drives_are_refused_with_a_reason() {
        assert!(refusal(true, false).is_none());
        let release = refusal(false, false).unwrap();
        assert_eq!(release.code, "invalid");
        assert!(release.message.contains("debug builds"));
        let drive = refusal(true, true).unwrap();
        assert!(drive.message.contains("Disconnect Google Drive"));
    }

    #[test]
    fn a_database_that_already_has_data_answers_invalid_and_keeps_it() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let today = minimap_store::today();
        seed_impl(&mut conn, today).unwrap();
        let err = seed_impl(&mut conn, today).unwrap_err();
        assert_eq!(err.code, "invalid");
        assert!(err.message.contains("empty database"));
    }

    // ---------------------------------------------------------------------------------------
    // Snapshot tests on the demo dataset (specs 13, 14, 15 and 19). The dataset is seeded for a
    // fixed Wednesday, so every date, working-day count and sentence below is stable. To review
    // a change: `INSTA_UPDATE=always cargo test -p minimap demo`, then read the `.snap` diff.

    use std::fmt::Write as _;

    use minimap_core::schedule::critical_path;
    use minimap_types::{
        HealthLevel, NodeRef, NodeType, ReportKind, ReportParams, ScheduleScope, Slip,
    };
    use time::macros::date;

    use crate::commands::{
        impact::with_world_at,
        overview::overview_at,
        review::{report_at, review_at},
        schedule::schedule_at,
    };

    /// A Wednesday. The seed's "this week" is the one starting Monday 2027-03-01.
    const TODAY: Date = date!(2027 - 03 - 03);

    fn demo() -> minimap_store::Connection {
        let mut conn = minimap_store::open_in_memory().unwrap();
        seed_impl(&mut conn, TODAY).unwrap();
        conn
    }

    fn project_id(conn: &minimap_store::Connection, title: &str) -> minimap_types::Uuid {
        minimap_store::projects::list(conn, false)
            .unwrap()
            .into_iter()
            .find(|p| p.title == title)
            .unwrap()
            .id
    }

    fn task_id(conn: &minimap_store::Connection, title: &str) -> minimap_types::Uuid {
        minimap_store::tasks::list(conn, false)
            .unwrap()
            .into_iter()
            .find(|t| t.title == title)
            .unwrap()
            .id
    }

    fn opt<T: std::fmt::Display>(v: Option<T>) -> String {
        v.map_or("-".to_owned(), |v| v.to_string())
    }

    #[test]
    fn the_schedule_and_critical_path_of_the_demo_data() {
        let conn = demo();
        let portfolio = schedule_at(&conn, TODAY, ScheduleScope::Portfolio).unwrap();
        let mut out = String::from("PROJECTS\n");
        for f in &portfolio.projects {
            let _ = writeln!(
                out,
                "{}: finish {} target {} late {} wd; {} open, {} unestimated, {} critical",
                f.title,
                opt(f.projected_finish),
                opt(f.target_date),
                opt(f.late_by_days),
                f.open_tasks,
                f.unestimated_tasks,
                f.critical_tasks
            );
        }
        for project in [
            "EU Region",
            "Security and Compliance",
            "Platform Cost Reduction",
        ] {
            let id = project_id(&conn, project);
            let s = schedule_at(&conn, TODAY, ScheduleScope::Project(id)).unwrap();
            let _ = writeln!(out, "\nCRITICAL PATH: {project}");
            for t in critical_path(&s) {
                let _ = writeln!(
                    out,
                    "  {} | {} -> {} | slack {:.1}",
                    t.title, t.start, t.finish, t.slack_days
                );
            }
        }
        insta::assert_snapshot!(out);
    }

    #[test]
    fn a_five_day_slip_on_the_eu_clusters_ripples_as_expected() {
        let conn = demo();
        let slip = Slip {
            node: NodeRef::new(
                NodeType::Task,
                task_id(&conn, "Provision EU network and clusters"),
            ),
            days: 5,
        };
        let report =
            with_world_at(&conn, TODAY, |w| minimap_core::impact::analyze(w, &[slip])).unwrap();
        let mut out = String::new();
        let s = &report.summary;
        let _ = writeln!(
            out,
            "SUMMARY: {} moved, {} absorbing, {} newly late tasks, {} projects, {} objectives, {} people, worst {:.1} wd",
            s.moved_tasks,
            s.absorbing_tasks,
            s.newly_late_tasks,
            s.newly_late_projects,
            s.newly_late_objectives,
            s.people,
            s.worst_delay_days
        );
        let _ = writeln!(out, "\nTASKS");
        for t in &report.tasks {
            let _ = writeln!(
                out,
                "  {}{} | in {:.1} absorbed {:.1} delay {:.1} | finish {} -> {} | due {} late {} -> {}{}",
                if t.direct { "* " } else { "" },
                t.title,
                t.incoming_days,
                t.absorbed_days,
                t.delay_days,
                t.old_finish,
                t.new_finish,
                opt(t.due_date),
                opt(t.late_before),
                opt(t.late_after),
                if t.newly_late { " NEWLY LATE" } else { "" }
            );
        }
        let _ = writeln!(out, "\nPROJECTS");
        for p in &report.projects {
            let _ = writeln!(
                out,
                "  {} | delay {:.1}, {} tasks | finish {} -> {} | target {} late {} -> {}{}",
                p.title,
                p.delay_days,
                p.affected_tasks,
                opt(p.old_finish),
                opt(p.new_finish),
                opt(p.target_date),
                opt(p.late_before),
                opt(p.late_after),
                if p.newly_late { " NEWLY LATE" } else { "" }
            );
        }
        let _ = writeln!(out, "\nOBJECTIVES");
        for o in &report.objectives {
            let _ = writeln!(
                out,
                "  {} | delay {:.1} | finish {} -> {} | target {} late {} -> {}{} | via {}",
                o.title,
                o.delay_days,
                opt(o.old_finish),
                opt(o.new_finish),
                opt(o.target_date),
                opt(o.late_before),
                opt(o.late_after),
                if o.newly_late { " NEWLY LATE" } else { "" },
                o.contributors.join(", ")
            );
        }
        let _ = writeln!(out, "\nPEOPLE");
        for p in &report.people {
            let _ = writeln!(
                out,
                "  {} | worst delay {:.1} | {}",
                p.name,
                p.delay_days,
                p.tasks
                    .iter()
                    .map(|t| format!("{} (+{:.1})", t.title, t.delay_days))
                    .collect::<Vec<_>>()
                    .join("; ")
            );
        }
        insta::assert_snapshot!(out);
    }

    #[test]
    fn the_overview_of_the_demo_data() {
        let mut conn = demo();
        let ov = overview_at(&mut conn, TODAY).unwrap();
        let mut out = String::new();
        let c = &ov.counts;
        let _ = writeln!(
            out,
            "COUNTS: {} red, {} amber, {} green, {} not scored",
            c.red, c.amber, c.green, c.idle
        );
        let health = |h: &minimap_types::Health| {
            format!(
                "{:?} {} [{}]",
                h.level,
                h.score,
                h.reasons
                    .iter()
                    .map(|r| r.text.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            )
        };
        for o in &ov.objectives {
            let _ = writeln!(
                out,
                "\nOBJECTIVE {} (P{}, {:?}, target {}): {}",
                o.objective.label,
                o.priority,
                o.status,
                opt(o.target_date),
                health(&o.health)
            );
            for p in &o.projects {
                let _ = writeln!(
                    out,
                    "  {} (weight {}): {} | finish {} target {} | {} open, {} overdue, {} blocked, {} unestimated",
                    p.project.label,
                    opt(p.weight),
                    health(&p.health),
                    opt(p.projected_finish),
                    opt(p.target_date),
                    p.open_tasks,
                    p.overdue_tasks,
                    p.blocked_tasks,
                    p.unestimated_tasks
                );
            }
        }
        let _ = writeln!(out, "\nRISKS ({} more)", ov.more_risks);
        for r in &ov.risks {
            let _ = writeln!(
                out,
                "  {:?} {} | {} | score {:.0} priority {}{}",
                r.kind,
                r.node.label,
                health(&r.health),
                r.risk_score,
                r.priority,
                r.via_objective
                    .as_ref()
                    .map_or(String::new(), |o| format!(" via {o}"))
            );
        }
        let _ = writeln!(out, "\nOVERLOADED");
        for p in &ov.overloaded {
            let _ = writeln!(
                out,
                "  {} | {:.0}% in week of {} | {} open tasks{}",
                p.person.label,
                p.load_pct,
                opt(p.peak_week),
                p.open_tasks,
                if p.over_task_limit {
                    " (over the task limit)"
                } else {
                    ""
                }
            );
        }
        let _ = writeln!(out, "\nSTALE WAITING-ONS");
        for w in &ov.stale_waiting {
            let _ = writeln!(
                out,
                "  {} | {} | {} days{}",
                w.waiting.description,
                w.person.label,
                w.age_days,
                if w.overdue { " overdue" } else { "" }
            );
        }
        let _ = writeln!(out, "\nWARNINGS: {}", ov.warnings.len());
        insta::assert_snapshot!(out);
        // The story the data tells.
        assert_eq!(ov.counts.red + ov.counts.amber, 1, "one project is at risk");
        let eu = ov
            .objectives
            .iter()
            .flat_map(|o| o.projects.iter())
            .find(|p| p.project.label == "EU Region")
            .unwrap();
        assert!(matches!(
            eu.health.level,
            HealthLevel::Amber | HealthLevel::Red
        ));
        assert_eq!(ov.overloaded.len(), 1, "one person is overloaded");
        assert_eq!(ov.overloaded[0].person.label, "Tomás Alvarez");
        assert_eq!(ov.stale_waiting.len(), 1, "one waiting-on is stale");
    }

    #[test]
    fn the_weekly_status_report_of_the_demo_data_reads_without_edits() {
        let mut conn = demo();
        let review = review_at(&mut conn, TODAY, None).unwrap();
        // The week's events are all there.
        assert_eq!(
            review
                .slipped
                .iter()
                .filter(|s| s.kind == minimap_types::SlipKind::DueMoved)
                .count(),
            2
        );
        assert_eq!(review.blocked.len(), 1);
        assert!(review.blocked[0].newly_blocked);
        assert_eq!(review.done.len(), 2);
        assert_eq!(review.decisions.len(), 1);
        assert_eq!(review.waiting_resolved.len(), 1);
        let text = report_at(
            &mut conn,
            TODAY,
            ReportKind::WeeklyStatus,
            &ReportParams { week_start: None },
        )
        .unwrap();
        insta::assert_snapshot!(text);
    }
}
