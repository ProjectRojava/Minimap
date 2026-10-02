//! Typed wrappers over `window.__TAURI__.core.invoke`: one async fn per command.

use minimap_types::{AppError, PingResponse};
use serde::{de::DeserializeOwned, Serialize};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(catch, js_namespace = ["window", "__TAURI__", "core"], js_name = invoke)]
    async fn invoke_raw(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
}

/// Calls a command; errors are rendered as a displayable string.
async fn invoke<A: Serialize, R: DeserializeOwned>(cmd: &str, args: &A) -> Result<R, String> {
    let args = serde_wasm_bindgen::to_value(args).map_err(|e| e.to_string())?;
    match invoke_raw(cmd, args).await {
        Ok(v) => serde_wasm_bindgen::from_value(v).map_err(|e| e.to_string()),
        Err(e) => Err(
            match serde_wasm_bindgen::from_value::<AppError>(e.clone()) {
                Ok(app) => app.to_string(),
                Err(_) => format!("{e:?}"),
            },
        ),
    }
}

#[derive(Serialize)]
struct NoArgs {}

pub async fn ping() -> Result<PingResponse, String> {
    invoke("ping", &NoArgs {}).await
}
