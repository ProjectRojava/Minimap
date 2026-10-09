use std::path::Path;

use minimap_core::{
    report,
    weekly_review::{build, week_of, ReviewInput},
};
use minimap_store::Connection;
use minimap_types::{AppError, Date, ExportResult, ReportKind, ReportParams, WeeklyReview};
use tauri::State;

use crate::{
    commands::overview::overview_at,
    error::{app_error, store_error},
    state::AppState,
};

/// What happened this week and where things stand: slipped, blocked, overloaded, stale
/// waiting-ons, decisions made, what got done, plus health and risks. The week is Monday to
/// Sunday and `week_start` can be any date in it (today's week when omitted). Read-only.
#[tauri::command]
pub async fn get_weekly_review(
    state: State<'_, AppState>,
    week_start: Option<Date>,
) -> Result<WeeklyReview, AppError> {
    state.run(move |conn| review_impl(conn, week_start)).await
}

pub(crate) fn review_impl(
    conn: &mut Connection,
    week_start: Option<Date>,
) -> Result<WeeklyReview, AppError> {
    review_at(conn, minimap_store::today(), week_start)
}

/// [`review_impl`] as of `today` (the demo-data snapshot tests fix it).
pub(crate) fn review_at(
    conn: &mut Connection,
    today: Date,
    week_start: Option<Date>,
) -> Result<WeeklyReview, AppError> {
    let (from, to) = week_of(week_start.unwrap_or(today));
    let overview = overview_at(conn, today)?;
    let conn: &Connection = conn;
    let settings = minimap_store::settings::get(conn).map_err(store_error)?;
    Ok(build(ReviewInput {
        // Meetings (spec 38) start and end on their own: they are not work that slipped, got
        // blocked or was done, so the review leaves them out.
        tasks: minimap_store::views::task_rows(conn)
            .map_err(store_error)?
            .into_iter()
            .filter(|r| !r.task.is_meeting())
            .collect(),
        blockers: minimap_store::views::open_blockers(conn).map_err(store_error)?,
        activity: minimap_store::activity::list_between(conn, from, to).map_err(store_error)?,
        decisions: minimap_store::views::decision_items(conn).map_err(store_error)?,
        waiting: minimap_store::views::waiting_on_items(conn).map_err(store_error)?,
        projects: minimap_store::projects::list(conn, false).map_err(store_error)?,
        objectives: minimap_store::objectives::list(conn, false).map_err(store_error)?,
        overview,
        today,
        week_of: week_start,
        stale_days: settings.stale_waiting_days,
        work_week: settings.work_week,
    }))
}

/// The report as Markdown text (the review's preview and "Copy to clipboard"), made from the
/// template in Settings.
#[tauri::command]
pub async fn render_report(
    state: State<'_, AppState>,
    report_kind: ReportKind,
    params: ReportParams,
) -> Result<String, AppError> {
    state
        .run(move |conn| report_impl(conn, report_kind, &params))
        .await
}

pub(crate) fn report_impl(
    conn: &mut Connection,
    kind: ReportKind,
    params: &ReportParams,
) -> Result<String, AppError> {
    report_at(conn, minimap_store::today(), kind, params)
}

/// [`report_impl`] as of `today` (the demo-data snapshot tests fix it).
pub(crate) fn report_at(
    conn: &mut Connection,
    today: Date,
    kind: ReportKind,
    params: &ReportParams,
) -> Result<String, AppError> {
    match kind {
        ReportKind::WeeklyStatus => {
            let review = review_at(conn, today, params.week_start)?;
            let settings = minimap_store::settings::get(conn).map_err(store_error)?;
            Ok(report::render(&review, &settings.report_template))
        }
    }
}

/// Writes the report to `path`, which the user chose in the save dialog, and nowhere else.
#[tauri::command]
pub async fn export_markdown(
    state: State<'_, AppState>,
    report_kind: ReportKind,
    params: ReportParams,
    path: String,
) -> Result<ExportResult, AppError> {
    state
        .run(move |conn| export_impl(conn, report_kind, &params, &path))
        .await
}

/// A Markdown file the user can name: an absolute path ending in `.md` or `.markdown`, in a
/// folder that exists, that is not itself a folder.
fn checked_path(path: &str) -> Result<&Path, AppError> {
    let invalid = |m: &str| app_error("invalid", m);
    let path = Path::new(path.trim());
    if path.as_os_str().is_empty() {
        return Err(invalid("Choose a file to save the report to"));
    }
    if !path.is_absolute() {
        return Err(invalid("The report path must be a full path"));
    }
    let markdown = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"));
    if !markdown {
        return Err(invalid("The report file must end in .md"));
    }
    if path.is_dir() {
        return Err(invalid("That path is a folder, not a file"));
    }
    if !path.parent().is_some_and(Path::is_dir) {
        return Err(invalid("The folder to save the report in does not exist"));
    }
    Ok(path)
}

pub(crate) fn export_impl(
    conn: &mut Connection,
    kind: ReportKind,
    params: &ReportParams,
    path: &str,
) -> Result<ExportResult, AppError> {
    // Check the path and make the whole report before touching the disk.
    let path = checked_path(path)?;
    let text = report_impl(conn, kind, params)?;
    std::fs::write(path, text.as_bytes())
        .map_err(|e| app_error("io", format!("Couldn't save {}: {e}", path.display())))?;
    tracing::info!(bytes = text.len(), "weekly report exported");
    Ok(ExportResult {
        path: path.display().to_string(),
        bytes: text.len() as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{
        AssigneeChoice, CreateDecision, CreatePerson, CreateTask, CreateWaitingOn, DecisionStatus,
        Patch, SlipKind, TaskStatus, UpdateDecision, UpdateSettings, UpdateTask,
    };
    use time::Duration;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "minimap-test-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn task(conn: &mut Connection, title: &str, due: Option<Date>) -> minimap_types::Task {
        minimap_store::tasks::create(
            conn,
            CreateTask {
                links: Vec::new(),
                task_type: None,
                focus: None,
                start_minute: None,
                length_minutes: None,
                title: title.into(),
                assignee: AssigneeChoice::Nobody,
                description: String::new(),
                project_id: None,
                status: None,
                estimate_days: None,
                start_date: None,
                due_date: due,
                priority: None,
                recurrence: None,
            },
        )
        .unwrap()
    }

    #[test]
    fn the_review_reads_the_activity_and_the_plan_of_the_week() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let today = minimap_store::today();
        let monday = week_of(today).0;
        // Done now (completed today), moved later now, blocked now: all in this week's log.
        let finished = task(&mut conn, "Finished", None);
        minimap_store::tasks::update(
            &mut conn,
            finished.id,
            UpdateTask {
                status: Some(TaskStatus::Done),
                ..Default::default()
            },
        )
        .unwrap();
        let moved = task(&mut conn, "Moved", Some(monday + Duration::days(11)));
        minimap_store::tasks::update(
            &mut conn,
            moved.id,
            UpdateTask {
                due_date: Patch::Set(monday + Duration::days(14)),
                ..Default::default()
            },
        )
        .unwrap();
        let stuck = task(&mut conn, "Stuck", None);
        minimap_store::tasks::update(
            &mut conn,
            stuck.id,
            UpdateTask {
                status: Some(TaskStatus::Blocked),
                ..Default::default()
            },
        )
        .unwrap();
        let review = review_impl(&mut conn, None).unwrap();
        assert!(review.is_current_week);
        let done: Vec<&str> = review.done.iter().map(|d| d.node.label.as_str()).collect();
        assert_eq!(done, vec!["Finished"]);
        assert_eq!(review.slipped.len(), 1);
        assert_eq!(review.slipped[0].kind, SlipKind::DueMoved);
        assert_eq!(review.slipped[0].node.label, "Moved");
        assert_eq!(review.blocked.len(), 1);
        assert!(review.blocked[0].newly_blocked);
        // Another week sees none of it.
        let last = review_impl(&mut conn, Some(monday - Duration::days(7))).unwrap();
        assert!(last.done.is_empty() && last.slipped.is_empty());
        assert!(!last.is_current_week);
        assert_eq!(last.blocked.len(), 1, "blocked is how things stand now");
        assert!(!last.blocked[0].newly_blocked);
    }

    #[test]
    fn decisions_and_waiting_ons_of_the_week_are_included() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let today = minimap_store::today();
        let raj = minimap_store::people::create(
            &mut conn,
            CreatePerson {
                name: "Raj".into(),
                role_title: String::new(),
                email: None,
                weekly_capacity_hours: None,
                is_self: false,
                notes: String::new(),
            },
        )
        .unwrap();
        let old = minimap_store::waiting_on::create(
            &mut conn,
            CreateWaitingOn {
                description: "Sign-off".into(),
                person_id: raj.id,
                asked_on: Some(today - Duration::days(30)),
                expected_by: None,
                follow_up_on: None,
            },
        )
        .unwrap();
        let fresh = minimap_store::waiting_on::create(
            &mut conn,
            CreateWaitingOn {
                description: "Quote".into(),
                person_id: raj.id,
                asked_on: Some(today),
                expected_by: None,
                follow_up_on: None,
            },
        )
        .unwrap();
        minimap_store::waiting_on::update(
            &mut conn,
            fresh.id,
            minimap_types::UpdateWaitingOn {
                resolved_on: Patch::Set(today),
                ..Default::default()
            },
        )
        .unwrap();
        let d = minimap_store::decisions::create(
            &mut conn,
            CreateDecision {
                title: "Postgres".into(),
                context: String::new(),
                decision: "Use it".into(),
                rationale: String::new(),
                decided_on: None,
                status: Some(DecisionStatus::Decided),
            },
        )
        .unwrap();
        minimap_store::decisions::update(
            &mut conn,
            d.id,
            UpdateDecision {
                rationale: Some("Boring is good".into()),
                ..Default::default()
            },
        )
        .unwrap();
        minimap_store::decisions::create(
            &mut conn,
            CreateDecision {
                title: "Maybe later".into(),
                context: String::new(),
                decision: String::new(),
                rationale: String::new(),
                decided_on: None,
                status: None,
            },
        )
        .unwrap();
        let review = review_impl(&mut conn, None).unwrap();
        let decisions: Vec<&str> = review.decisions.iter().map(|d| d.title.as_str()).collect();
        assert_eq!(
            decisions,
            vec!["Postgres"],
            "a mere proposal is not a decision made"
        );
        assert_eq!(review.waiting.len(), 1);
        assert_eq!(review.waiting[0].waiting.id, old.id);
        assert_eq!(review.waiting_resolved.len(), 1);
        assert_eq!(review.waiting_resolved[0].waiting.id, fresh.id);
    }

    #[test]
    fn the_report_follows_the_template_in_settings() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        task(&mut conn, "Anything", None);
        let params = ReportParams::default();
        let default = report_impl(&mut conn, ReportKind::WeeklyStatus, &params).unwrap();
        assert!(default.starts_with("# Weekly status report"), "{default}");
        assert!(default.contains("## Blocked"));
        minimap_store::settings::update(
            &mut conn,
            UpdateSettings {
                report_template: Some("Board note\n\n{{summary}}".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let custom = report_impl(&mut conn, ReportKind::WeeklyStatus, &params).unwrap();
        assert!(
            custom.starts_with("Board note\n\n- **Projects:**"),
            "{custom}"
        );
        assert!(!custom.contains("## Blocked"));
    }

    #[test]
    fn export_writes_the_report_to_the_chosen_file_and_nothing_else() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        task(&mut conn, "Anything", None);
        let dir = temp_dir("export");
        let target = dir.join("status.md");
        let r = export_impl(
            &mut conn,
            ReportKind::WeeklyStatus,
            &ReportParams::default(),
            target.to_str().unwrap(),
        )
        .unwrap();
        let written = std::fs::read_to_string(&target).unwrap();
        assert!(written.starts_with("# Weekly status report"));
        assert_eq!(r.bytes as usize, written.len());
        assert_eq!(r.path, target.display().to_string());
        let names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            vec!["status.md".to_owned()],
            "no other file is created"
        );
        // Saving again over the same file replaces it.
        export_impl(
            &mut conn,
            ReportKind::WeeklyStatus,
            &ReportParams::default(),
            target.to_str().unwrap(),
        )
        .unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn export_refuses_paths_that_are_not_a_markdown_file_and_writes_nothing() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let dir = temp_dir("refuse");
        let missing_folder = dir.join("nope").join("a.md");
        let not_markdown = dir.join("a.txt");
        let folder = dir.join("folder.md");
        std::fs::create_dir(&folder).unwrap();
        for bad in [
            "",
            "   ",
            "relative.md",
            not_markdown.to_str().unwrap(),
            missing_folder.to_str().unwrap(),
            folder.to_str().unwrap(),
        ] {
            let e = export_impl(
                &mut conn,
                ReportKind::WeeklyStatus,
                &ReportParams::default(),
                bad,
            )
            .unwrap_err();
            assert_eq!(e.code, "invalid", "{bad:?}: {e}");
        }
        let mut names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, vec!["folder.md".to_owned()]);
        // A capital extension is fine.
        let upper = dir.join("REPORT.MD");
        export_impl(
            &mut conn,
            ReportKind::WeeklyStatus,
            &ReportParams::default(),
            upper.to_str().unwrap(),
        )
        .unwrap();
        assert!(upper.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
