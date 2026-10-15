//! The shell's own state: the settings of this computer (what closing does, the tray, starting
//! with the computer, reminders, locking), which live in a small file in the data folder and
//! never in the database, and the decisions made from them. The decisions are plain functions so
//! they can be tested; the tray (`tray.rs`) and the watcher (`watch.rs`) act on them.

use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, MutexGuard,
    },
    time::{Duration, Instant},
};

use minimap_types::{Date, DeviceSettings, OnClose};
use serde::{Deserialize, Serialize};

/// The user's choices for this computer.
pub const SETTINGS_FILE: &str = "device-settings.json";
/// What the shell remembers for itself (not shown anywhere).
pub const NOTES_FILE: &str = "device-state.json";
/// The argument the operating system passes when it starts Minimap with the computer.
pub const AUTOSTART_ARG: &str = "--autostart";

/// Reads the settings file. A missing, damaged or out-of-range file gives the defaults: a bad
/// file must never stop the app from opening.
pub fn read_settings(dir: &Path) -> DeviceSettings {
    std::fs::read_to_string(dir.join(SETTINGS_FILE))
        .ok()
        .and_then(|text| serde_json::from_str::<DeviceSettings>(&text).ok())
        .filter(|s| s.check().is_ok())
        .unwrap_or_default()
}

/// Writes to a temporary file and renames it, so a crash never leaves half a file.
fn write_json<T: Serialize>(dir: &Path, name: &str, value: &T) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let text = serde_json::to_string_pretty(value).map_err(std::io::Error::other)?;
    let temp = dir.join(format!("{name}.tmp"));
    std::fs::write(&temp, text)?;
    std::fs::rename(&temp, dir.join(name))
}

pub fn write_settings(dir: &Path, settings: &DeviceSettings) -> std::io::Result<()> {
    write_json(dir, SETTINGS_FILE, settings)
}

/// What the shell keeps for itself between runs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Notes {
    /// How far the user's clock is ahead of UTC, as the screen last said it. The shell has no
    /// reliable way to know the time zone itself, and needs it for meeting reminders.
    pub utc_offset_minutes: Option<i32>,
    /// The day the "pings are waiting" notification was last sent.
    pub pings_notified_on: Option<Date>,
    /// The one-time "Minimap is still running here" note has been shown.
    pub tray_hint_shown: bool,
}

pub fn read_notes(dir: &Path) -> Notes {
    std::fs::read_to_string(dir.join(NOTES_FILE))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn write_notes(dir: &Path, notes: &Notes) -> std::io::Result<()> {
    write_json(dir, NOTES_FILE, notes)
}

// ------------------------------------------------------------------ the decisions

/// What the window's close button does right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseAction {
    /// Hide the window; Minimap keeps running in the tray.
    Hide,
    /// Close Minimap.
    Quit,
    /// Ask the person which, in the window.
    Ask,
}

/// `quitting`: Quit was chosen (tray menu, dialog, Settings), so nothing may hold it back.
/// Without a tray there is nothing to click to bring the window back, so closing quits. While the
/// page has not loaded there is no dialog to ask with, so that quits too.
pub fn close_action(
    on_close: OnClose,
    tray_available: bool,
    ui_ready: bool,
    quitting: bool,
) -> CloseAction {
    if quitting || !tray_available {
        return CloseAction::Quit;
    }
    match on_close {
        OnClose::Tray => CloseAction::Hide,
        OnClose::Quit => CloseAction::Quit,
        OnClose::Ask if ui_ready => CloseAction::Ask,
        OnClose::Ask => CloseAction::Quit,
    }
}

/// Started by the computer at login, and the person wants that to stay in the tray (which needs a
/// tray to come back from).
pub fn starts_hidden(args: &[String], settings: &DeviceSettings, tray_available: bool) -> bool {
    tray_available && settings.start_hidden && args.iter().any(|a| a == AUTOSTART_ARG)
}

/// The window has been hidden long enough that an encrypted database should lock.
pub fn lock_due(hidden_for: Option<Duration>, lock_after_minutes: u32) -> bool {
    lock_after_minutes > 0
        && hidden_for.is_some_and(|d| d >= Duration::from_secs(u64::from(lock_after_minutes) * 60))
}

// ------------------------------------------------------------------ the state

/// Managed by Tauri; shared by the window events, the tray, the commands and the watcher.
pub struct Shell {
    pub dir: PathBuf,
    settings: Mutex<DeviceSettings>,
    notes: Mutex<Notes>,
    quitting: AtomicBool,
    tray: AtomicBool,
    ui_ready: AtomicBool,
    /// Started in the tray and the window has not been shown yet.
    hidden_start: AtomicBool,
    hidden_since: Mutex<Option<Instant>>,
}

fn locked<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Shell {
    pub fn new(dir: PathBuf) -> Self {
        let settings = read_settings(&dir);
        let notes = read_notes(&dir);
        Self {
            dir,
            settings: Mutex::new(settings),
            notes: Mutex::new(notes),
            quitting: AtomicBool::new(false),
            tray: AtomicBool::new(false),
            ui_ready: AtomicBool::new(false),
            hidden_start: AtomicBool::new(false),
            hidden_since: Mutex::new(None),
        }
    }

    pub fn settings(&self) -> DeviceSettings {
        locked(&self.settings).clone()
    }

    pub fn set_settings(&self, settings: DeviceSettings) -> std::io::Result<()> {
        write_settings(&self.dir, &settings)?;
        *locked(&self.settings) = settings;
        Ok(())
    }

    pub fn notes(&self) -> Notes {
        locked(&self.notes).clone()
    }

    /// Changes the notes and keeps them, when they changed.
    pub fn update_notes(&self, change: impl FnOnce(&mut Notes)) {
        let mut notes = locked(&self.notes);
        let before = notes.clone();
        change(&mut notes);
        if *notes != before {
            if let Err(e) = write_notes(&self.dir, &notes) {
                tracing::warn!(error = %e, "could not keep the shell's notes");
            }
        }
    }

    pub fn tray_available(&self) -> bool {
        self.tray.load(Ordering::SeqCst)
    }

    pub fn set_tray_available(&self, on: bool) {
        self.tray.store(on, Ordering::SeqCst);
    }

    pub fn quitting(&self) -> bool {
        self.quitting.load(Ordering::SeqCst)
    }

    pub fn begin_quit(&self) {
        self.quitting.store(true, Ordering::SeqCst);
    }

    pub fn ui_ready(&self) -> bool {
        self.ui_ready.load(Ordering::SeqCst)
    }

    pub fn set_ui_ready(&self) {
        self.ui_ready.store(true, Ordering::SeqCst);
    }

    /// Started in the tray, and nobody has opened the window yet.
    pub fn hidden_start(&self) -> bool {
        self.hidden_start.load(Ordering::SeqCst)
    }

    pub fn set_hidden_start(&self, on: bool) {
        self.hidden_start.store(on, Ordering::SeqCst);
    }

    /// The window was hidden to the tray just now (or shown again, with `false`).
    pub fn note_hidden(&self, hidden: bool) {
        *locked(&self.hidden_since) = hidden.then(Instant::now);
    }

    pub fn hidden_for(&self) -> Option<Duration> {
        locked(&self.hidden_since).map(|t| t.elapsed())
    }

    pub fn close_action(&self) -> CloseAction {
        close_action(
            self.settings().on_close,
            self.tray_available(),
            self.ui_ready(),
            self.quitting(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("minimap-shell-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn closing_follows_the_setting_when_there_is_a_tray() {
        use CloseAction::*;
        assert_eq!(close_action(OnClose::Tray, true, true, false), Hide);
        assert_eq!(close_action(OnClose::Quit, true, true, false), Quit);
        assert_eq!(close_action(OnClose::Ask, true, true, false), Ask);
    }

    #[test]
    fn closing_quits_when_nothing_could_bring_the_window_back() {
        use CloseAction::*;
        // No tray: every setting quits, or the app would vanish with nothing to click.
        for on_close in [OnClose::Ask, OnClose::Tray, OnClose::Quit] {
            assert_eq!(close_action(on_close, false, true, false), Quit);
        }
        // Quit was chosen: nothing holds it back.
        assert_eq!(close_action(OnClose::Tray, true, true, true), Quit);
        // The page has not loaded, so there is no dialog to ask with.
        assert_eq!(close_action(OnClose::Ask, true, false, false), Quit);
        // ...but a remembered choice needs no dialog.
        assert_eq!(close_action(OnClose::Tray, true, false, false), Hide);
    }

    #[test]
    fn only_a_start_by_the_computer_with_a_tray_starts_hidden() {
        let on = DeviceSettings::default();
        let args = |a: &[&str]| a.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert!(starts_hidden(&args(&["minimap", AUTOSTART_ARG]), &on, true));
        // Started by hand: the window opens.
        assert!(!starts_hidden(&args(&["minimap"]), &on, true));
        // No tray to come back from: the window opens.
        assert!(!starts_hidden(
            &args(&["minimap", AUTOSTART_ARG]),
            &on,
            false
        ));
        // The person wants the window at login.
        let mut shown = on;
        shown.start_hidden = false;
        assert!(!starts_hidden(
            &args(&["minimap", AUTOSTART_ARG]),
            &shown,
            true
        ));
    }

    #[test]
    fn the_lock_comes_after_the_time_and_never_when_it_is_zero() {
        let mins = |m: u64| Some(Duration::from_secs(m * 60));
        assert!(!lock_due(mins(100), 0), "0 means never");
        assert!(!lock_due(None, 5), "a visible window never locks");
        assert!(!lock_due(mins(4), 5));
        assert!(lock_due(mins(5), 5));
        assert!(lock_due(mins(60), 5));
    }

    #[test]
    fn the_settings_file_round_trips_and_a_bad_one_gives_the_defaults() {
        let dir = temp("settings");
        assert_eq!(read_settings(&dir), DeviceSettings::default());
        let s = DeviceSettings {
            on_close: OnClose::Tray,
            lock_after_minutes: 30,
            ..Default::default()
        };
        write_settings(&dir, &s).unwrap();
        assert_eq!(read_settings(&dir), s);
        // Damaged, or holding a number out of range: the defaults.
        std::fs::write(dir.join(SETTINGS_FILE), "{ not json").unwrap();
        assert_eq!(read_settings(&dir), DeviceSettings::default());
        std::fs::write(dir.join(SETTINGS_FILE), r#"{"meeting_lead_minutes":0}"#).unwrap();
        assert_eq!(read_settings(&dir), DeviceSettings::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_notes_are_kept_only_when_they_change() {
        let dir = temp("notes");
        let shell = Shell::new(dir.clone());
        assert_eq!(shell.notes(), Notes::default());
        shell.update_notes(|n| n.tray_hint_shown = true);
        assert!(read_notes(&dir).tray_hint_shown);
        // Same again: nothing is rewritten (the file is gone, and stays gone).
        std::fs::remove_file(dir.join(NOTES_FILE)).unwrap();
        shell.update_notes(|n| n.tray_hint_shown = true);
        assert!(!dir.join(NOTES_FILE).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hiding_starts_the_clock_and_showing_stops_it() {
        let shell = Shell::new(temp("hidden"));
        assert!(shell.hidden_for().is_none());
        shell.note_hidden(true);
        assert!(shell.hidden_for().is_some());
        shell.note_hidden(false);
        assert!(shell.hidden_for().is_none());
    }
}
