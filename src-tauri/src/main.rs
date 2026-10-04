// Prevents an extra console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod error;
mod state;

use std::{fs, sync::Mutex};

use anyhow::Context;
use tauri::Manager;

use crate::state::AppState;

fn init_logging(dir: &std::path::Path) -> anyhow::Result<()> {
    let file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("minimap.log"))
        .context("open log file")?;
    tracing_subscriber::fmt()
        .with_writer(Mutex::new(file))
        .with_ansi(false)
        .init();
    Ok(())
}

fn main() {
    let result = tauri::Builder::default()
        .setup(|app| {
            let dir = app.path().app_data_dir().context("resolve app data dir")?;
            fs::create_dir_all(&dir).context("create app data dir")?;
            init_logging(&dir)?;
            let db_path = dir.join("minimap.db");
            let conn = minimap_store::open(&db_path)
                .map_err(|e| anyhow::anyhow!("open database {}: {e}", db_path.display()))?;
            tracing::info!(path = %db_path.display(), "database opened");
            app.manage(AppState::new(conn));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::system::ping,
            commands::nodes::get_node_summary,
            commands::nodes::list_edges_for,
            commands::nodes::list_activity_for,
            commands::nodes::list_node_summaries,
            commands::objectives::list_objectives,
            commands::objectives::get_objective,
            commands::objectives::get_objective_detail,
            commands::objectives::create_objective,
            commands::objectives::update_objective,
            commands::objectives::archive_objective,
            commands::people::get_self_person,
            commands::people::create_self_person,
            commands::people::list_people,
            commands::people::get_person,
            commands::people::get_person_detail,
            commands::people::create_person,
            commands::people::update_person,
            commands::people::preview_archive_person,
            commands::people::archive_person,
            commands::teams::list_teams,
            commands::teams::get_team,
            commands::teams::get_team_detail,
            commands::teams::create_team,
            commands::teams::update_team,
            commands::teams::archive_team,
            commands::edges::add_edge,
            commands::edges::remove_edge,
            commands::edges::set_manager,
            commands::edges::update_edge_attrs,
            commands::edges::list_link_options,
            commands::projects::list_projects,
            commands::projects::get_project,
            commands::projects::get_project_detail,
            commands::projects::create_project,
            commands::projects::update_project,
            commands::projects::preview_archive_project,
            commands::projects::archive_project,
            commands::tasks::list_tasks,
            commands::tasks::get_task,
            commands::tasks::get_task_detail,
            commands::tasks::create_task,
            commands::tasks::update_task,
            commands::tasks::set_task_estimate,
            commands::tasks::set_assignee,
            commands::tasks::archive_task,
            commands::tasks::parse_task_lines,
            commands::tasks::create_tasks_bulk,
            commands::settings::get_settings,
            commands::settings::update_settings,
            commands::waiting_on::get_waiting_on,
            commands::waiting_on::get_waiting_on_detail,
            commands::waiting_on::create_waiting_on,
            commands::waiting_on::update_waiting_on,
            commands::waiting_on::resolve_waiting_on,
            commands::waiting_on::reopen_waiting_on,
            commands::waiting_on::snooze_waiting_on,
            commands::waiting_on::archive_waiting_on,
            commands::quick_add::parse_quick_add,
            commands::quick_add::commit_quick_add,
            commands::this_week::get_this_week,
            commands::this_week::reschedule_task,
            commands::overview::get_portfolio_overview,
            commands::impact::run_impact_analysis,
            commands::impact::preview_apply_slips,
            commands::impact::apply_slips,
            commands::schedule::get_schedule,
            commands::schedule::get_critical_path,
            commands::search::search,
            commands::decisions::list_decisions,
            commands::decisions::get_decision,
            commands::decisions::create_decision,
            commands::decisions::update_decision,
            commands::decisions::archive_decision,
            commands::notes::list_notes,
            commands::notes::get_note,
            commands::notes::get_note_detail,
            commands::notes::create_note,
            commands::notes::update_note,
            commands::notes::archive_note,
            commands::notes::render_markdown,
            commands::notes::convert_checklist_item,
        ])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        eprintln!("error while running Minimap: {e}");
        std::process::exit(1);
    }
}
