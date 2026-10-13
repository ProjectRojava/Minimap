//! Demo data (spec 24): fills an empty database with a realistic dataset. Developer-only: the
//! command refuses in release builds and the Settings tab that offers it is not shown there.

use std::path::Path;

use minimap_store::{security::Key, Connection};
use minimap_types::{AppError, Date, DemoRemoval, DemoStatus, DemoSummary};
use tauri::State;

use crate::{
    commands::backup::backup_now_impl,
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

/// Adds the demo dataset (3 objectives, 3 projects, 45 tasks, 8 people, ...) to an empty
/// database, dated around today. Debug builds only.
#[tauri::command]
pub async fn seed_demo_data(state: State<'_, AppState>) -> Result<DemoSummary, AppError> {
    if let Some(e) = refusal(cfg!(debug_assertions), state.sync.engine.is_connected()) {
        return Err(e);
    }
    // Through `run_vault`: the whole seed is not a step to undo. Whatever came before it
    // refers to an empty database, so that history goes.
    let summary = state
        .run_vault(|vault, _| seed_impl(vault.parts()?.0, minimap_store::today()))
        .await?;
    state.forget_undo();
    Ok(summary)
}

/// Is there demo data in this database, and what would removing it do? Changes nothing. Works
/// in every build: the demo data may have been added by a development build and be shown by a
/// released one.
#[tauri::command]
pub async fn get_demo_status(state: State<'_, AppState>) -> Result<DemoStatus, AppError> {
    state
        .run(|conn| minimap_store::demo_remove::status(conn).map_err(store_error))
        .await
}

/// Removes the demo data and only that: a backup is saved first, the user's own items are kept
/// and detached from what goes, and the removal is all or nothing.
#[tauri::command]
pub async fn remove_demo_data(state: State<'_, AppState>) -> Result<DemoRemoval, AppError> {
    let data_dir = state.data_dir.clone();
    // Through `run_vault` (it needs the key to back up an encrypted database): the removal is
    // not a step to undo, and older steps may refer to what is gone.
    let removal = state
        .run_vault(move |vault, _| {
            let (conn, key) = vault.parts()?;
            remove_impl(conn, &data_dir, key)
        })
        .await?;
    state.forget_undo();
    Ok(removal)
}

pub(crate) fn remove_impl(
    conn: &mut Connection,
    data_dir: &Path,
    key: &Key,
) -> Result<DemoRemoval, AppError> {
    if !minimap_store::demo_remove::status(conn)
        .map_err(store_error)?
        .found
    {
        return Err(app_error("invalid", "There is no demo data to remove."));
    }
    // If the backup can't be made nothing is removed.
    let backup = backup_now_impl(conn, data_dir, None, key)?;
    let (removed, impact) = minimap_store::demo_remove::remove(conn).map_err(store_error)?;
    Ok(DemoRemoval {
        removed,
        impact,
        backup: Some(backup.path),
    })
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

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "minimap-demo-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn removing_demo_data_saves_a_backup_first_and_leaves_an_empty_database() {
        let data = temp_dir("remove");
        let mut conn = minimap_store::open(&data.join("minimap.db")).unwrap();
        seed_impl(&mut conn, minimap_store::today()).unwrap();
        let status = minimap_store::demo_remove::status(&conn).unwrap();
        assert!(status.found);

        let removal = remove_impl(&mut conn, &data, &Key::None).unwrap();
        assert_eq!(removal.removed.tasks, 45);
        // The backup exists and still holds the demo data: it is the way back.
        let backup = std::path::PathBuf::from(removal.backup.unwrap());
        assert!(backup.starts_with(data.join("backups")) && backup.exists());
        let copy = minimap_store::open(&backup).unwrap();
        assert_eq!(minimap_store::tasks::list(&copy, false).unwrap().len(), 45);
        // The live database is back to nothing but me.
        assert!(minimap_store::tasks::list(&conn, false).unwrap().is_empty());
        assert!(!minimap_store::demo_remove::status(&conn).unwrap().found);
        // And a second removal has nothing to do.
        let again = remove_impl(&mut conn, &data, &Key::None).unwrap_err();
        assert_eq!(again.code, "invalid");
        assert!(again.message.contains("no demo data"));
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
        let _ = writeln!(out, "\nROUTES");
        for route in &report.routes {
            let _ = writeln!(
                out,
                "  {} | {} | finish {} -> {} | +{:.1} | late {} | route {} -> {}",
                route.title,
                if route.changed {
                    "recalculated"
                } else {
                    "unchanged"
                },
                opt(route.old_finish),
                opt(route.new_finish),
                route.delay_days,
                opt(route.late_after),
                route.before.len(),
                route.after.len()
            );
            for (i, t) in route.turns.iter().enumerate() {
                let _ = writeln!(
                    out,
                    "    {}. {:?} {} (in {:.1}, absorbed {:.1}, delay {:.1}, also {}) finishes {}",
                    i + 1,
                    t.kind,
                    t.task.label,
                    t.incoming_days,
                    t.absorbed_days,
                    t.delay_days,
                    t.also,
                    t.new_finish
                );
            }
        }
        insta::assert_snapshot!(out);
    }

    #[test]
    fn the_overview_of_the_demo_data() {
        let conn = demo();
        let ov = overview_at(&conn, TODAY).unwrap();
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

    #[test]
    fn the_my_view_of_the_demo_data() {
        use minimap_types::{View, Why};
        let conn = demo();
        let scope = crate::commands::orientation::my_scope_at(&conn, TODAY).unwrap();
        let name_of = |node_type: NodeType, id: minimap_types::Uuid| {
            minimap_store::nodes::summary(&conn, minimap_types::NodeRef::new(node_type, id))
                .map_or_else(|_| "?".to_owned(), |s| s.label)
        };
        let words = |why: &Why| match why {
            Why::Assigned => "assigned".to_owned(),
            Why::Owner => "owner".to_owned(),
            Why::InMyProject => "in my project".to_owned(),
            Why::Reaches { via } => format!("reaches via {}", name_of(NodeType::Task, *via)),
            Why::Blocks { via } => format!("blocks via {}", name_of(NodeType::Task, *via)),
            Why::Serves => "serves".to_owned(),
            Why::Report => "report".to_owned(),
            Why::OnMyProjects => "on my projects".to_owned(),
            Why::Me => "me".to_owned(),
        };
        let mut out = String::new();
        for (heading, node_type, list) in [
            ("OBJECTIVES", NodeType::Objective, &scope.objectives),
            ("PROJECTS", NodeType::Project, &scope.projects),
            ("PEOPLE", NodeType::Person, &scope.people),
            ("TASKS", NodeType::Task, &scope.tasks),
        ] {
            let _ = writeln!(out, "{heading}");
            let mut lines: Vec<String> = list
                .iter()
                .map(|(id, why)| {
                    format!(
                        "  {} - {}",
                        name_of(node_type, *id),
                        why.iter().map(&words).collect::<Vec<_>>().join(", ")
                    )
                })
                .collect();
            lines.sort();
            for line in lines {
                let _ = writeln!(out, "{line}");
            }
        }
        insta::assert_snapshot!(out);
        // My view is not trivial: you own a project, some of the work is assigned to you, and
        // your work holds up another project's. (Nothing in the demo data reaches your work from
        // outside: every cross-project link runs out of your project. Reach has hand-built tests.)
        assert!(scope.projects.values().any(|w| w.contains(&Why::Owner)));
        assert!(scope.tasks.values().any(|w| w.contains(&Why::Assigned)));
        assert!(scope
            .tasks
            .values()
            .any(|w| w.iter().any(|x| matches!(x, Why::Blocks { .. }))));
        assert!(scope.people.values().any(|w| w.contains(&Why::Report)));

        // The overview in My view is smaller than the portfolio, and its top risks are recomputed
        // among what is mine.
        let all =
            crate::commands::overview::overview_in_view(&conn, TODAY, View::Portfolio).unwrap();
        let mine = crate::commands::overview::overview_in_view(&conn, TODAY, View::Mine).unwrap();
        let mut report = String::new();
        for (heading, o) in [("PORTFOLIO", &all), ("MINE", &mine)] {
            let _ = writeln!(report, "{heading}");
            for obj in &o.objectives {
                let projects: Vec<&str> = obj
                    .projects
                    .iter()
                    .map(|p| p.project.label.as_str())
                    .collect();
                let _ = writeln!(
                    report,
                    "  {} [{}]",
                    obj.objective.label,
                    projects.join(", ")
                );
            }
            let unlinked: Vec<&str> = o
                .unlinked_projects
                .iter()
                .map(|p| p.project.label.as_str())
                .collect();
            let _ = writeln!(report, "  unlinked [{}]", unlinked.join(", "));
            let risks: Vec<&str> = o.risks.iter().map(|r| r.node.label.as_str()).collect();
            let _ = writeln!(report, "  risks [{}] +{}", risks.join(", "), o.more_risks);
            let loaded: Vec<&str> = o
                .overloaded
                .iter()
                .map(|p| p.person.label.as_str())
                .collect();
            let _ = writeln!(report, "  overloaded [{}]", loaded.join(", "));
            let _ = writeln!(
                report,
                "  counts {}/{}/{}/{}",
                o.counts.red, o.counts.amber, o.counts.green, o.counts.idle
            );
        }
        insta::assert_snapshot!("the_my_view_overview_of_the_demo_data", report);
        assert!(mine.objectives.len() <= all.objectives.len());
    }

    #[test]
    fn the_people_frames_of_the_demo_data() {
        use minimap_types::FrameFlag;
        let conn = demo();
        let frames = crate::commands::people_frames::frames_at(&conn, TODAY)
            .unwrap()
            .unwrap();
        let mut out = String::new();
        for f in std::iter::once(&frames.me).chain(frames.reports.iter()) {
            let _ = writeln!(
                out,
                "{} ({}) load {} open {} blocked {} overdue {} waiting {} stale {} last 1:1 {} due {} flags {:?}",
                f.person.label,
                if f.is_self { "you" } else { "report" },
                opt(f.load_pct),
                f.open_tasks,
                f.blocked_tasks,
                f.overdue_tasks,
                f.waiting_on_them,
                f.waiting_stale,
                f.last_one_on_one.map_or("-".to_owned(), |d| d.to_string()),
                opt(f.one_on_one_due),
                f.flags
            );
        }
        for group in &frames.below {
            let manager = frames
                .reports
                .iter()
                .chain(frames.below.iter().flat_map(|g| g.frames.iter()))
                .find(|f| f.person.node.id == group.manager)
                .map_or("?", |f| f.person.label.as_str());
            let names: Vec<&str> = group
                .frames
                .iter()
                .map(|f| f.person.label.as_str())
                .collect();
            let _ = writeln!(
                out,
                "BELOW {manager} (depth {}): {}",
                group.depth,
                names.join(", ")
            );
        }
        insta::assert_snapshot!(out);
        // The story the data tells: the overloaded person is over capacity, someone is due a
        // 1:1, and Priya (1:1 today) is not.
        let report = |name: &str| {
            frames
                .reports
                .iter()
                .find(|f| f.person.label == name)
                .unwrap()
        };
        assert!(report("Tomás Alvarez").flags.contains(&FrameFlag::Over));
        assert!(frames.reports.iter().any(|f| f.one_on_one_due.is_some()));
        assert_eq!(report("Priya Nair").one_on_one_due, None);
        assert!(frames.hint.is_none());
    }

    #[test]
    fn the_pings_of_the_demo_data() {
        let conn = demo();
        let view = crate::commands::pings::list_pings_at(&conn, TODAY).unwrap();
        let mut out = String::new();
        for (heading, rows) in [("DUE", &view.due), ("COMING UP", &view.upcoming)] {
            let _ = writeln!(out, "{heading}");
            for r in rows {
                let _ = writeln!(
                    out,
                    "{:?} {} - {} - {} (made {})",
                    r.target.node.node_type,
                    r.target.label,
                    r.ping.note,
                    r.ping.on_date,
                    r.ping.created_at.date()
                );
            }
        }
        insta::assert_snapshot!(out);
        assert_eq!(view.due.len(), 2);
        assert_eq!(view.upcoming.len(), 1);
    }

    #[test]
    fn the_fog_of_the_demo_data() {
        use minimap_types::{FogLevel, NodeType};
        let conn = demo();
        let fog = crate::commands::fog::fog_at(&conn, TODAY).unwrap();
        let mut out = String::new();
        let _ = writeln!(
            out,
            "VISIBILITY: {} ({} in play, {} clear)",
            opt(fog.visibility),
            fog.in_play,
            fog.clear
        );
        for m in &fog.marks {
            let _ = writeln!(
                out,
                "{:?} {:?} {} - {}d, last seen {} ({:?})",
                m.level, m.node.node_type, m.label, m.age_days, m.last_seen, m.reason
            );
        }
        insta::assert_snapshot!(out);
        // The story the data tells.
        let level = |title: &str| {
            fog.marks
                .iter()
                .find(|m| m.label == title)
                .map_or(FogLevel::Clear, |m| m.level)
        };
        assert_eq!(level("Platform Cost Reduction"), FogLevel::Fog);
        assert_eq!(
            level("EU Region"),
            FogLevel::Clear,
            "the busy project stays clear"
        );
        assert_eq!(level("Choose a penetration-test vendor"), FogLevel::Haze);
        assert_eq!(level("Monthly access review"), FogLevel::Haze);
        // Old, but confirmed three days ago.
        assert_eq!(level("Rotate service credentials"), FogLevel::Clear);
        assert!(fog
            .marks
            .iter()
            .filter(|m| m.node.node_type == NodeType::Task)
            .any(|m| m.level == FogLevel::Fog));
    }

    #[test]
    fn the_safe_zones_of_the_demo_data() {
        use minimap_types::{ProjectFilter, ProjectLayout, Zone};
        let conn = demo();
        let groups = crate::commands::projects::list_at(
            &conn,
            &ProjectFilter::default(),
            ProjectLayout::Board,
            TODAY,
        )
        .unwrap();
        let mut out = String::new();
        let mut zones = std::collections::BTreeMap::new();
        for row in groups.iter().flat_map(|g| g.rows.iter()) {
            let _ = writeln!(out, "{}: {:?}", row.project.title, row.zone);
            zones.insert(row.project.title.clone(), row.zone);
        }
        // The Overview reads the same zone (the same schedule run).
        let ov = overview_at(&demo(), TODAY).unwrap();
        for row in ov
            .objectives
            .iter()
            .flat_map(|o| o.projects.iter())
            .chain(ov.unlinked_projects.iter())
        {
            assert_eq!(
                zones.get(&row.project.label).copied(),
                Some(row.zone),
                "{}",
                row.project.label
            );
        }
        insta::assert_snapshot!(out);
        // The story: EU Region (amber) is outside its target, the others are inside.
        assert!(matches!(zones["EU Region"], Zone::Outside { .. }));
        assert!(matches!(
            zones["Platform Cost Reduction"],
            Zone::Inside { .. }
        ));
    }

    #[test]
    fn what_is_approaching_in_the_demo_data() {
        use minimap_types::ApproachScope;
        let mut conn = demo();
        let mut out = String::new();
        let mut scopes = vec![("Me".to_owned(), ApproachScope::Me)];
        for title in [
            "EU Region",
            "Platform Cost Reduction",
            "Security and Compliance",
        ] {
            scopes.push((
                title.to_owned(),
                ApproachScope::Project(project_id(&conn, title)),
            ));
        }
        let mut found = std::collections::BTreeMap::new();
        for (name, scope) in scopes {
            let r = crate::commands::approach::approaching_at(&mut conn, scope, TODAY).unwrap();
            let _ = writeln!(
                out,
                "{name}: {}",
                r.headline.map_or("nothing".to_owned(), |h| format!(
                    "{} approaching, first in {} days, worst {:?}",
                    h.count, h.first_in_days, h.worst
                ))
            );
            for group in &r.groups {
                let from = group
                    .source_project
                    .as_ref()
                    .map_or("(inbox)", |p| p.label.as_str());
                for a in &group.items {
                    let path: Vec<&str> = a.path.iter().map(|n| n.label.as_str()).collect();
                    let _ = writeln!(
                        out,
                        "  from {from}: {} [{:?}] arrives in {}, slack {:?}, {:?}",
                        path.join(" -> "),
                        a.reasons,
                        a.arrives_in,
                        a.target_slack,
                        a.severity
                    );
                }
            }
            found.insert(name, r);
        }
        insta::assert_snapshot!(out);
        // The story: trouble in EU Region reaches the other projects through the cross-project
        // links, and EU Region itself hears about nothing from them.
        let from_eu = |name: &str| {
            found[name].groups.iter().any(|g| {
                g.source_project
                    .as_ref()
                    .is_some_and(|p| p.label == "EU Region")
            })
        };
        assert!(
            from_eu("Platform Cost Reduction") || from_eu("Security and Compliance"),
            "something approaches from EU Region"
        );
    }

    #[test]
    fn the_dependency_graph_of_the_demo_data_says_who_what_and_how_much_room() {
        use minimap_types::{GraphFilter, GraphLevel};
        let conn = demo();
        // Positions depend on the layout, so the snapshot keeps to what the boxes say.
        let mut out = String::new();
        for level in [GraphLevel::Tasks, GraphLevel::Projects] {
            let g = crate::commands::graph::graph_at(
                &conn,
                &GraphFilter {
                    level,
                    ..Default::default()
                },
                TODAY,
            )
            .unwrap();
            let _ = writeln!(
                out,
                "{level:?}: {} boxes, {} links ({} cross projects), {} unlinked ({} listed)",
                g.nodes.len(),
                g.edges.len(),
                g.edges.iter().filter(|e| e.cross_project).count(),
                g.hidden_unlinked,
                g.unlinked.len()
            );
            let mut nodes: Vec<_> = g.nodes.iter().collect();
            nodes.sort_by(|a, b| {
                b.downstream
                    .cmp(&a.downstream)
                    .then_with(|| a.label.cmp(&b.label))
            });
            for n in nodes.iter().take(6) {
                let _ = writeln!(
                    out,
                    "  {} | owner {} | holds up {} waits on {} | finish {} due {} | {:?} health {:?}",
                    n.label,
                    n.assignee.as_ref().map_or("-", |a| a.label.as_str()),
                    n.downstream,
                    n.upstream,
                    opt(n.finish),
                    opt(n.due),
                    n.zone,
                    n.health
                );
            }
            for e in g.edges.iter().filter(|e| e.room_days.is_some()) {
                let _ = writeln!(out, "  room between projects: {:?}", e.room_days);
            }
        }
        insta::assert_snapshot!(out);
    }
}
