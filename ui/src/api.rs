//! Typed wrappers over `window.__TAURI__.core.invoke`: one async fn per command.

use minimap_types::{
    Activity, AddAttachment, AppError, ApplyPreview, ApplyResult, AssigneeChoice, Attachment,
    BackupEntry, BackupStatus, Capacity, CheckpointInfo, ConnectOutcome, CreateDecision,
    CreateNote, CreateObjective, CreatePerson, CreateProject, CreateTask, CreateTeam,
    CreateWaitingOn, DataInfo, Decision, DecisionFilter, DecisionRow, DemoSummary, DependencyGraph,
    Edge, EdgeLink, EncryptionResult, ExportResult, FinishConnect, GraphFilter, ImpactReport,
    LinkOption, NewEdge, NodeRef, NodeSummary, NodeType, Note, NoteDetail, NoteFilter, NoteRow,
    Objective, ObjectiveDetail, ObjectiveGroup, ObjectiveGrouping, Person, PersonArchivePreview,
    PersonDetail, PersonRow, PingResponse, PortfolioOverview, Project, ProjectArchivePreview,
    ProjectDetail, ProjectFilter, ProjectGroup, ProjectLayout, QuickChoice, QuickPreview,
    QuickResult, RecoverCheckpoint, RecoverResult, ReportKind, ReportParams, RestorePreview,
    RestoreResult, Schedule, ScheduleScope, ScheduledTask, SearchFilter, SearchHit, Secret,
    SecurityStatus, SetEncryption, Settings, Slip, SyncStatus, Task, TaskDetail, TaskDisposition,
    TaskFilter, TaskRow, Team, TeamDetail, TeamRow, ThisWeek, UndoOutcome, UpdateDecision,
    UpdateNote, UpdateObjective, UpdatePerson, UpdateProject, UpdateSettings, UpdateSyncSettings,
    UpdateTask, UpdateTeam, UpdateWaitingOn, Uuid, WaitingOn, WaitingOnFilter, WaitingOnRow,
    WeeklyReview, ATTACHMENT_EXTENSIONS,
};
use serde::{de::DeserializeOwned, Serialize};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(catch, js_namespace = ["window", "__TAURI__", "core"], js_name = invoke)]
    async fn invoke_raw(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
}

fn ipc_error(message: impl std::fmt::Display) -> AppError {
    AppError {
        code: "ipc".into(),
        message: message.to_string(),
    }
}

/// Calls a command. Backend errors arrive as `{ code, message }`; anything else
/// (bridge missing, bad payload) is reported as code `ipc`.
async fn invoke<A: Serialize, R: DeserializeOwned>(cmd: &str, args: &A) -> Result<R, AppError> {
    // Maps (e.g. edge `attrs`) must become plain objects: JSON.stringify turns a JS Map into `{}`.
    let args = args
        .serialize(&serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true))
        .map_err(ipc_error)?;
    match invoke_raw(cmd, args).await {
        Ok(v) => serde_wasm_bindgen::from_value(v).map_err(ipc_error),
        Err(e) => Err(serde_wasm_bindgen::from_value::<AppError>(e.clone())
            .unwrap_or_else(|_| ipc_error(format!("{e:?}")))),
    }
}

#[derive(Serialize)]
struct NoArgs {}

#[derive(Serialize)]
struct NodeArg {
    node: NodeRef,
}

/// Tauri maps `node_id` on the Rust side to `nodeId` on the JS side.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NodeIdArg {
    node_id: Uuid,
}

// The Overview used to show this; kept for the `ping` command's own smoke test.
#[allow(dead_code)]
pub async fn ping() -> Result<PingResponse, AppError> {
    invoke("ping", &NoArgs {}).await
}

pub async fn get_node_summary(node: NodeRef) -> Result<NodeSummary, AppError> {
    invoke("get_node_summary", &NodeArg { node }).await
}

pub async fn list_edges_for(node_id: Uuid) -> Result<Vec<EdgeLink>, AppError> {
    invoke("list_edges_for", &NodeIdArg { node_id }).await
}

pub async fn list_activity_for(node_id: Uuid) -> Result<Vec<Activity>, AppError> {
    invoke("list_activity_for", &NodeIdArg { node_id }).await
}

#[derive(Serialize)]
struct IdArg {
    id: Uuid,
}

#[derive(Serialize)]
struct InputArg<T> {
    input: T,
}

#[derive(Serialize)]
struct PatchArg<T> {
    id: Uuid,
    patch: T,
}

#[derive(Serialize)]
struct NameArg {
    name: String,
}

#[derive(Serialize)]
struct NewEdgeArg {
    new: NewEdge,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EdgeIdArg {
    edge_id: Uuid,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ManagerArg {
    person_id: Uuid,
    manager_id: Option<Uuid>,
}

pub async fn get_self_person() -> Result<Option<Person>, AppError> {
    invoke("get_self_person", &NoArgs {}).await
}

pub async fn create_self_person(name: String) -> Result<Person, AppError> {
    invoke("create_self_person", &NameArg { name }).await
}

pub async fn list_people() -> Result<Vec<PersonRow>, AppError> {
    invoke("list_people", &NoArgs {}).await
}

pub async fn get_person(id: Uuid) -> Result<Person, AppError> {
    invoke("get_person", &IdArg { id }).await
}

pub async fn get_person_detail(id: Uuid) -> Result<PersonDetail, AppError> {
    invoke("get_person_detail", &IdArg { id }).await
}

pub async fn create_person(input: CreatePerson) -> Result<Person, AppError> {
    invoke("create_person", &InputArg { input }).await
}

pub async fn update_person(id: Uuid, patch: UpdatePerson) -> Result<Person, AppError> {
    invoke("update_person", &PatchArg { id, patch }).await
}

pub async fn preview_archive_person(id: Uuid) -> Result<PersonArchivePreview, AppError> {
    invoke("preview_archive_person", &IdArg { id }).await
}

pub async fn archive_person(id: Uuid) -> Result<(), AppError> {
    invoke("archive_person", &IdArg { id }).await
}

pub async fn list_teams() -> Result<Vec<TeamRow>, AppError> {
    invoke("list_teams", &NoArgs {}).await
}

pub async fn get_team(id: Uuid) -> Result<Team, AppError> {
    invoke("get_team", &IdArg { id }).await
}

pub async fn get_team_detail(id: Uuid) -> Result<TeamDetail, AppError> {
    invoke("get_team_detail", &IdArg { id }).await
}

pub async fn create_team(input: CreateTeam) -> Result<Team, AppError> {
    invoke("create_team", &InputArg { input }).await
}

pub async fn update_team(id: Uuid, patch: UpdateTeam) -> Result<Team, AppError> {
    invoke("update_team", &PatchArg { id, patch }).await
}

pub async fn archive_team(id: Uuid) -> Result<(), AppError> {
    invoke("archive_team", &IdArg { id }).await
}

pub async fn add_edge(new: NewEdge) -> Result<Edge, AppError> {
    invoke("add_edge", &NewEdgeArg { new }).await
}

pub async fn remove_edge(edge_id: Uuid) -> Result<(), AppError> {
    invoke("remove_edge", &EdgeIdArg { edge_id }).await
}

pub async fn set_manager(person_id: Uuid, manager_id: Option<Uuid>) -> Result<(), AppError> {
    invoke(
        "set_manager",
        &ManagerArg {
            person_id,
            manager_id,
        },
    )
    .await
}

#[derive(Serialize)]
struct GroupingArg {
    grouping: ObjectiveGrouping,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NodeTypeArg {
    node_type: NodeType,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EdgeAttrsArg {
    edge_id: Uuid,
    attrs: serde_json::Value,
}

pub async fn list_objectives(grouping: ObjectiveGrouping) -> Result<Vec<ObjectiveGroup>, AppError> {
    invoke("list_objectives", &GroupingArg { grouping }).await
}

pub async fn get_objective(id: Uuid) -> Result<Objective, AppError> {
    invoke("get_objective", &IdArg { id }).await
}

pub async fn get_objective_detail(id: Uuid) -> Result<ObjectiveDetail, AppError> {
    invoke("get_objective_detail", &IdArg { id }).await
}

pub async fn create_objective(input: CreateObjective) -> Result<Objective, AppError> {
    invoke("create_objective", &InputArg { input }).await
}

pub async fn update_objective(id: Uuid, patch: UpdateObjective) -> Result<Objective, AppError> {
    invoke("update_objective", &PatchArg { id, patch }).await
}

pub async fn archive_objective(id: Uuid) -> Result<(), AppError> {
    invoke("archive_objective", &IdArg { id }).await
}

pub async fn list_node_summaries(node_type: NodeType) -> Result<Vec<NodeSummary>, AppError> {
    invoke("list_node_summaries", &NodeTypeArg { node_type }).await
}

pub async fn update_edge_attrs(edge_id: Uuid, attrs: serde_json::Value) -> Result<Edge, AppError> {
    invoke("update_edge_attrs", &EdgeAttrsArg { edge_id, attrs }).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectListArg {
    filter_by: ProjectFilter,
    layout: ProjectLayout,
}

#[derive(Serialize)]
struct ArchiveProjectArg {
    id: Uuid,
    tasks: TaskDisposition,
}

pub async fn list_projects(
    filter_by: ProjectFilter,
    layout: ProjectLayout,
) -> Result<Vec<ProjectGroup>, AppError> {
    invoke("list_projects", &ProjectListArg { filter_by, layout }).await
}

pub async fn get_project(id: Uuid) -> Result<Project, AppError> {
    invoke("get_project", &IdArg { id }).await
}

pub async fn get_project_detail(id: Uuid) -> Result<ProjectDetail, AppError> {
    invoke("get_project_detail", &IdArg { id }).await
}

pub async fn create_project(input: CreateProject) -> Result<Project, AppError> {
    invoke("create_project", &InputArg { input }).await
}

pub async fn update_project(id: Uuid, patch: UpdateProject) -> Result<Project, AppError> {
    invoke("update_project", &PatchArg { id, patch }).await
}

pub async fn preview_archive_project(id: Uuid) -> Result<ProjectArchivePreview, AppError> {
    invoke("preview_archive_project", &IdArg { id }).await
}

pub async fn archive_project(id: Uuid, tasks: TaskDisposition) -> Result<(), AppError> {
    invoke("archive_project", &ArchiveProjectArg { id, tasks }).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TaskListArg {
    filter_by: TaskFilter,
}

#[derive(Serialize)]
struct EstimateArg {
    id: Uuid,
    text: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AssigneeArg {
    task_id: Uuid,
    person_id: Option<Uuid>,
}

#[derive(Serialize)]
struct TextArg {
    text: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BulkArg {
    titles: Vec<String>,
    project_id: Option<Uuid>,
    assignee: AssigneeChoice,
}

#[derive(Serialize)]
struct SettingsPatchArg {
    patch: UpdateSettings,
}

pub async fn list_tasks(filter_by: TaskFilter) -> Result<Vec<TaskRow>, AppError> {
    invoke("list_tasks", &TaskListArg { filter_by }).await
}

// Kept for parity with the `get_task` command; the UI reads `get_task_detail`.
#[allow(dead_code)]
pub async fn get_task(id: Uuid) -> Result<Task, AppError> {
    invoke("get_task", &IdArg { id }).await
}

pub async fn get_task_detail(id: Uuid) -> Result<TaskDetail, AppError> {
    invoke("get_task_detail", &IdArg { id }).await
}

pub async fn create_task(input: CreateTask) -> Result<Task, AppError> {
    invoke("create_task", &InputArg { input }).await
}

pub async fn update_task(id: Uuid, patch: UpdateTask) -> Result<Task, AppError> {
    invoke("update_task", &PatchArg { id, patch }).await
}

pub async fn set_task_estimate(id: Uuid, text: String) -> Result<Task, AppError> {
    invoke("set_task_estimate", &EstimateArg { id, text }).await
}

pub async fn set_assignee(task_id: Uuid, person_id: Option<Uuid>) -> Result<(), AppError> {
    invoke("set_assignee", &AssigneeArg { task_id, person_id }).await
}

pub async fn archive_task(id: Uuid) -> Result<(), AppError> {
    invoke("archive_task", &IdArg { id }).await
}

pub async fn parse_task_lines(text: String) -> Result<Vec<String>, AppError> {
    invoke("parse_task_lines", &TextArg { text }).await
}

pub async fn create_tasks_bulk(
    titles: Vec<String>,
    project_id: Option<Uuid>,
    assignee: AssigneeChoice,
) -> Result<Vec<Task>, AppError> {
    invoke(
        "create_tasks_bulk",
        &BulkArg {
            titles,
            project_id,
            assignee,
        },
    )
    .await
}

pub async fn get_settings() -> Result<Settings, AppError> {
    invoke("get_settings", &NoArgs {}).await
}

pub async fn update_settings(patch: UpdateSettings) -> Result<Settings, AppError> {
    invoke("update_settings", &SettingsPatchArg { patch }).await
}

/// Where the data lives (database file, folder, size, schema, encryption, log).
pub async fn get_data_info() -> Result<DataInfo, AppError> {
    invoke("get_data_info", &NoArgs {}).await
}

/// Takes back the last change this session (up to 20 steps).
pub async fn undo_last() -> Result<UndoOutcome, AppError> {
    invoke("undo_last", &NoArgs {}).await
}

/// Puts back what the last undo took back.
pub async fn redo_last() -> Result<UndoOutcome, AppError> {
    invoke("redo_last", &NoArgs {}).await
}

/// Adds the demo dataset to an empty database (debug builds only; the backend refuses in a
/// release build).
pub async fn seed_demo_data() -> Result<DemoSummary, AppError> {
    invoke("seed_demo_data", &NoArgs {}).await
}

/// Opens the app's data folder in the file manager.
pub async fn show_data_folder() -> Result<(), AppError> {
    invoke("show_data_folder", &NoArgs {}).await
}

pub async fn list_link_options(node_type: NodeType) -> Result<Vec<LinkOption>, AppError> {
    invoke("list_link_options", &NodeTypeArg { node_type }).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WaitingListArg {
    filter_by: WaitingOnFilter,
}

#[derive(Serialize)]
struct SnoozeArg {
    id: Uuid,
    days: Option<u32>,
}

pub async fn get_waiting_on(filter_by: WaitingOnFilter) -> Result<Vec<WaitingOnRow>, AppError> {
    invoke("get_waiting_on", &WaitingListArg { filter_by }).await
}

pub async fn get_waiting_on_detail(id: Uuid) -> Result<WaitingOnRow, AppError> {
    invoke("get_waiting_on_detail", &IdArg { id }).await
}

pub async fn create_waiting_on(input: CreateWaitingOn) -> Result<WaitingOn, AppError> {
    invoke("create_waiting_on", &InputArg { input }).await
}

pub async fn update_waiting_on(id: Uuid, patch: UpdateWaitingOn) -> Result<WaitingOn, AppError> {
    invoke("update_waiting_on", &PatchArg { id, patch }).await
}

pub async fn resolve_waiting_on(id: Uuid) -> Result<WaitingOn, AppError> {
    invoke("resolve_waiting_on", &IdArg { id }).await
}

pub async fn reopen_waiting_on(id: Uuid) -> Result<WaitingOn, AppError> {
    invoke("reopen_waiting_on", &IdArg { id }).await
}

/// `Some(days)` hides it for that many days; `None` ends the snooze.
pub async fn snooze_waiting_on(id: Uuid, days: Option<u32>) -> Result<WaitingOn, AppError> {
    invoke("snooze_waiting_on", &SnoozeArg { id, days }).await
}

pub async fn archive_waiting_on(id: Uuid) -> Result<(), AppError> {
    invoke("archive_waiting_on", &IdArg { id }).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NoteListArg {
    filter_by: NoteFilter,
}

#[derive(Serialize)]
struct BodyArg {
    body: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConvertArg {
    note_id: Uuid,
    line: u32,
    text: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GraphArg {
    filter_by: GraphFilter,
}

pub async fn get_dependency_graph(filter_by: GraphFilter) -> Result<DependencyGraph, AppError> {
    invoke("get_dependency_graph", &GraphArg { filter_by }).await
}

#[derive(Serialize)]
struct CapacityArg {
    from: Option<minimap_types::Date>,
    to: Option<minimap_types::Date>,
    weeks: Option<u32>,
}

pub async fn get_capacity(
    from: Option<minimap_types::Date>,
    to: Option<minimap_types::Date>,
    weeks: Option<u32>,
) -> Result<Capacity, AppError> {
    invoke("get_capacity", &CapacityArg { from, to, weeks }).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WeekArg {
    week_start: Option<minimap_types::Date>,
}

pub async fn get_this_week(week_start: Option<minimap_types::Date>) -> Result<ThisWeek, AppError> {
    invoke("get_this_week", &WeekArg { week_start }).await
}

#[derive(Serialize)]
struct RescheduleArg {
    id: Uuid,
    when: String,
}

pub async fn reschedule_task(id: Uuid, when: String) -> Result<Task, AppError> {
    invoke("reschedule_task", &RescheduleArg { id, when }).await
}

pub async fn get_weekly_review(
    week_start: Option<minimap_types::Date>,
) -> Result<WeeklyReview, AppError> {
    invoke("get_weekly_review", &WeekArg { week_start }).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReportArg {
    report_kind: ReportKind,
    params: ReportParams,
}

/// The weekly status report as Markdown, made from the template in Settings.
pub async fn render_report(week_start: Option<minimap_types::Date>) -> Result<String, AppError> {
    invoke(
        "render_report",
        &ReportArg {
            report_kind: ReportKind::WeeklyStatus,
            params: ReportParams { week_start },
        },
    )
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportArg {
    report_kind: ReportKind,
    params: ReportParams,
    path: String,
}

/// Writes the weekly status report to `path` (chosen with [`save_dialog`]).
pub async fn export_markdown(
    week_start: Option<minimap_types::Date>,
    path: String,
) -> Result<ExportResult, AppError> {
    invoke(
        "export_markdown",
        &ExportArg {
            report_kind: ReportKind::WeeklyStatus,
            params: ReportParams { week_start },
            path,
        },
    )
    .await
}

pub async fn get_backup_status() -> Result<BackupStatus, AppError> {
    invoke("get_backup_status", &NoArgs {}).await
}

#[derive(Serialize)]
struct OptionalPathArg {
    path: Option<String>,
}

/// Backs up into `folder` (the configured folder when `None`).
pub async fn backup_now(folder: Option<String>) -> Result<BackupEntry, AppError> {
    invoke("backup_now", &OptionalPathArg { path: folder }).await
}

#[derive(Serialize)]
struct RestoreArg {
    path: String,
    secret: Option<Secret>,
}

/// `secret` is a passphrase or recovery key for a backup encrypted with another key.
pub async fn preview_restore(
    path: String,
    secret: Option<Secret>,
) -> Result<RestorePreview, AppError> {
    invoke("preview_restore", &RestoreArg { path, secret }).await
}

pub async fn restore_backup(
    path: String,
    secret: Option<Secret>,
) -> Result<RestoreResult, AppError> {
    invoke("restore_backup", &RestoreArg { path, secret }).await
}

pub async fn get_security_status() -> Result<SecurityStatus, AppError> {
    invoke("get_security_status", &NoArgs {}).await
}

#[derive(Serialize)]
struct SecretArg {
    secret: Secret,
}

/// Opens a locked database with a passphrase or recovery key.
pub async fn unlock_database(secret: Secret) -> Result<SecurityStatus, AppError> {
    invoke("unlock_database", &SecretArg { secret }).await
}

#[derive(Serialize)]
struct EncryptionArg {
    request: SetEncryption,
}

/// Turns encryption on, changes how the key is kept, or turns it off.
pub async fn set_encryption(request: SetEncryption) -> Result<EncryptionResult, AppError> {
    invoke("set_encryption", &EncryptionArg { request }).await
}

/// Deletes backups that are not encrypted; returns how many.
pub async fn delete_unencrypted_backups() -> Result<u32, AppError> {
    invoke("delete_unencrypted_backups", &NoArgs {}).await
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(catch, js_namespace = ["window", "__TAURI__", "dialog"], js_name = open)]
    async fn dialog_open_raw(options: JsValue) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch, js_namespace = ["window", "__TAURI__", "dialog"], js_name = save)]
    async fn dialog_save_raw(options: JsValue) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch, js_namespace = ["navigator", "clipboard"], js_name = writeText)]
    async fn clipboard_write_raw(text: &str) -> Result<JsValue, JsValue>;
}

#[derive(Serialize)]
struct SaveFilter {
    name: &'static str,
    extensions: Vec<&'static str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SaveOptions {
    title: &'static str,
    default_path: String,
    filters: [SaveFilter; 1],
}

#[derive(Serialize)]
struct OpenOptions {
    title: &'static str,
    directory: bool,
    multiple: bool,
    filters: Vec<SaveFilter>,
}

async fn open_dialog(options: OpenOptions) -> Result<Option<String>, AppError> {
    let options = options
        .serialize(&serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true))
        .map_err(ipc_error)?;
    match dialog_open_raw(options).await {
        Ok(v) if v.is_null() || v.is_undefined() => Ok(None),
        Ok(v) => Ok(v.as_string()),
        Err(e) => Err(ipc_error(format!("{e:?}"))),
    }
}

/// The operating system's folder picker. `Ok(None)` = cancelled.
pub async fn pick_folder() -> Result<Option<String>, AppError> {
    open_dialog(OpenOptions {
        title: "Choose the backup folder",
        directory: true,
        multiple: false,
        filters: Vec::new(),
    })
    .await
}

/// The operating system's file picker for a Minimap backup (`.db`). `Ok(None)` = cancelled.
pub async fn pick_backup_file() -> Result<Option<String>, AppError> {
    open_dialog(OpenOptions {
        title: "Choose a backup to restore",
        directory: false,
        multiple: false,
        filters: vec![SaveFilter {
            name: "Minimap backup",
            extensions: vec!["db"],
        }],
    })
    .await
}

/// The operating system's save dialog for a Markdown file. `Ok(None)` = cancelled.
pub async fn save_dialog(default_name: &str) -> Result<Option<String>, AppError> {
    let options = SaveOptions {
        title: "Save the status report",
        default_path: default_name.to_owned(),
        filters: [SaveFilter {
            name: "Markdown",
            extensions: vec!["md", "markdown"],
        }],
    }
    .serialize(&serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true))
    .map_err(ipc_error)?;
    match dialog_save_raw(options).await {
        Ok(v) if v.is_null() || v.is_undefined() => Ok(None),
        Ok(v) => Ok(v.as_string()),
        Err(e) => Err(ipc_error(format!("{e:?}"))),
    }
}

/// Puts text on the clipboard.
pub async fn copy_text(text: &str) -> Result<(), AppError> {
    clipboard_write_raw(text)
        .await
        .map(|_| ())
        .map_err(|e| ipc_error(format!("{e:?}")))
}

pub async fn get_portfolio_overview() -> Result<PortfolioOverview, AppError> {
    invoke("get_portfolio_overview", &NoArgs {}).await
}

#[derive(Serialize)]
struct SlipsArg {
    slips: Vec<Slip>,
}

pub async fn run_impact_analysis(slips: Vec<Slip>) -> Result<ImpactReport, AppError> {
    invoke("run_impact_analysis", &SlipsArg { slips }).await
}

pub async fn preview_apply_slips(slips: Vec<Slip>) -> Result<ApplyPreview, AppError> {
    invoke("preview_apply_slips", &SlipsArg { slips }).await
}

pub async fn apply_slips(slips: Vec<Slip>) -> Result<ApplyResult, AppError> {
    invoke("apply_slips", &SlipsArg { slips }).await
}

#[derive(Serialize)]
struct ScopeArg {
    scope: ScheduleScope,
}

pub async fn get_schedule(scope: ScheduleScope) -> Result<Schedule, AppError> {
    invoke("get_schedule", &ScopeArg { scope }).await
}

// Kept for parity with the `get_critical_path` command; the project panel reads the critical
// tasks out of `get_schedule`.
#[allow(dead_code)]
pub async fn get_critical_path(scope: ScheduleScope) -> Result<Vec<ScheduledTask>, AppError> {
    invoke("get_critical_path", &ScopeArg { scope }).await
}

#[derive(Serialize)]
struct QuickArg {
    text: String,
    choices: Vec<QuickChoice>,
}

pub async fn parse_quick_add(
    text: String,
    choices: Vec<QuickChoice>,
) -> Result<QuickPreview, AppError> {
    invoke("parse_quick_add", &QuickArg { text, choices }).await
}

pub async fn commit_quick_add(
    text: String,
    choices: Vec<QuickChoice>,
) -> Result<QuickResult, AppError> {
    invoke("commit_quick_add", &QuickArg { text, choices }).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchArg {
    query: String,
    filter_by: SearchFilter,
}

pub async fn search(query: String, filter_by: SearchFilter) -> Result<Vec<SearchHit>, AppError> {
    invoke("search", &SearchArg { query, filter_by }).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DecisionListArg {
    filter_by: DecisionFilter,
}

pub async fn list_decisions(filter_by: DecisionFilter) -> Result<Vec<DecisionRow>, AppError> {
    invoke("list_decisions", &DecisionListArg { filter_by }).await
}

pub async fn get_decision(id: Uuid) -> Result<Decision, AppError> {
    invoke("get_decision", &IdArg { id }).await
}

pub async fn create_decision(input: CreateDecision) -> Result<Decision, AppError> {
    invoke("create_decision", &InputArg { input }).await
}

pub async fn update_decision(id: Uuid, patch: UpdateDecision) -> Result<Decision, AppError> {
    invoke("update_decision", &PatchArg { id, patch }).await
}

pub async fn archive_decision(id: Uuid) -> Result<(), AppError> {
    invoke("archive_decision", &IdArg { id }).await
}

pub async fn list_notes(filter_by: NoteFilter) -> Result<Vec<NoteRow>, AppError> {
    invoke("list_notes", &NoteListArg { filter_by }).await
}

pub async fn get_note(id: Uuid) -> Result<Note, AppError> {
    invoke("get_note", &IdArg { id }).await
}

pub async fn get_note_detail(id: Uuid) -> Result<NoteDetail, AppError> {
    invoke("get_note_detail", &IdArg { id }).await
}

pub async fn create_note(input: CreateNote) -> Result<Note, AppError> {
    invoke("create_note", &InputArg { input }).await
}

pub async fn update_note(id: Uuid, patch: UpdateNote) -> Result<Note, AppError> {
    invoke("update_note", &PatchArg { id, patch }).await
}

pub async fn archive_note(id: Uuid) -> Result<(), AppError> {
    invoke("archive_note", &IdArg { id }).await
}

/// Safe HTML for a Markdown body (mentions resolved to current names).
pub async fn render_markdown(body: String) -> Result<String, AppError> {
    invoke("render_markdown", &BodyArg { body }).await
}

/// `text` is the line as the caller saw it; a changed line is refused.
pub async fn convert_checklist_item(
    note_id: Uuid,
    line: u32,
    text: String,
) -> Result<Task, AppError> {
    invoke(
        "convert_checklist_item",
        &ConvertArg {
            note_id,
            line,
            text,
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    /// Tauri turns a command's `snake_case` parameters into `camelCase` keys on the JS side, so
    /// every argument struct with a multi-word field must say so. (A missed one fails only when
    /// the screen that uses it opens: "missing required key filterBy".)
    #[test]
    fn argument_structs_with_multi_word_fields_use_camel_case() {
        let source = include_str!("api.rs");
        // Only the code above this test module.
        let code = source.split("#[cfg(test)]").next().unwrap_or(source);
        let mut checked = 0;
        let mut offenders = Vec::new();
        for block in code.split("#[derive(Serialize)]").skip(1) {
            let header_end = block.find('{').unwrap_or(block.len());
            let (header, rest) = block.split_at(header_end);
            let body = rest.split('}').next().unwrap_or("");
            let has_multi_word_field = body.lines().any(|l| {
                let name = l
                    .trim()
                    .trim_start_matches("pub ")
                    .split(':')
                    .next()
                    .unwrap_or("");
                !name.is_empty()
                    && name.chars().all(|c| c.is_ascii_lowercase() || c == '_')
                    && name.contains('_')
            });
            checked += 1;
            if has_multi_word_field && !header.contains("camelCase") {
                offenders.push(header.trim().lines().last().unwrap_or("").to_owned());
            }
        }
        assert!(
            checked > 20,
            "the scan found only {checked} argument structs"
        );
        assert!(
            offenders.is_empty(),
            "missing rename_all = \"camelCase\": {offenders:?}"
        );
    }
}

// ------------------------------------------------------------------ Google Drive and attachments (spec 22)

#[derive(Serialize)]
struct RequestArg<T> {
    request: T,
}

/// What the status bar and Settings show: connected or local only, saved or not, devices.
pub async fn get_sync_status() -> Result<SyncStatus, AppError> {
    invoke("get_sync_status", &NoArgs {}).await
}

pub async fn update_sync_settings(request: UpdateSyncSettings) -> Result<SyncStatus, AppError> {
    invoke("update_sync_settings", &RequestArg { request }).await
}

/// Opens the browser to sign in to Google and waits for the answer (up to five minutes).
pub async fn connect_drive() -> Result<ConnectOutcome, AppError> {
    invoke("connect_drive", &NoArgs {}).await
}

pub async fn cancel_drive_connect() -> Result<(), AppError> {
    invoke("cancel_drive_connect", &NoArgs {}).await
}

/// The recovery key that opens data already on Drive.
pub async fn finish_drive_connect(recovery_key: Secret) -> Result<ConnectOutcome, AppError> {
    invoke(
        "finish_drive_connect",
        &RequestArg {
            request: FinishConnect { recovery_key },
        },
    )
    .await
}

pub async fn disconnect_drive() -> Result<SyncStatus, AppError> {
    invoke("disconnect_drive", &NoArgs {}).await
}

pub async fn sync_now() -> Result<SyncStatus, AppError> {
    invoke("sync_now", &NoArgs {}).await
}

pub async fn list_drive_checkpoints() -> Result<Vec<CheckpointInfo>, AppError> {
    invoke("list_drive_checkpoints", &NoArgs {}).await
}

pub async fn recover_checkpoint(name: String) -> Result<RecoverResult, AppError> {
    invoke(
        "recover_checkpoint",
        &RequestArg {
            request: RecoverCheckpoint { name },
        },
    )
    .await
}

pub async fn list_attachments(node: NodeRef) -> Result<Vec<Attachment>, AppError> {
    invoke("list_attachments", &NodeArg { node }).await
}

/// Attaches the file at `path` (from [`pick_attachment`]).
pub async fn add_attachment(node: NodeRef, path: String) -> Result<Attachment, AppError> {
    invoke(
        "add_attachment",
        &RequestArg {
            request: AddAttachment { node, path },
        },
    )
    .await
}

pub async fn remove_attachment(id: Uuid) -> Result<(), AppError> {
    invoke("remove_attachment", &IdArg { id }).await
}

/// Opens an attachment with the program the system uses for that kind of file.
pub async fn open_attachment(id: Uuid) -> Result<(), AppError> {
    invoke("open_attachment", &IdArg { id }).await
}

#[wasm_bindgen]
extern "C" {
    /// The same `invoke`, with a binary body and headers (for files pasted or dropped).
    #[wasm_bindgen(catch, js_namespace = ["window", "__TAURI__", "core"], js_name = invoke)]
    async fn invoke_bytes_raw(
        cmd: &str,
        body: &js_sys::Uint8Array,
        options: JsValue,
    ) -> Result<JsValue, JsValue>;

    /// The address the webview uses for a custom protocol (it differs between systems).
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], js_name = convertFileSrc)]
    fn convert_file_src(path: &str, protocol: &str) -> String;
}

/// The address of an attached picture, served by the app's `minimap-media` protocol.
pub fn attachment_url(id: Uuid) -> String {
    convert_file_src(&id.to_string(), "minimap-media")
}

/// Attaches a file that arrived as data (pasted or dropped) to `node`.
pub async fn add_attachment_file(
    node: NodeRef,
    file: &web_sys::File,
) -> Result<Attachment, AppError> {
    let buffer = wasm_bindgen_futures::JsFuture::from(file.array_buffer())
        .await
        .map_err(|e| ipc_error(format!("Couldn't read the file: {e:?}")))?;
    let bytes = js_sys::Uint8Array::new(&buffer);
    let headers = js_sys::Object::new();
    let encoded_name = js_sys::encode_uri_component(&file.name());
    for (k, v) in [
        ("x-node-type", node.node_type.as_str().to_owned()),
        ("x-node-id", node.id.to_string()),
        ("x-file-name", String::from(encoded_name)),
    ] {
        js_sys::Reflect::set(&headers, &JsValue::from_str(k), &JsValue::from_str(&v))
            .map_err(|e| ipc_error(format!("{e:?}")))?;
    }
    let options = js_sys::Object::new();
    js_sys::Reflect::set(&options, &JsValue::from_str("headers"), &headers)
        .map_err(|e| ipc_error(format!("{e:?}")))?;
    match invoke_bytes_raw("add_attachment_data", &bytes, options.into()).await {
        Ok(v) => serde_wasm_bindgen::from_value(v).map_err(ipc_error),
        Err(e) => Err(serde_wasm_bindgen::from_value::<AppError>(e.clone())
            .unwrap_or_else(|_| ipc_error(format!("{e:?}")))),
    }
}

/// The operating system's file picker for something to attach. `Ok(None)` = cancelled.
pub async fn pick_attachment() -> Result<Option<String>, AppError> {
    open_dialog(OpenOptions {
        title: "Attach a file",
        directory: false,
        multiple: false,
        filters: vec![SaveFilter {
            name: "Images, documents, spreadsheets, presentations",
            extensions: ATTACHMENT_EXTENSIONS.to_vec(),
        }],
    })
    .await
}
