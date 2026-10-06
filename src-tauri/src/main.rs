// Prevents an extra console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod error;
mod keystore;
mod state;
mod sync;

use std::{
    fs,
    sync::{Arc, Mutex},
};

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

/// The daily backup: once at start (when the last one is over a day old), then hourly checks.
fn spawn_auto_backup(state: AppState) -> anyhow::Result<()> {
    std::thread::Builder::new()
        .name("auto-backup".into())
        .spawn(move || loop {
            commands::backup::auto_backup_tick(&state);
            std::thread::sleep(std::time::Duration::from_secs(60 * 60));
        })
        .context("start the automatic backup timer")?;
    Ok(())
}

fn main() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        // Attached pictures are shown through this protocol, never through a network address.
        .register_asynchronous_uri_scheme_protocol("minimap-media", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            tauri::async_runtime::spawn_blocking(move || {
                let state = app.state::<AppState>();
                responder.respond(commands::attachments::serve_media(
                    &state,
                    request.uri().path(),
                ));
            });
        })
        .setup(|app| {
            let dir = app.path().app_data_dir().context("resolve app data dir")?;
            fs::create_dir_all(&dir).context("create app data dir")?;
            init_logging(&dir)?;
            commands::attachments::clean_opened(&dir);
            let keys: Arc<dyn keystore::KeyStore> = Arc::new(keystore::OsKeyStore);
            let vault = commands::security::boot(&dir, keys.as_ref())
                .map_err(|e| anyhow::anyhow!("open the database: {e}"))?;
            tracing::info!(locked = vault.conn.is_none(), "database ready");
            let state = AppState::new(vault, dir, keys);
            app.manage(state.clone());
            spawn_auto_backup(state.clone())?;
            sync::spawn(state)?;
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
            commands::settings::get_data_info,
            commands::settings::show_data_folder,
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
            commands::graph::get_dependency_graph,
            commands::capacity::get_capacity,
            commands::security::get_security_status,
            commands::security::unlock_database,
            commands::security::set_encryption,
            commands::security::delete_unencrypted_backups,
            commands::backup::get_backup_status,
            commands::backup::backup_now,
            commands::backup::preview_restore,
            commands::backup::restore_backup,
            commands::this_week::get_this_week,
            commands::this_week::reschedule_task,
            commands::overview::get_portfolio_overview,
            commands::review::get_weekly_review,
            commands::review::render_report,
            commands::review::export_markdown,
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
            commands::sync::get_sync_status,
            commands::sync::update_sync_settings,
            commands::sync::connect_drive,
            commands::sync::cancel_drive_connect,
            commands::sync::finish_drive_connect,
            commands::sync::disconnect_drive,
            commands::sync::sync_now,
            commands::sync::list_drive_checkpoints,
            commands::sync::recover_checkpoint,
            commands::attachments::list_attachments,
            commands::attachments::add_attachment,
            commands::attachments::add_attachment_data,
            commands::attachments::remove_attachment,
            commands::attachments::open_attachment,
        ])
        .build(tauri::generate_context!());
    let app = match result {
        Ok(app) => app,
        Err(e) => {
            eprintln!("error while running Minimap: {e}");
            std::process::exit(1);
        }
    };
    app.run(|handle, event| {
        // On quit, save what is unsaved to Drive (waiting at most ten seconds; anything left
        // stays marked and goes up at the next start).
        if let tauri::RunEvent::ExitRequested { .. } = event {
            let state = handle.state::<AppState>();
            let engine = state.sync.engine.clone();
            let (done, wait) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                engine.flush();
                let _ = done.send(());
            });
            let _ = wait.recv_timeout(std::time::Duration::from_secs(10));
        }
    });
}
