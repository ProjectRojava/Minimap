/// Commands exposed to the frontend. Keep in sync with `invoke_handler` in main.rs
/// and the permissions in capabilities/default.json.
const COMMANDS: &[&str] = &[
    "ping",
    "get_node_summary",
    "list_edges_for",
    "list_activity_for",
    "list_node_summaries",
    "list_objectives",
    "get_objective",
    "get_objective_detail",
    "create_objective",
    "update_objective",
    "archive_objective",
    "get_self_person",
    "create_self_person",
    "list_people",
    "get_person",
    "get_person_detail",
    "create_person",
    "update_person",
    "preview_archive_person",
    "archive_person",
    "list_teams",
    "get_team",
    "get_team_detail",
    "create_team",
    "update_team",
    "archive_team",
    "add_edge",
    "remove_edge",
    "set_manager",
    "update_edge_attrs",
    "list_link_options",
    "list_projects",
    "get_project",
    "get_project_detail",
    "create_project",
    "update_project",
    "preview_archive_project",
    "archive_project",
    "list_tasks",
    "get_task",
    "get_task_detail",
    "create_task",
    "update_task",
    "set_task_estimate",
    "set_assignee",
    "archive_task",
    "parse_task_lines",
    "create_tasks_bulk",
    "get_settings",
    "update_settings",
    "get_data_info",
    "show_data_folder",
    "get_waiting_on",
    "get_waiting_on_detail",
    "create_waiting_on",
    "update_waiting_on",
    "resolve_waiting_on",
    "reopen_waiting_on",
    "snooze_waiting_on",
    "archive_waiting_on",
    "parse_quick_add",
    "commit_quick_add",
    "get_dependency_graph",
    "get_capacity",
    "get_security_status",
    "unlock_database",
    "set_encryption",
    "delete_unencrypted_backups",
    "get_backup_status",
    "backup_now",
    "preview_restore",
    "restore_backup",
    "get_this_week",
    "reschedule_task",
    "get_portfolio_overview",
    "get_weekly_review",
    "render_report",
    "export_markdown",
    "run_impact_analysis",
    "preview_apply_slips",
    "apply_slips",
    "get_schedule",
    "get_critical_path",
    "search",
    "list_decisions",
    "get_decision",
    "create_decision",
    "update_decision",
    "archive_decision",
    "list_notes",
    "get_note",
    "get_note_detail",
    "create_note",
    "update_note",
    "archive_note",
    "render_markdown",
    "convert_checklist_item",
    "get_sync_status",
    "update_sync_settings",
    "connect_drive",
    "cancel_drive_connect",
    "finish_drive_connect",
    "disconnect_drive",
    "sync_now",
    "list_drive_checkpoints",
    "recover_checkpoint",
    "list_attachments",
    "add_attachment",
    "add_attachment_data",
    "remove_attachment",
    "open_attachment",
];

/// Bakes the Google OAuth client into the app so users just click "Sign in with Google": the
/// values come from the environment or from a git-ignored `.env` at the repository root (see
/// `.env.example`). They are read with `option_env!` in `src/sync.rs`.
fn google_client_from_env_file() {
    println!("cargo:rerun-if-changed=../.env");
    println!("cargo:rerun-if-env-changed=MINIMAP_GOOGLE_CLIENT_ID");
    println!("cargo:rerun-if-env-changed=MINIMAP_GOOGLE_CLIENT_SECRET");
    let Ok(text) = std::fs::read_to_string("../.env") else {
        return;
    };
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (
            key.trim(),
            value.trim().trim_matches(|c| c == '"' || c == '\''),
        );
        let wanted = matches!(
            key,
            "MINIMAP_GOOGLE_CLIENT_ID" | "MINIMAP_GOOGLE_CLIENT_SECRET"
        );
        // A variable already set in the environment wins over the file.
        if wanted && !value.is_empty() && std::env::var_os(key).is_none() {
            println!("cargo:rustc-env={key}={value}");
        }
    }
}

fn main() {
    google_client_from_env_file();
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to run tauri-build");
}
