//! Settings that belong to one computer and are never synced: what closing the window does, the
//! system tray, starting with the computer, reminders and locking. They live in a small file in
//! the app's data folder (not in the database), because the shell needs them before the database
//! is open (start hidden) and while it is locked, and because another computer may want other
//! answers.

use serde::{Deserialize, Serialize};

/// Longest lead time for a meeting reminder, in minutes.
pub const MAX_MEETING_LEAD_MINUTES: u32 = 120;
/// Longest time the window may stay hidden before an encrypted database locks, in minutes.
pub const MAX_LOCK_AFTER_MINUTES: u32 = 24 * 60;

/// What the window's close button does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnClose {
    /// Ask the first time and remember the answer (the default).
    #[default]
    Ask,
    /// Hide the window and keep running in the system tray.
    Tray,
    /// Close Minimap.
    Quit,
}

/// The answer to "keep running in the tray?".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloseChoice {
    Tray,
    Quit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DeviceSettings {
    pub on_close: OnClose,
    /// The minimise button hides the window to the tray too (otherwise it goes to the task bar).
    pub minimise_to_tray: bool,
    /// Start Minimap when the computer starts.
    pub start_at_login: bool,
    /// When it was started by the computer, stay in the tray instead of opening the window.
    pub start_hidden: bool,
    /// A notification shortly before a meeting starts.
    pub notify_meetings: bool,
    /// How many minutes before (1..=120).
    pub meeting_lead_minutes: u32,
    /// One notification a day when pings are waiting.
    pub notify_pings: bool,
    /// Lock an encrypted database (one opened with a passphrase) after the window has been hidden
    /// this many minutes. 0 means never.
    pub lock_after_minutes: u32,
}

impl Default for DeviceSettings {
    fn default() -> Self {
        Self {
            on_close: OnClose::Ask,
            minimise_to_tray: false,
            start_at_login: false,
            start_hidden: true,
            notify_meetings: true,
            meeting_lead_minutes: 10,
            notify_pings: true,
            lock_after_minutes: 0,
        }
    }
}

impl DeviceSettings {
    /// `Err` with a sentence for the person when a number is out of range.
    pub fn check(&self) -> Result<(), String> {
        if !(1..=MAX_MEETING_LEAD_MINUTES).contains(&self.meeting_lead_minutes) {
            return Err(format!(
                "A meeting reminder comes 1 to {MAX_MEETING_LEAD_MINUTES} minutes before"
            ));
        }
        if self.lock_after_minutes > MAX_LOCK_AFTER_MINUTES {
            return Err(format!(
                "The lock time is at most {MAX_LOCK_AFTER_MINUTES} minutes (a day), or 0 for never"
            ));
        }
        Ok(())
    }
}

/// The device settings and what this computer can do, for Settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceView {
    pub settings: DeviceSettings,
    /// The tray icon exists. When it doesn't (the system has no place for it), closing always
    /// quits, whatever the setting says.
    pub tray_available: bool,
    /// The database is encrypted with a passphrase, the only kind that can lock again without
    /// the recovery key.
    pub can_lock: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_ask_first_and_lock_nothing() {
        let d = DeviceSettings::default();
        assert_eq!(d.on_close, OnClose::Ask);
        assert!(!d.minimise_to_tray && !d.start_at_login);
        assert_eq!(d.lock_after_minutes, 0);
        assert!(d.check().is_ok());
    }

    #[test]
    fn numbers_out_of_range_are_refused_with_a_sentence() {
        let with = |lead: u32, lock: u32| DeviceSettings {
            meeting_lead_minutes: lead,
            lock_after_minutes: lock,
            ..Default::default()
        };
        assert!(with(0, 0).check().unwrap_err().contains("minutes before"));
        assert!(with(MAX_MEETING_LEAD_MINUTES + 1, 0).check().is_err());
        assert!(with(15, MAX_LOCK_AFTER_MINUTES + 1)
            .check()
            .unwrap_err()
            .contains("never"));
        assert!(with(15, MAX_LOCK_AFTER_MINUTES).check().is_ok());
    }

    #[test]
    fn a_file_with_missing_or_new_fields_still_reads() {
        // Older files lack newer fields; they take their defaults.
        let d: DeviceSettings = serde_json::from_str(r#"{"on_close":"tray"}"#).unwrap();
        assert_eq!(d.on_close, OnClose::Tray);
        assert_eq!(d.meeting_lead_minutes, 10);
        assert!(d.notify_meetings && d.start_hidden);
    }
}
