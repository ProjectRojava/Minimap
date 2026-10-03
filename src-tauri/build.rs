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
    "list_projects",
    "get_project",
    "get_project_detail",
    "create_project",
    "update_project",
    "preview_archive_project",
    "archive_project",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to run tauri-build");
}
