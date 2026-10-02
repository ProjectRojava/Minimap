//! Typed wrappers over `window.__TAURI__.core.invoke`: one async fn per command.

use minimap_types::{Activity, AppError, EdgeLink, NodeRef, NodeSummary, PingResponse, Uuid};
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
    let args = serde_wasm_bindgen::to_value(args).map_err(ipc_error)?;
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
