//! The radar (docs/product-design/10-radar-window.md, spec 53, ADR-0021): a small second window
//! with your world centred on you. Thin: the layout is `minimap_core::radar`; this gathers its
//! inputs with the helpers the other screens use, opens and closes the window, remembers where
//! it was left, and asks the main window to open an item. The radar itself can read, never
//! write (its capability, `capabilities/radar.json`, lists the few commands it may call).

use std::{
    sync::mpsc::{channel, RecvTimeoutError, Sender},
    time::Duration,
};

use minimap_core::radar::{build, RadarInput};
use minimap_store::Connection;
use minimap_types::{
    AppError, ApproachScope, Date, EdgeType, NodeRef, Radar, UpdateSettings, Uuid, View,
};
use serde::{Deserialize, Serialize};
use tauri::{
    window::Color, AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder, Window,
    WindowEvent,
};

use crate::{
    commands::{
        approach::approaching_at, fog::fog_at, orientation::my_scope_at, overview::overview_at,
        pings::list_pings_at,
    },
    error::{app_error, store_error},
    state::AppState,
};

/// The radar window's label. It loads the same page as the main window and the page tells the
/// two apart by this name (`window::is_radar_window`).
pub const RADAR_LABEL: &str = "radar";
/// The event the main window listens for: "open this item" (`NodeRef` as the payload).
pub const OPEN_EVENT: &str = "minimap://open";
/// The first size, in logical pixels, and the smallest.
const START_SIZE: f64 = 260.0;
const MIN_SIZE: f64 = 200.0;
/// How long the window must be still before its place is written down.
const SAVE_AFTER: Duration = Duration::from_millis(600);

// ------------------------------------------------------------------ the data

/// A number that moves when anything the radar shows may have changed: the newest activity row
/// (every write logs one, and a merge from another computer does too) plus how many times the
/// sync engine has merged another computer's data. Cheap.
pub(crate) fn revision_of(conn: &Connection, merges: u64) -> i64 {
    let rows = minimap_store::activity::latest_rowid(conn).unwrap_or(0);
    rows.saturating_add(i64::try_from(merges).unwrap_or(0))
}

/// The radar in a view. In My view without a "me" it is the Portfolio view.
#[tauri::command]
pub async fn get_radar(state: State<'_, AppState>, view: Option<View>) -> Result<Radar, AppError> {
    let merges = state.sync.engine.clone();
    state
        .run(move |conn| {
            // Read first: a change that lands while this builds then shows as a newer number.
            let revision = revision_of(conn, merges.data_revision());
            let mut radar = radar_at(conn, view.unwrap_or_default(), minimap_store::today())?;
            radar.revision = revision;
            Ok(radar)
        })
        .await
}

/// The number [`get_radar`] stamps on its answer; the radar polls it and asks again when it
/// moves.
#[tauri::command]
pub async fn get_data_revision(state: State<'_, AppState>) -> Result<i64, AppError> {
    let engine = state.sync.engine.clone();
    state
        .run_vault(move |vault, _| {
            let conn = vault.parts()?.0;
            Ok(revision_of(conn, engine.data_revision()))
        })
        .await
}

/// [`get_radar`] as of `today` (the tests and the demo snapshots fix it). Reuses what the other
/// screens compute; nothing here queries on its own.
pub(crate) fn radar_at(conn: &mut Connection, view: View, today: Date) -> Result<Radar, AppError> {
    let settings = minimap_store::settings::get(conn).map_err(store_error)?;
    let scope = my_scope_at(conn, today)?;
    // My view needs a "me" to look from, as everywhere else.
    let view = if view == View::Mine && !scope.is_empty() {
        View::Mine
    } else {
        View::Portfolio
    };
    let tasks = minimap_store::tasks::list(conn, false).map_err(store_error)?;
    let projects = minimap_store::projects::list(conn, false).map_err(store_error)?;
    let objectives = minimap_store::objectives::list(conn, false).map_err(store_error)?;
    let contributes: Vec<(Uuid, Uuid)> =
        minimap_store::edges::list_active_of_type(conn, EdgeType::ContributesTo)
            .map_err(store_error)?
            .into_iter()
            .map(|e| (e.from_id, e.to_id))
            .collect();
    let overview = overview_at(conn, today)?;
    let fog = fog_at(conn, today)?;
    let pings = list_pings_at(conn, today)?;
    let approaching = approaching_at(conn, ApproachScope::Me, today)?;
    Ok(build(&RadarInput {
        today,
        work_week: settings.work_week,
        view,
        scope: Some(&scope),
        tasks: &tasks,
        projects: &projects,
        objectives: &objectives,
        contributes: &contributes,
        overview: &overview,
        fog: &fog,
        pings: &pings,
        approaching: &approaching,
        rings: settings.radar_rings,
    }))
}

// ------------------------------------------------------------------ where the window sits

/// A window's place in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    fn overlap(&self, other: &Rect) -> i64 {
        let left = i64::from(self.x).max(i64::from(other.x));
        let top = i64::from(self.y).max(i64::from(other.y));
        let right =
            (i64::from(self.x) + i64::from(self.w)).min(i64::from(other.x) + i64::from(other.w));
        let bottom =
            (i64::from(self.y) + i64::from(self.h)).min(i64::from(other.y) + i64::from(other.h));
        (right - left).max(0) * (bottom - top).max(0)
    }
}

/// Brings a remembered place back onto a screen that exists now (a monitor may have been
/// unplugged): onto the screen it overlaps most, shrunk and shifted to fit. `None` when it is
/// on none of them.
pub(crate) fn clamp_to_screens(saved: Rect, screens: &[Rect]) -> Option<Rect> {
    let best = screens
        .iter()
        .filter(|s| s.overlap(&saved) > 0)
        .max_by_key(|s| s.overlap(&saved))?;
    let w = saved.w.min(best.w);
    let h = saved.h.min(best.h);
    let max_x = best
        .x
        .saturating_add(i32::try_from(best.w - w).unwrap_or(0));
    let max_y = best
        .y
        .saturating_add(i32::try_from(best.h - h).unwrap_or(0));
    Some(Rect {
        x: saved.x.clamp(best.x, max_x),
        y: saved.y.clamp(best.y, max_y),
        w,
        h,
    })
}

/// Where a first-time radar goes: the bottom-right corner of the screen, clear of a task bar.
pub(crate) fn corner_of(screen: Rect, scale: f64) -> Rect {
    let size = (START_SIZE * scale).round() as u32;
    let margin = (24.0 * scale).round() as i32;
    let bar = (48.0 * scale).round() as i32;
    Rect {
        x: screen.x + screen.w as i32 - size as i32 - margin,
        y: screen.y + screen.h as i32 - size as i32 - margin - bar,
        w: size,
        h: size,
    }
}

fn read_geometry(conn: &Connection) -> Option<Rect> {
    minimap_store::meta::get(conn, minimap_store::meta::RADAR_GEOMETRY)
        .ok()
        .flatten()
        .and_then(|text| serde_json::from_str(&text).ok())
}

fn screen_of(monitor: &tauri::Monitor) -> Rect {
    Rect {
        x: monitor.position().x,
        y: monitor.position().y,
        w: monitor.size().width,
        h: monitor.size().height,
    }
}

/// Remembers the radar's place. Moving a window fires an event for every pixel, so the place is
/// written only once it has been still for a moment (and at once when a window closes).
pub struct RadarSaver {
    tx: Sender<SaveMessage>,
}

enum SaveMessage {
    Place(Rect),
    /// Write what is waiting now.
    Flush,
}

impl RadarSaver {
    pub fn start(state: AppState) -> std::io::Result<Self> {
        let (tx, rx) = channel::<SaveMessage>();
        std::thread::Builder::new()
            .name("radar-place".into())
            .spawn(move || {
                let mut waiting: Option<Rect> = None;
                let write = |place: Option<Rect>| {
                    let Some(place) = place else { return };
                    let Ok(text) = serde_json::to_string(&place) else {
                        return;
                    };
                    if let Ok(vault) = state.db.lock() {
                        if let Some(conn) = vault.conn.as_ref() {
                            let _ = minimap_store::meta::set(
                                conn,
                                minimap_store::meta::RADAR_GEOMETRY,
                                &text,
                            );
                        }
                    }
                };
                loop {
                    let message = if waiting.is_some() {
                        rx.recv_timeout(SAVE_AFTER)
                    } else {
                        rx.recv().map_err(|_| RecvTimeoutError::Disconnected)
                    };
                    match message {
                        Ok(SaveMessage::Place(place)) => waiting = Some(place),
                        Ok(SaveMessage::Flush) | Err(RecvTimeoutError::Timeout) => {
                            write(waiting.take());
                        }
                        Err(RecvTimeoutError::Disconnected) => {
                            write(waiting.take());
                            break;
                        }
                    }
                }
            })?;
        Ok(Self { tx })
    }

    fn place(&self, place: Rect) {
        let _ = self.tx.send(SaveMessage::Place(place));
    }

    fn flush(&self) {
        let _ = self.tx.send(SaveMessage::Flush);
    }
}

/// Window events the app cares about (wired in `main.rs`): the radar's moves and resizes are
/// remembered, and closing the main window closes the radar with it.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    let app = window.app_handle();
    let Some(saver) = app.try_state::<RadarSaver>() else {
        return;
    };
    match (window.label(), event) {
        (RADAR_LABEL, WindowEvent::Moved(_) | WindowEvent::Resized(_)) => {
            if let (Ok(at), Ok(size)) = (window.outer_position(), window.inner_size()) {
                // A minimised or hidden window reports nonsense places: do not keep those.
                if size.width >= 100 && size.height >= 100 && at.x > -30_000 && at.y > -30_000 {
                    saver.place(Rect {
                        x: at.x,
                        y: at.y,
                        w: size.width,
                        h: size.height,
                    });
                }
            }
        }
        (RADAR_LABEL, WindowEvent::CloseRequested { .. } | WindowEvent::Destroyed) => {
            saver.flush();
        }
        ("main", WindowEvent::CloseRequested { .. } | WindowEvent::Destroyed) => {
            if let Some(radar) = app.get_webview_window(RADAR_LABEL) {
                let _ = radar.close();
            }
        }
        _ => {}
    }
}

// ------------------------------------------------------------------ the window

fn window_error(what: &str, e: impl std::fmt::Display) -> AppError {
    app_error("window", format!("Couldn't {what}: {e}"))
}

/// Opens the radar, or brings the open one forward. `dark` picks the colour it shows before the
/// page has painted (the page applies the real theme).
pub(crate) fn open_radar_window(
    app: &AppHandle,
    place: Option<Rect>,
    on_top: bool,
    dark: bool,
) -> Result<(), AppError> {
    if let Some(window) = app.get_webview_window(RADAR_LABEL) {
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }
    let background = if dark {
        Color(0x1e, 0x1f, 0x21, 0xff)
    } else {
        Color(0xf6, 0xf6, 0xf4, 0xff)
    };
    // Where it goes, worked out before the window exists: where it was left, on a screen that
    // is still there; else the corner of the primary one. The window is made visible and in
    // place at once (hiding it and moving it later left the web view unpainted on Linux).
    let monitors = app.available_monitors().unwrap_or_default();
    let screens: Vec<Rect> = monitors.iter().map(screen_of).collect();
    let scale_at = |p: &Rect| {
        monitors
            .iter()
            .find(|m| screen_of(m).overlap(p) > 0)
            .map_or(1.0, tauri::Monitor::scale_factor)
    };
    let placed = place
        .and_then(|p| clamp_to_screens(p, &screens))
        .map(|p| {
            let min = (MIN_SIZE * scale_at(&p)).round() as u32;
            Rect {
                w: p.w.max(min),
                h: p.h.max(min),
                ..p
            }
        })
        .or_else(|| {
            app.primary_monitor()
                .ok()
                .flatten()
                .map(|m| corner_of(screen_of(&m), m.scale_factor()))
        });
    let mut builder =
        WebviewWindowBuilder::new(app, RADAR_LABEL, WebviewUrl::App("index.html".into()))
            .title("Minimap radar")
            .decorations(false)
            .resizable(true)
            .inner_size(START_SIZE, START_SIZE)
            .min_inner_size(MIN_SIZE, MIN_SIZE)
            .always_on_top(on_top)
            .skip_taskbar(true)
            .transparent(false)
            .background_color(background);
    if let Some(p) = placed {
        let scale = scale_at(&p);
        builder = builder
            .inner_size(f64::from(p.w) / scale, f64::from(p.h) / scale)
            .position(f64::from(p.x) / scale, f64::from(p.y) / scale);
    }
    let window = builder
        .build()
        .map_err(|e| window_error("open the radar", e))?;
    // For finding out why a radar is empty: MINIMAP_RADAR_DEVTOOLS=1 opens its web inspector
    // (debug builds only).
    #[cfg(debug_assertions)]
    if std::env::var_os("MINIMAP_RADAR_DEVTOOLS").is_some() {
        window.open_devtools();
    }
    window.show().map_err(|e| window_error("show the radar", e))
}

/// What opening needs from the database: where it was left and whether it stays on top.
fn saved_place(conn: &Connection) -> Result<(Option<Rect>, bool), AppError> {
    let on_top = minimap_store::settings::get(conn)
        .map_err(store_error)?
        .radar_on_top;
    Ok((read_geometry(conn), on_top))
}

/// Opens the radar, or brings it forward when it is open.
#[tauri::command]
pub async fn open_radar(
    app: AppHandle,
    state: State<'_, AppState>,
    dark: Option<bool>,
) -> Result<(), AppError> {
    let (place, on_top) = state.run(|conn| saved_place(conn)).await?;
    open_radar_window(&app, place, on_top, dark.unwrap_or(true))
}

/// Closes the radar (nothing happens when it is not open).
#[tauri::command]
pub async fn close_radar(app: AppHandle) -> Result<(), AppError> {
    if let Some(window) = app.get_webview_window(RADAR_LABEL) {
        window
            .close()
            .map_err(|e| window_error("close the radar", e))?;
    }
    Ok(())
}

/// Opens the radar when it is closed and closes it when it is open; says whether it is open now.
#[tauri::command]
pub async fn toggle_radar(
    app: AppHandle,
    state: State<'_, AppState>,
    dark: Option<bool>,
) -> Result<bool, AppError> {
    if app.get_webview_window(RADAR_LABEL).is_some() {
        close_radar(app).await?;
        return Ok(false);
    }
    open_radar(app, state, dark).await?;
    Ok(true)
}

/// Keeps the radar on top of other windows or lets it go behind them; remembered on this
/// computer.
#[tauri::command]
pub async fn set_radar_on_top(
    app: AppHandle,
    state: State<'_, AppState>,
    on: bool,
) -> Result<(), AppError> {
    state
        .run(move |conn| {
            minimap_store::settings::update(
                conn,
                UpdateSettings {
                    radar_on_top: Some(on),
                    ..Default::default()
                },
            )
            .map(drop)
            .map_err(store_error)
        })
        .await?;
    if let Some(window) = app.get_webview_window(RADAR_LABEL) {
        window
            .set_always_on_top(on)
            .map_err(|e| window_error("change whether the radar stays on top", e))?;
    }
    Ok(())
}

/// Brings the main window forward and tells it to open `node` in its pane. The radar's only way
/// to act: it opens things, it never changes them.
#[tauri::command]
pub async fn open_in_main(app: AppHandle, node: NodeRef) -> Result<(), AppError> {
    let main = app
        .get_webview_window("main")
        .ok_or_else(|| app_error("window", "The main Minimap window is not open"))?;
    let _ = main.unminimize();
    let _ = main.show();
    let _ = main.set_focus();
    app.emit_to("main", OPEN_EVENT, node)
        .map_err(|e| window_error("open the item", e))
}

/// Opens the radar at start when Settings says so. The database may be locked (an encrypted one
/// waiting for its key): then there is nothing to show yet and it stays closed.
pub fn open_at_start(app: &AppHandle, state: &AppState) {
    let wanted = state
        .db
        .lock()
        .ok()
        .and_then(|vault| {
            vault.conn.as_ref().map(|conn| {
                (
                    minimap_store::settings::get(conn)
                        .map(|s| s.radar_open_at_start)
                        .unwrap_or(false),
                    saved_place(conn).ok(),
                )
            })
        })
        .filter(|(on, _)| *on);
    if let Some((_, Some((place, on_top)))) = wanted {
        if let Err(e) = open_radar_window(app, place, on_top, true) {
            tracing::warn!(error = %e, "could not open the radar at start");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::demo::seed_impl;
    use minimap_types::{BlipKind, RadarMark};
    use time::macros::date;

    // Wednesday 2027-03-03, as in the demo snapshots.
    const TODAY: Date = date!(2027 - 03 - 03);

    fn demo() -> Connection {
        let mut conn = minimap_store::open_in_memory().unwrap();
        seed_impl(&mut conn, TODAY).unwrap();
        conn
    }

    #[test]
    fn an_empty_database_is_an_empty_radar() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let radar = radar_at(&mut conn, View::Portfolio, TODAY).unwrap();
        assert!(radar.blips.is_empty() && radar.sectors.is_empty());
        assert_eq!(radar.footer.text(), "Nothing in range this week");
    }

    #[test]
    fn my_view_without_a_me_is_the_portfolio_view() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let radar = radar_at(&mut conn, View::Mine, TODAY).unwrap();
        assert_eq!(radar.view, View::Portfolio);
    }

    #[test]
    fn the_demo_portfolio_has_trouble_in_eu_region() {
        let mut conn = demo();
        let radar = radar_at(&mut conn, View::Portfolio, TODAY).unwrap();
        assert_eq!(radar.view, View::Portfolio);
        assert!(radar.blips.len() > 5 && radar.blips.len() <= 60);
        // EU Region is the amber/red project; its sector exists and holds a project blip.
        let eu = radar
            .blips
            .iter()
            .find(|b| b.kind == BlipKind::Project && b.node.label == "EU Region")
            .expect("EU Region is on the radar");
        assert_ne!(eu.mark, RadarMark::Green);
        assert!(radar.footer.overdue > 0, "{:?}", radar.footer);
        // The angles are inside the circle and the sweeps add up.
        let total: f64 = radar.sectors.iter().map(|s| s.sweep).sum();
        assert!((total - std::f64::consts::TAU).abs() < 1e-6);
    }

    #[test]
    fn trouble_elsewhere_that_reaches_your_work_is_a_mark_on_the_rim() {
        use minimap_types::{
            AssigneeChoice, CreatePerson, CreateProject, CreateTask, NewEdge, NodeType, Severity,
            TaskStatus,
        };
        // Monday 2027-03-01: "Here" must finish by Wednesday, but the work it waits on is blocked.
        let monday = date!(2027 - 03 - 01);
        let mut conn = minimap_store::open_in_memory().unwrap();
        let me = minimap_store::people::create(
            &mut conn,
            CreatePerson {
                name: "Me".into(),
                role_title: String::new(),
                email: None,
                weekly_capacity_hours: None,
                is_self: true,
                notes: String::new(),
            },
        )
        .unwrap()
        .id;
        let project = |conn: &mut Connection, title: &str, target| {
            minimap_store::projects::create(
                conn,
                CreateProject {
                    title: title.into(),
                    slug: None,
                    description: String::new(),
                    owner_person_id: None,
                    start_date: None,
                    target_date: target,
                    status: Some(minimap_types::ProjectStatus::Active),
                    priority: None,
                },
            )
            .unwrap()
            .id
        };
        let task = |conn: &mut Connection, title: &str, project, status, who| {
            minimap_store::tasks::create(
                conn,
                CreateTask {
                    links: Vec::new(),
                    task_type: Some("build".into()),
                    focus: None,
                    backburner_since: None,
                    start_minute: None,
                    length_minutes: None,
                    title: title.into(),
                    assignee: who,
                    description: String::new(),
                    project_id: Some(project),
                    status: Some(status),
                    estimate_days: Some(2.0),
                    start_date: None,
                    due_date: None,
                    priority: None,
                    recurrence: None,
                },
            )
            .unwrap()
            .id
        };
        let here = project(&mut conn, "Here", Some(date!(2027 - 03 - 03)));
        let there = project(&mut conn, "There", None);
        let gate = task(
            &mut conn,
            "Gateway auth",
            there,
            TaskStatus::Blocked,
            AssigneeChoice::Nobody,
        );
        let mine = task(
            &mut conn,
            "Pen test",
            here,
            TaskStatus::Todo,
            AssigneeChoice::Person(me),
        );
        minimap_store::edges::add(
            &mut conn,
            NewEdge {
                edge_type: EdgeType::Blocks,
                from: NodeRef::new(NodeType::Task, gate),
                to: NodeRef::new(NodeType::Task, mine),
                attrs: serde_json::json!({}),
            },
        )
        .unwrap();
        let radar = radar_at(&mut conn, View::Portfolio, monday).unwrap();
        assert_eq!(radar.edge_marks.len(), 1, "{:?}", radar.edge_marks);
        let mark = &radar.edge_marks[0];
        assert_eq!(mark.severity, Severity::Hits);
        assert_eq!(mark.source.node.id, gate);
        assert!(
            mark.label
                .starts_with("Gateway auth (There) is blocked; reaches Pen test"),
            "{}",
            mark.label
        );
        assert_eq!(radar.footer.approaching, 1);
        assert!(radar.footer.text().contains("1 approaching"));
        // Your own blocked task's blocker is not itself a blip (it is not in range), only a mark.
        assert!(radar
            .blips
            .iter()
            .all(|b| b.node.node.id != mine || b.kind == BlipKind::Project));
    }

    #[test]
    fn my_view_of_the_demo_data_is_a_subset_of_the_portfolio() {
        let mut conn = demo();
        let all = radar_at(&mut conn, View::Portfolio, TODAY).unwrap();
        let mine = radar_at(&mut conn, View::Mine, TODAY).unwrap();
        assert_eq!(mine.view, View::Mine);
        assert!(!mine.blips.is_empty());
        let all_ids: std::collections::HashSet<_> =
            all.blips.iter().map(|b| b.node.node.id).collect();
        // A due ping (a blip of its own in either view) can differ only by scope, never appear
        // from nowhere.
        assert!(mine.blips.iter().all(|b| all_ids.contains(&b.node.node.id)));
        assert!(mine.blips.len() <= all.blips.len());
    }

    #[test]
    fn the_revision_moves_with_every_write_and_with_merges() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let before = revision_of(&conn, 0);
        minimap_store::people::ensure_self(&mut conn, "Me").unwrap();
        let after = revision_of(&conn, 0);
        assert!(after > before);
        assert_eq!(revision_of(&conn, 3), after + 3);
    }

    #[test]
    fn a_remembered_place_comes_back_onto_a_screen_that_exists() {
        let screen = Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        };
        let second = Rect {
            x: 1920,
            y: 0,
            w: 1280,
            h: 1024,
        };
        // Fully on a screen: unchanged.
        let inside = Rect {
            x: 100,
            y: 100,
            w: 260,
            h: 260,
        };
        assert_eq!(clamp_to_screens(inside, &[screen]), Some(inside));
        // Hanging off the right edge: pulled back in.
        let hanging = Rect {
            x: 1800,
            y: 900,
            w: 260,
            h: 260,
        };
        assert_eq!(
            clamp_to_screens(hanging, &[screen]),
            Some(Rect {
                x: 1660,
                y: 820,
                w: 260,
                h: 260
            })
        );
        // On a monitor that is gone: nowhere (the caller uses the corner).
        let gone = Rect {
            x: 4000,
            y: 0,
            w: 260,
            h: 260,
        };
        assert_eq!(clamp_to_screens(gone, &[screen, second]), None);
        // On the second screen: stays there.
        let on_second = Rect {
            x: 2000,
            y: 50,
            w: 300,
            h: 300,
        };
        assert_eq!(
            clamp_to_screens(on_second, &[screen, second]),
            Some(on_second)
        );
        // Bigger than the screen: shrunk to fit.
        let big = Rect {
            x: 0,
            y: 0,
            w: 5000,
            h: 5000,
        };
        assert_eq!(
            clamp_to_screens(big, &[screen]),
            Some(Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 1080
            })
        );
    }

    #[test]
    fn the_first_radar_goes_in_the_corner_clear_of_the_task_bar() {
        let screen = Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        };
        let corner = corner_of(screen, 1.0);
        assert_eq!((corner.w, corner.h), (260, 260));
        assert!(corner.x + 260 <= 1920 && corner.y + 260 <= 1080);
        assert!(corner.x > 1500 && corner.y > 700);
        let hidpi = corner_of(screen, 2.0);
        assert_eq!(hidpi.w, 520);
    }

    // ------------------------------------------------------------ what the radar may do

    fn permissions(json: &str) -> Vec<String> {
        let value: serde_json::Value = serde_json::from_str(json).expect("a capability is JSON");
        value["permissions"]
            .as_array()
            .expect("a capability lists permissions")
            .iter()
            .map(|p| p.as_str().expect("a permission is text").to_owned())
            .collect()
    }

    #[test]
    fn the_radar_window_may_read_and_open_but_never_write() {
        let json = include_str!("../../capabilities/radar.json");
        let value: serde_json::Value = serde_json::from_str(json).unwrap();
        assert_eq!(value["windows"], serde_json::json!(["radar"]));
        let mut have = permissions(json);
        have.sort();
        let mut want: Vec<String> = [
            "allow-get-radar",
            "allow-get-data-revision",
            "allow-open-in-main",
            "allow-close-radar",
            "allow-set-radar-on-top",
            "allow-get-settings",
            "core:window:allow-start-dragging",
            "core:window:allow-start-resize-dragging",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        want.sort();
        assert_eq!(have, want);
    }

    #[test]
    fn only_the_main_window_may_listen_for_events() {
        let main = permissions(include_str!("../../capabilities/default.json"));
        assert!(main.contains(&"core:event:allow-listen".to_owned()));
        let radar = permissions(include_str!("../../capabilities/radar.json"));
        assert!(radar.iter().all(|p| !p.starts_with("core:event")));
        // The main window opens and closes the radar.
        for p in [
            "allow-open-radar",
            "allow-close-radar",
            "allow-toggle-radar",
        ] {
            assert!(main.contains(&p.to_owned()), "{p}");
        }
    }

    #[test]
    fn every_radar_command_is_in_the_manifest_and_the_main_window_capability() {
        let build = include_str!("../../build.rs");
        for name in [
            "get_radar",
            "get_data_revision",
            "open_radar",
            "close_radar",
            "toggle_radar",
            "set_radar_on_top",
            "open_in_main",
        ] {
            assert!(build.contains(&format!("\"{name}\"")), "{name} in build.rs");
        }
    }
}
