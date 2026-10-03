//! Window controls for the custom title bar: thin wrappers over Tauri's window API
//! (`window.__TAURI__.window`). Permissions are listed in `capabilities/default.json`.

use minimap_types::AppError;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    type TauriWindow;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "window"], js_name = getCurrentWindow)]
    fn current_window() -> TauriWindow;

    #[wasm_bindgen(method, catch, js_name = minimize)]
    async fn minimize_raw(this: &TauriWindow) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(method, catch, js_name = toggleMaximize)]
    async fn toggle_maximize_raw(this: &TauriWindow) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(method, catch, js_name = close)]
    async fn close_raw(this: &TauriWindow) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(method, catch, js_name = isMaximized)]
    async fn is_maximized_raw(this: &TauriWindow) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(method, catch, js_name = startResizeDragging)]
    async fn start_resize_raw(this: &TauriWindow, direction: &str) -> Result<JsValue, JsValue>;
}

fn window_error(what: &str, e: JsValue) -> AppError {
    AppError {
        code: "window".into(),
        message: format!(
            "Couldn't {what}: {}",
            e.as_string().unwrap_or_else(|| format!("{e:?}"))
        ),
    }
}

pub async fn minimize() -> Result<(), AppError> {
    current_window()
        .minimize_raw()
        .await
        .map(drop)
        .map_err(|e| window_error("minimize", e))
}

pub async fn toggle_maximize() -> Result<(), AppError> {
    current_window()
        .toggle_maximize_raw()
        .await
        .map(drop)
        .map_err(|e| window_error("maximize", e))
}

pub async fn close() -> Result<(), AppError> {
    current_window()
        .close_raw()
        .await
        .map(drop)
        .map_err(|e| window_error("close", e))
}

pub async fn is_maximized() -> Result<bool, AppError> {
    current_window()
        .is_maximized_raw()
        .await
        .map(|v| v.as_bool().unwrap_or(false))
        .map_err(|e| window_error("read window state", e))
}

pub async fn start_resize(dir: ResizeDir) -> Result<(), AppError> {
    current_window()
        .start_resize_raw(dir.as_str())
        .await
        .map(drop)
        .map_err(|e| window_error("resize", e))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Windows,
    Mac,
    Linux,
    Other,
}

impl Platform {
    pub fn from_user_agent(ua: &str) -> Self {
        if ua.contains("Windows") {
            Platform::Windows
        } else if ua.contains("Macintosh") || ua.contains("Mac OS X") {
            Platform::Mac
        } else if ua.contains("Linux") || ua.contains("X11") {
            Platform::Linux
        } else {
            Platform::Other
        }
    }

    pub fn detect() -> Self {
        let user_agent = || {
            let navigator = js_sys::Reflect::get(&js_sys::global(), &"navigator".into()).ok()?;
            js_sys::Reflect::get(&navigator, &"userAgent".into())
                .ok()?
                .as_string()
        };
        user_agent()
            .map(|ua| Self::from_user_agent(&ua))
            .unwrap_or(Platform::Other)
    }

    /// macOS keeps its native traffic lights (overlay title bar) and resize behaviour.
    pub fn draws_window_controls(self) -> bool {
        self != Platform::Mac
    }

    /// Windows resizes undecorated windows natively; on Linux we draw the resize borders.
    pub fn needs_resize_handles(self) -> bool {
        matches!(self, Platform::Linux | Platform::Other)
    }
}

/// Edge or corner being dragged (Tauri's `ResizeDirection`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeDir {
    North,
    South,
    East,
    West,
    NorthEast,
    NorthWest,
    SouthEast,
    SouthWest,
}

impl ResizeDir {
    pub const ALL: [ResizeDir; 8] = [
        ResizeDir::North,
        ResizeDir::South,
        ResizeDir::East,
        ResizeDir::West,
        ResizeDir::NorthEast,
        ResizeDir::NorthWest,
        ResizeDir::SouthEast,
        ResizeDir::SouthWest,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ResizeDir::North => "North",
            ResizeDir::South => "South",
            ResizeDir::East => "East",
            ResizeDir::West => "West",
            ResizeDir::NorthEast => "NorthEast",
            ResizeDir::NorthWest => "NorthWest",
            ResizeDir::SouthEast => "SouthEast",
            ResizeDir::SouthWest => "SouthWest",
        }
    }

    /// Position and cursor of the invisible grab area for this edge.
    pub fn class(self) -> &'static str {
        match self {
            ResizeDir::North => "top-0 left-2 right-2 h-1 cursor-ns-resize",
            ResizeDir::South => "bottom-0 left-2 right-2 h-1 cursor-ns-resize",
            ResizeDir::East => "right-0 top-2 bottom-2 w-1 cursor-ew-resize",
            ResizeDir::West => "left-0 top-2 bottom-2 w-1 cursor-ew-resize",
            ResizeDir::NorthEast => "top-0 right-0 h-2 w-2 cursor-nesw-resize",
            ResizeDir::SouthWest => "bottom-0 left-0 h-2 w-2 cursor-nesw-resize",
            ResizeDir::NorthWest => "top-0 left-0 h-2 w-2 cursor-nwse-resize",
            ResizeDir::SouthEast => "bottom-0 right-0 h-2 w-2 cursor-nwse-resize",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_platform_from_webview_user_agents() {
        let gtk = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko)";
        let edge = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Edg/120.0";
        let mac = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15";
        assert_eq!(Platform::from_user_agent(gtk), Platform::Linux);
        assert_eq!(Platform::from_user_agent(edge), Platform::Windows);
        assert_eq!(Platform::from_user_agent(mac), Platform::Mac);
        assert_eq!(Platform::from_user_agent(""), Platform::Other);
    }

    #[test]
    fn platform_behaviour() {
        assert!(!Platform::Mac.draws_window_controls());
        assert!(
            Platform::Windows.draws_window_controls() && Platform::Linux.draws_window_controls()
        );
        assert!(Platform::Linux.needs_resize_handles());
        assert!(!Platform::Windows.needs_resize_handles());
        assert!(!Platform::Mac.needs_resize_handles());
    }

    #[test]
    fn resize_directions_match_tauri_names() {
        let names: Vec<_> = ResizeDir::ALL.iter().map(|d| d.as_str()).collect();
        assert_eq!(
            names,
            [
                "North",
                "South",
                "East",
                "West",
                "NorthEast",
                "NorthWest",
                "SouthEast",
                "SouthWest"
            ]
        );
        for d in ResizeDir::ALL {
            assert!(d.class().contains("cursor-"));
        }
    }
}
