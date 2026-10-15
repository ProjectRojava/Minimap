//! The watcher: one background thread that looks after what the window can't while it is hidden
//! in the tray. Every half minute it
//! - tells the tray what needs attention (a badge and the tooltip),
//! - sends the meeting reminders and the day's "pings are waiting" note, and
//! - locks an encrypted database when the window has been hidden for as long as Settings says.
//!
//! The rules (who is reminded, when) are plain functions so they can be tested. The database is
//! only read again when something in it has changed (`radar::revision_of`) or the day has turned.

use std::{collections::HashSet, sync::mpsc::channel, time::Duration};

use anyhow::Context;
use minimap_types::{Date, TaskStatus, Uuid};
use tauri::{AppHandle, Emitter, Manager};
use time::{OffsetDateTime, UtcOffset};

use crate::{
    commands::{pings::list_pings_at, radar, security, this_week::this_week_impl},
    shell::{lock_due, Shell},
    state::AppState,
    tray::{self, LOCKED_EVENT},
};

/// How often the watcher looks.
const EVERY: Duration = Duration::from_secs(30);
/// How long a lock waits for unsaved changes to reach Drive.
const FLUSH_WAIT: Duration = Duration::from_secs(10);

/// A meeting today, for the reminders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeetingSlot {
    pub id: Uuid,
    pub title: String,
    pub date: Date,
    /// Start, as a minute of the day on the user's clock.
    pub minute: u16,
}

/// The user's clock: the day and the minute of the day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalNow {
    pub date: Date,
    pub minute: u16,
}

/// What time it is for the user, from UTC and how far ahead their clock is.
pub fn local_now(now_utc: OffsetDateTime, utc_offset_minutes: i32) -> LocalNow {
    let offset = utc_offset_minutes
        .checked_mul(60)
        .and_then(|seconds| UtcOffset::from_whole_seconds(seconds).ok())
        .unwrap_or(UtcOffset::UTC);
    let local = now_utc.to_offset(offset);
    LocalNow {
        date: local.date(),
        minute: u16::from(local.hour()) * 60 + u16::from(local.minute()),
    }
}

/// A notification to show once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reminder {
    /// Names this reminder, so it is sent once: the meeting and the time it was for (a meeting
    /// that is moved is reminded again).
    pub key: String,
    pub title: String,
    pub body: String,
}

/// The meetings starting within `lead` minutes (or that start this minute) that have not been
/// reminded of yet.
pub fn meeting_reminders(
    slots: &[MeetingSlot],
    now: LocalNow,
    lead: u32,
    sent: &HashSet<String>,
) -> Vec<Reminder> {
    slots
        .iter()
        .filter(|s| s.date == now.date && s.minute >= now.minute)
        .filter(|s| u32::from(s.minute - now.minute) <= lead)
        .map(|s| {
            let left = s.minute - now.minute;
            let body = match left {
                0 => "Starting now".to_owned(),
                1 => "Starts in 1 minute".to_owned(),
                n => format!("Starts in {n} minutes"),
            };
            Reminder {
                key: format!("{}@{}:{}", s.id, s.date, s.minute),
                title: s.title.clone(),
                body,
            }
        })
        .filter(|r| !sent.contains(&r.key))
        .collect()
}

/// The note for pings waiting today: at most one a day.
pub fn ping_digest(due: usize, today: Date, last_sent: Option<Date>) -> Option<(String, String)> {
    if due == 0 || last_sent == Some(today) {
        return None;
    }
    let body = if due == 1 {
        "1 ping is waiting for you today".to_owned()
    } else {
        format!("{due} pings are waiting for you today")
    };
    Some(("Pings".to_owned(), body))
}

// ------------------------------------------------------------------ what it knows

/// What the thread has worked out, so the database is read again only when it changed.
#[derive(Default)]
struct Memory {
    revision: Option<i64>,
    day: Option<Date>,
    slots: Vec<MeetingSlot>,
    overdue: u32,
    pings: u32,
    shown: Option<(u32, u32)>,
    sent: HashSet<String>,
}

/// Reads what the watcher needs; `None` while the database is locked.
fn refresh(state: &AppState, memory: &mut Memory, day: Date) -> Option<()> {
    let vault = state.db.lock().ok()?;
    let conn = vault.conn.as_ref()?;
    let revision = radar::revision_of(conn, state.sync.engine.data_revision());
    if memory.revision == Some(revision) && memory.day == Some(day) {
        return Some(());
    }
    let today = minimap_store::today();
    memory.slots = minimap_store::tasks::list(conn, false)
        .ok()?
        .into_iter()
        .filter(|t| t.status == TaskStatus::Todo && t.due_date == Some(day))
        .filter_map(|t| {
            Some(MeetingSlot {
                id: t.id,
                title: t.title,
                date: day,
                minute: t.start_minute?,
            })
        })
        .collect();
    memory.overdue = this_week_impl(conn, None, None)
        .map(|w| u32::try_from(w.overdue.len()).unwrap_or(u32::MAX))
        .unwrap_or(0);
    memory.pings = list_pings_at(conn, today)
        .map(|p| u32::try_from(p.due.len()).unwrap_or(u32::MAX))
        .unwrap_or(0);
    if memory.day != Some(day) {
        // A new day: the reminders of yesterday's meetings are over.
        memory.sent.clear();
    }
    memory.revision = Some(revision);
    memory.day = Some(day);
    Some(())
}

/// Whether the main window is on screen (so a note about what is in it would say nothing new).
fn window_in_view(app: &AppHandle) -> bool {
    app.get_webview_window("main")
        .is_some_and(|w| w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false))
}

/// Locks the database now: waits for unsaved changes to reach Drive, drops the key, closes the
/// radar (it would only show errors) and tells the window to show the unlock screen.
pub fn lock_now(app: &AppHandle, state: &AppState) {
    let lockable = state
        .db
        .lock()
        .map(|v| security::lockable(&v))
        .unwrap_or(false);
    if !lockable {
        return;
    }
    let engine = state.sync.engine.clone();
    let (done, wait) = channel();
    std::thread::spawn(move || {
        engine.flush();
        let _ = done.send(());
    });
    let _ = wait.recv_timeout(FLUSH_WAIT);
    let locked = state
        .db
        .lock()
        .map(|mut v| security::lock_impl(&mut v))
        .unwrap_or(false);
    if locked {
        state.forget_undo();
        if let Some(radar) = app.get_webview_window(radar::RADAR_LABEL) {
            let _ = radar.close();
        }
        let _ = app.emit_to("main", LOCKED_EVENT, ());
    }
}

fn tick(app: &AppHandle, state: &AppState, shell: &Shell, memory: &mut Memory) {
    let settings = shell.settings();
    if lock_due(shell.hidden_for(), settings.lock_after_minutes) {
        lock_now(app, state);
    }
    let offset = state.clock.offset();
    if let Some(minutes) = offset {
        shell.update_notes(|n| n.utc_offset_minutes = Some(minutes));
    }
    let now = offset.map(|m| local_now(OffsetDateTime::now_utc(), m));
    let day = now.map_or_else(minimap_store::today, |n| n.date);
    if refresh(state, memory, day).is_none() {
        return;
    }
    if memory.shown != Some((memory.overdue, memory.pings)) {
        tray::show_attention(app, memory.overdue, memory.pings);
        memory.shown = Some((memory.overdue, memory.pings));
    }
    if let (Some(now), true) = (now, settings.notify_meetings) {
        for r in meeting_reminders(
            &memory.slots,
            now,
            settings.meeting_lead_minutes,
            &memory.sent,
        ) {
            tray::notify(app, &r.title, &r.body);
            memory.sent.insert(r.key);
        }
    }
    if settings.notify_pings && !window_in_view(app) {
        let today = minimap_store::today();
        let last = shell.notes().pings_notified_on;
        if let Some((title, body)) = ping_digest(memory.pings as usize, today, last) {
            tray::notify(app, &title, &body);
            shell.update_notes(|n| n.pings_notified_on = Some(today));
        }
    }
}

/// Starts the thread.
pub fn spawn(app: AppHandle) -> anyhow::Result<()> {
    std::thread::Builder::new()
        .name("tray-watch".into())
        .spawn(move || {
            let mut memory = Memory::default();
            // Let the window come up first.
            std::thread::sleep(Duration::from_secs(5));
            loop {
                if let (Some(state), Some(shell)) =
                    (app.try_state::<AppState>(), app.try_state::<Shell>())
                {
                    tick(&app, &state, &shell, &mut memory);
                }
                std::thread::sleep(EVERY);
            }
        })
        .context("start the tray watcher")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::{date, datetime};

    const DAY: Date = date!(2027 - 03 - 03);

    fn slot(title: &str, minute: u16) -> MeetingSlot {
        MeetingSlot {
            id: Uuid::from_u128(u128::from(minute) + 1),
            title: title.into(),
            date: DAY,
            minute,
        }
    }

    fn at(minute: u16) -> LocalNow {
        LocalNow { date: DAY, minute }
    }

    #[test]
    fn the_clock_is_utc_shifted_by_the_offset_the_screen_gave() {
        // 18:30 UTC is midnight the next day in India (+5:30).
        let now = datetime!(2027-03-03 18:30 UTC);
        assert_eq!(
            local_now(now, 330),
            LocalNow {
                date: date!(2027 - 03 - 04),
                minute: 0
            }
        );
        // And 05:00 on the 3rd in New York in winter (-5:00).
        let now = datetime!(2027-03-03 10:00 UTC);
        assert_eq!(
            local_now(now, -300),
            LocalNow {
                date: DAY,
                minute: 5 * 60
            }
        );
        // An offset that makes no sense is read as UTC rather than failing.
        assert_eq!(local_now(now, i32::MAX).minute, 10 * 60);
    }

    #[test]
    fn a_meeting_is_reminded_once_inside_the_lead_time() {
        let standup = slot("Standup", 10 * 60);
        let slots = [standup.clone(), slot("Later", 15 * 60)];
        let mut sent = HashSet::new();

        // 20 minutes before, with a 10 minute lead: not yet.
        assert!(meeting_reminders(&slots, at(10 * 60 - 20), 10, &sent).is_empty());
        // 10 minutes before: now.
        let due = meeting_reminders(&slots, at(10 * 60 - 10), 10, &sent);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].title, "Standup");
        assert_eq!(due[0].body, "Starts in 10 minutes");
        sent.insert(due[0].key.clone());
        // Half a minute later it is the same minute: not again.
        assert!(meeting_reminders(&slots, at(10 * 60 - 10), 10, &sent).is_empty());
        // The words change as the time comes.
        assert_eq!(
            meeting_reminders(&slots, at(10 * 60 - 1), 10, &HashSet::new())[0].body,
            "Starts in 1 minute"
        );
        assert_eq!(
            meeting_reminders(&slots, at(10 * 60), 10, &HashSet::new())[0].body,
            "Starting now"
        );
    }

    #[test]
    fn a_meeting_that_has_started_or_is_another_day_is_not_reminded() {
        let slots = [slot("Standup", 10 * 60)];
        assert!(meeting_reminders(&slots, at(10 * 60 + 1), 10, &HashSet::new()).is_empty());
        let tomorrow = LocalNow {
            date: date!(2027 - 03 - 04),
            minute: 9 * 60,
        };
        assert!(meeting_reminders(&slots, tomorrow, 120, &HashSet::new()).is_empty());
    }

    #[test]
    fn a_moved_meeting_is_reminded_again() {
        let mut m = slot("Standup", 10 * 60);
        let mut sent = HashSet::new();
        let first = meeting_reminders(&[m.clone()], at(9 * 60 + 55), 10, &sent);
        sent.insert(first[0].key.clone());
        m.minute = 11 * 60;
        // Moved to 11:00: a new reminder when that is near.
        assert!(meeting_reminders(&[m.clone()], at(9 * 60 + 55), 10, &sent).is_empty());
        assert_eq!(
            meeting_reminders(&[m], at(10 * 60 + 55), 10, &sent).len(),
            1
        );
    }

    #[test]
    fn pings_are_announced_once_a_day_and_only_when_there_are_some() {
        assert!(ping_digest(0, DAY, None).is_none());
        let (title, body) = ping_digest(1, DAY, None).unwrap();
        assert_eq!(
            (title.as_str(), body.as_str()),
            ("Pings", "1 ping is waiting for you today")
        );
        assert_eq!(
            ping_digest(3, DAY, Some(date!(2027 - 03 - 02))).unwrap().1,
            "3 pings are waiting for you today"
        );
        assert!(
            ping_digest(3, DAY, Some(DAY)).is_none(),
            "already told today"
        );
    }
}
