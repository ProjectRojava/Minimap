//! The system tray: the icon with its menu, showing and hiding the main window, what closing the
//! window does, and the badge that says something needs attention. The decisions (what a close
//! does, when to start hidden) are in `shell.rs`; this is the part that touches the windows.

use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, Window, WindowEvent,
};
use tauri_plugin_notification::NotificationExt;

use crate::{
    commands::{radar, splash},
    shell::{CloseAction, Shell},
    state::AppState,
};

pub const TRAY_ID: &str = "main";
/// Sent to the main window: open the quick-add box.
pub const QUICK_ADD_EVENT: &str = "minimap://quick-add";
/// Sent to the main window: the close button was pressed and the answer is not remembered, so
/// ask which it should be (the answer comes back through `answer_close`).
pub const CLOSE_ASKED_EVENT: &str = "minimap://close-asked";
/// Sent to the main window: the database was locked, so show the unlock screen.
pub const LOCKED_EVENT: &str = "minimap://locked";

// ------------------------------------------------------------------ the badge

const RED: [u8; 4] = [0xe5, 0x39, 0x35, 0xff];
const WHITE: [u8; 4] = [0xff, 0xff, 0xff, 0xff];

/// A red dot with a white rim, centred at (`cx`, `cy`) with outer radius `r`, painted onto an
/// RGBA picture. Anything outside the picture is left alone.
fn paint_dot(rgba: &mut [u8], width: u32, height: u32, cx: f32, cy: f32, r: f32) {
    if rgba.len() != (width as usize) * (height as usize) * 4 {
        return;
    }
    for y in 0..height {
        for x in 0..width {
            let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            let colour = if d <= r * 0.72 {
                RED
            } else if d <= r {
                WHITE
            } else {
                continue;
            };
            let at = ((y * width + x) * 4) as usize;
            rgba[at..at + 4].copy_from_slice(&colour);
        }
    }
}

/// The picture with a badge in its top-right corner.
pub fn with_badge(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut out = rgba.to_vec();
    let r = width.min(height) as f32 * 0.3;
    paint_dot(&mut out, width, height, width as f32 - r - 0.5, r + 0.5, r);
    out
}

/// A square picture of just the badge, for the task bar button's overlay (Windows).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn badge_alone(size: u32) -> Vec<u8> {
    let mut out = vec![0; (size * size * 4) as usize];
    let c = size as f32 / 2.0;
    paint_dot(&mut out, size, size, c, c, c - 0.5);
    out
}

/// "Minimap", with what is waiting after it when something is.
pub fn tooltip(overdue: u32, pings: u32) -> String {
    let mut parts = Vec::new();
    if overdue > 0 {
        parts.push(format!("{overdue} overdue"));
    }
    if pings > 0 {
        parts.push(format!(
            "{pings} {} waiting",
            if pings == 1 { "ping" } else { "pings" }
        ));
    }
    if parts.is_empty() {
        "Minimap".to_owned()
    } else {
        format!("Minimap · {}", parts.join(" · "))
    }
}

/// Shows what needs attention on the tray icon (a badge and the tooltip) and, on Windows, on the
/// task bar button.
pub fn show_attention(app: &AppHandle, overdue: u32, pings: u32) {
    let attention = overdue + pings > 0;
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(tooltip(overdue, pings)));
        if let Some(base) = app.default_window_icon() {
            let picture = if attention {
                Image::new_owned(
                    with_badge(base.rgba(), base.width(), base.height()),
                    base.width(),
                    base.height(),
                )
            } else {
                base.clone()
            };
            let _ = tray.set_icon(Some(picture));
        }
    }
    #[cfg(windows)]
    if let Some(main) = app.get_webview_window("main") {
        let overlay = attention.then(|| Image::new_owned(badge_alone(16), 16, 16));
        let _ = main.set_overlay_icon(overlay);
    }
}

// ------------------------------------------------------------------ the window

/// A desktop notification. A failure is only logged: a reminder that can't be shown is not worth
/// an error on screen.
pub fn notify(app: &AppHandle, title: &str, body: &str) {
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        tracing::warn!(error = %e, "could not show a notification");
    }
}

/// Brings the main window forward from the tray, the task bar or behind other windows.
pub fn show_main(app: &AppHandle) {
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.unminimize();
        let _ = main.show();
        let _ = main.set_focus();
    }
    if let Some(shell) = app.try_state::<Shell>() {
        shell.note_hidden(false);
        shell.set_hidden_start(false);
    }
}

/// Hides the main window; Minimap keeps running in the tray. The first time, it says so.
pub fn hide_main(app: &AppHandle) {
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.hide();
    }
    let Some(shell) = app.try_state::<Shell>() else {
        return;
    };
    shell.note_hidden(true);
    if !shell.notes().tray_hint_shown {
        shell.update_notes(|n| n.tray_hint_shown = true);
        notify(
            app,
            "Minimap is still running",
            "It's in the tray. Click the icon to open it, or choose Quit from its menu to close it.",
        );
    }
}

/// Closes Minimap for good: nothing holds the windows back, and unsaved changes go up to Drive on
/// the way out (`main.rs`).
pub fn quit(app: &AppHandle) {
    if let Some(shell) = app.try_state::<Shell>() {
        shell.begin_quit();
    }
    app.exit(0);
}

/// The main window's close button. Returns whether the event was dealt with here (the close is
/// held back); `false` lets it go on, and the radar's handler closes the radar with it.
pub fn on_window_event(window: &Window, event: &WindowEvent) -> bool {
    let WindowEvent::CloseRequested { api, .. } = event else {
        return false;
    };
    if window.label() != "main" {
        return false;
    }
    let app = window.app_handle();
    let Some(shell) = app.try_state::<Shell>() else {
        return false;
    };
    match shell.close_action() {
        CloseAction::Hide => {
            api.prevent_close();
            hide_main(app);
            true
        }
        CloseAction::Ask => {
            api.prevent_close();
            show_main(app);
            let _ = app.emit_to("main", CLOSE_ASKED_EVENT, ());
            true
        }
        CloseAction::Quit => {
            shell.begin_quit();
            false
        }
    }
}

// ------------------------------------------------------------------ the icon

/// Whether anything on this desktop shows tray icons. Windows and macOS always do. On Linux an
/// icon is only seen when a program that hosts them (the desktop's panel, or an AppIndicator
/// extension) is on the session bus; without one the icon can be made but nobody sees it, and a
/// window hidden "to the tray" would be lost. WSLg has none.
#[cfg(target_os = "linux")]
fn has_tray_host() -> bool {
    use zbus::{blocking::fdo::DBusProxy, blocking::Connection, names::BusName};
    let Ok(connection) = Connection::session() else {
        return false;
    };
    let Ok(bus) = DBusProxy::new(&connection) else {
        return false;
    };
    [
        "org.kde.StatusNotifierWatcher",
        "org.freedesktop.StatusNotifierWatcher",
    ]
    .into_iter()
    .any(|name| {
        BusName::try_from(name)
            .ok()
            .and_then(|name| bus.name_has_owner(name).ok())
            .unwrap_or(false)
    })
}

#[cfg(not(target_os = "linux"))]
fn has_tray_host() -> bool {
    true
}

/// Makes the tray icon. `false` when the system can't have one, or has nothing to show it;
/// closing then always quits.
pub fn create(app: &AppHandle) -> bool {
    if !has_tray_host() {
        tracing::warn!("nothing here shows tray icons; closing the window will quit");
        return false;
    }
    match build(app) {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!(error = %e, "no tray icon; closing the window will quit");
            false
        }
    }
}

fn build(app: &AppHandle) -> tauri::Result<()> {
    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| tauri::Error::AssetNotFound("the window icon".into()))?;
    let open = MenuItem::with_id(app, "open", "Open Minimap", true, None::<&str>)?;
    let quick = MenuItem::with_id(app, "quick-add", "Quick add…", true, None::<&str>)?;
    let radar = MenuItem::with_id(app, "radar", "Show or hide the radar", true, None::<&str>)?;
    let sync = MenuItem::with_id(app, "sync", "Sync now", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Minimap", true, None::<&str>)?;
    let line = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&open, &quick, &radar, &sync, &line, &quit])?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .tooltip(tooltip(0, 0))
        .menu(&menu)
        // A left click opens the window; the menu is the right button's.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

fn on_menu(app: &AppHandle, id: &str) {
    match id {
        "open" => show_main(app),
        "quick-add" => {
            show_main(app);
            let _ = app.emit_to("main", QUICK_ADD_EVENT, ());
        }
        "radar" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let state = app.state::<AppState>();
                let dark = splash::read_scheme(&state.data_dir) != splash::Scheme::Light;
                if let Err(e) = radar::toggle_radar(app.clone(), state, Some(dark)).await {
                    tracing::warn!(error = %e.message, "could not toggle the radar from the tray");
                }
            });
        }
        "sync" => {
            if let Some(state) = app.try_state::<AppState>() {
                let engine = state.sync.engine.clone();
                std::thread::spawn(move || {
                    // Not connected, or locked: nothing to do, and nothing to say.
                    let _ = engine.sync_now();
                });
            }
        }
        "quit" => quit(app),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(buf: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * width + x) * 4) as usize;
        [buf[at], buf[at + 1], buf[at + 2], buf[at + 3]]
    }

    #[test]
    fn the_badge_sits_in_the_top_right_and_leaves_the_rest_alone() {
        let (w, h) = (32, 32);
        let base = vec![10u8; (w * h * 4) as usize];
        let out = with_badge(&base, w, h);
        assert_eq!(out.len(), base.len());
        let r = 32.0 * 0.3;
        let cx = (w as f32 - r - 0.5) as u32;
        let cy = (r + 0.5) as u32;
        assert_eq!(pixel(&out, w, cx, cy), RED, "the middle of the dot is red");
        // The far corner and the middle of the picture are untouched.
        assert_eq!(pixel(&out, w, 0, h - 1), [10, 10, 10, 10]);
        assert_eq!(pixel(&out, w, 8, 16), [10, 10, 10, 10]);
        // The rim is white.
        let rim = pixel(&out, w, cx + (r * 0.85) as u32, cy);
        assert_eq!(rim, WHITE);
    }

    #[test]
    fn a_picture_of_the_wrong_size_is_left_as_it_was() {
        let base = vec![7u8; 10];
        assert_eq!(with_badge(&base, 32, 32), base);
    }

    #[test]
    fn the_task_bar_overlay_is_a_dot_on_nothing() {
        let out = badge_alone(16);
        assert_eq!(out.len(), 16 * 16 * 4);
        assert_eq!(pixel(&out, 16, 8, 8), RED);
        assert_eq!(pixel(&out, 16, 0, 0)[3], 0, "the corners stay see-through");
    }

    #[test]
    fn the_tooltip_says_what_is_waiting() {
        assert_eq!(tooltip(0, 0), "Minimap");
        assert_eq!(tooltip(3, 0), "Minimap · 3 overdue");
        assert_eq!(tooltip(0, 1), "Minimap · 1 ping waiting");
        assert_eq!(tooltip(2, 4), "Minimap · 2 overdue · 4 pings waiting");
    }
}
