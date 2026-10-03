//! Typed wrappers over `window.__TAURI__.core.invoke`: one async fn per command.

use minimap_types::{
    Activity, AppError, AssigneeChoice, CreateObjective, CreatePerson, CreateProject, CreateTask,
    CreateTeam, Edge, EdgeLink, NewEdge, NodeRef, NodeSummary, NodeType, Objective,
    ObjectiveDetail, ObjectiveGroup, ObjectiveGrouping, Person, PersonArchivePreview, PersonDetail,
    PersonRow, PingResponse, Project, ProjectArchivePreview, ProjectDetail, ProjectFilter,
    ProjectGroup, ProjectLayout, Settings, Task, TaskDetail, TaskDisposition, TaskFilter, TaskRow,
    Team, TeamDetail, TeamRow, UpdateObjective, UpdatePerson, UpdateProject, UpdateSettings,
    UpdateTask, UpdateTeam, Uuid,
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
