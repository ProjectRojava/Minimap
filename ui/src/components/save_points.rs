//! Save points (docs/product-design/08-save-points.md, spec 51): every restorable copy of the
//! data on one timeline, `Ctrl/Cmd+S` to make a named one, and "Last save point 2 h ago" in the
//! status bar. The copies are the local backups of spec 20 and Google Drive's checkpoints of spec
//! 22; restoring uses their existing flows. Undo (spec 25) is a different thing: per change and
//! per session.

use std::time::Duration;

use leptos::{ev, html, prelude::*, task::spawn_local};
use leptos_router::components::A;
use minimap_types::{
    BackupKind, CheckpointCadence, CheckpointInfo, SaveDay, SavePoint, SaveSource, SaveSummary,
    SaveTimeline, StaleReason,
};
use time::{Date, OffsetDateTime, UtcOffset};

use crate::{
    api,
    components::{
        backup_settings::{check, size_text, ConfirmRestore, KeyPrompt, Restore},
        drive_settings::ConfirmRecover,
        form::{BUTTON, BUTTON_PRIMARY, INPUT},
        meeting::local_clock,
        page::{Card, Icon, Tone},
    },
    state::{DataVersion, Toasts},
};

/// How often the status bar asks again (it also asks whenever data changes).
const REFRESH: Duration = Duration::from_secs(300);

// ------------------------------------------------------------------ pure wording

/// "just now", "12 min ago", "2 h ago", "yesterday", "3 days ago".
pub fn last_text(age_minutes: u64) -> String {
    match age_minutes {
        0 => "just now".to_owned(),
        1..=59 => format!("{age_minutes} min ago"),
        60..=1439 => format!("{} h ago", age_minutes / 60),
        1440..=2879 => "yesterday".to_owned(),
        _ => format!("{} days ago", age_minutes / 1440),
    }
}

/// The status-bar line: "Last save point 2 h ago", or a nudge when there has never been one.
pub fn status_text(summary: &SaveSummary) -> String {
    match summary.age_minutes {
        Some(minutes) => format!("Last save point {}", last_text(minutes)),
        None => "No save point yet".to_owned(),
    }
}

/// Why the line is amber.
pub fn stale_text(reason: StaleReason) -> &'static str {
    match reason {
        StaleReason::AutoOff => "Automatic backups are off",
        StaleReason::AutoFailing => "Automatic backups haven't run in a day",
        StaleReason::DriveOffline => "Google Drive hasn't saved a checkpoint in a day",
    }
}

/// "2027-03-03 15:30 UTC", the way backups and checkpoints give their time.
pub fn utc_text(at: OffsetDateTime) -> String {
    let at = at.to_offset(UtcOffset::UTC);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02} UTC",
        at.year(),
        at.month() as u8,
        at.day(),
        at.hour(),
        at.minute()
    )
}

/// "14:02" on the person's own clock.
pub fn time_text(at: OffsetDateTime, utc_offset_minutes: i32) -> String {
    let local = at.to_offset(offset_of(utc_offset_minutes));
    format!("{:02}:{:02}", local.hour(), local.minute())
}

fn offset_of(minutes: i32) -> UtcOffset {
    UtcOffset::from_whole_seconds(minutes.clamp(-14 * 60, 14 * 60) * 60).unwrap_or(UtcOffset::UTC)
}

const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/// "Today · Wed 2027-03-03", "Yesterday · Tue 2027-03-02", or "Mon 2027-03-01".
pub fn day_heading(date: Date, today: Option<Date>) -> String {
    let plain = format!(
        "{} {date}",
        WEEKDAYS[date.weekday().number_days_from_monday() as usize]
    );
    match today.map(|t| (t - date).whole_days()) {
        Some(0) => format!("Today · {plain}"),
        Some(1) => format!("Yesterday · {plain}"),
        _ => plain,
    }
}

/// The chip of a save point and its tone: *Named*, *Manual*, *Daily*, *Before …*, *Drive hourly ·
/// Laptop*.
pub fn chip_of(point: &SavePoint) -> (String, Tone) {
    match &point.source {
        SaveSource::Local {
            kind: BackupKind::Manual,
            label: Some(_),
            ..
        } => ("Named".to_owned(), Tone::Accent),
        SaveSource::Local { kind, .. } => (
            crate::components::backup_settings::kind_label(*kind).to_owned(),
            Tone::Neutral,
        ),
        SaveSource::Drive {
            device_name,
            cadence,
            this_device,
            ..
        } => {
            let how = match cadence {
                CheckpointCadence::Hourly => "Drive hourly",
                CheckpointCadence::Daily => "Drive daily",
            };
            let whose = if *this_device {
                format!("{device_name} (this computer)")
            } else {
                device_name.clone()
            };
            (format!("{how} · {whose}"), Tone::Neutral)
        }
    }
}

/// The name of a named save point, as words.
pub fn name_of(point: &SavePoint) -> Option<String> {
    match &point.source {
        SaveSource::Local {
            label: Some(slug), ..
        } => Some(slug.replace('-', " ")),
        _ => None,
    }
}

/// What the toast says after making one: the name, or the time.
pub fn made_text(label: Option<&str>, time: &str) -> String {
    match label.map(str::trim).filter(|l| !l.is_empty()) {
        Some(name) => format!("Save point made: {name}."),
        None => format!("Save point made at {time}."),
    }
}

// ------------------------------------------------------------------ the box (Ctrl/Cmd+S)

/// Whether the *Save point* box is open: one dialog, drawn once at the top of the app.
#[derive(Clone, Copy)]
pub struct SaveBoxState(pub RwSignal<bool>);

impl SaveBoxState {
    pub fn open(&self) {
        self.0.set(true);
    }

    pub fn close(&self) {
        self.0.set(false);
    }
}

/// Mount once, inside the router.
#[component]
pub fn SaveBoxHost() -> impl IntoView {
    let state = expect_context::<SaveBoxState>();
    view! {
        {move || state.0.get().then(|| view! { <SaveBox /> })}
    }
}

#[component]
fn SaveBox() -> impl IntoView {
    let state = expect_context::<SaveBoxState>();
    let toasts = expect_context::<Toasts>();
    let version = expect_context::<DataVersion>();
    let name = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let input = leptos::prelude::NodeRef::<html::Input>::new();
    Effect::new(move |_| {
        if let Some(el) = input.get() {
            let _ = el.focus();
        }
    });
    let make = move || {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        let label = name.get_untracked();
        spawn_local(async move {
            let result = api::backup_now(None, Some(label.clone())).await;
            busy.set(false);
            match result {
                Ok(entry) => {
                    state.close();
                    let time = entry.created.clone();
                    toasts.info(made_text(Some(&label), &time));
                    version.bump();
                }
                Err(e) => toasts.error(&e),
            }
        });
    };
    let on_keydown = move |e: ev::KeyboardEvent| {
        e.stop_propagation();
        if e.key() == "Escape" {
            e.prevent_default();
            state.close();
        }
    };
    view! {
        <div class="fixed inset-0 z-[80] bg-scrim" on:mousedown=move |_| state.close()></div>
        <div role="dialog" aria-label="Make a save point"
             class="fixed inset-x-0 mx-auto top-[14vh] z-[81] w-[28rem] max-w-[94vw] \
                    rounded-sm border border-line bg-panel p-4 text-[13px]"
             on:keydown=on_keydown>
            <h2 class="mb-1 flex items-center gap-2 text-[14px] font-semibold">
                <Icon name="save" size="h-4 w-4" />
                "Make a save point"
            </h2>
            <p class="mb-3 text-muted">
                "A copy of everything as it is now, kept until you delete it. Restore it any time from "
                "Settings → Data & backup."
            </p>
            <label class="mb-1 block text-[11px] text-muted">"Save point name (optional)"</label>
            <input class=format!("{INPUT} mb-3") node_ref=input maxlength="80"
                placeholder="before the Q2 replan"
                prop:value=move || name.get()
                disabled=move || busy.get()
                on:input=move |ev| name.set(event_target_value(&ev))
                on:keydown=move |e: ev::KeyboardEvent| {
                    if e.key() == "Enter" { e.prevent_default(); make(); }
                } />
            <div class="flex items-center justify-end gap-2">
                <span class="text-[11px] text-faint">
                    {move || if busy.get() { "Saving…" } else { "Enter to save" }}
                </span>
                <button type="button" class=BUTTON disabled=move || busy.get()
                        on:click=move |_| state.close()>"Cancel"</button>
                <button type="button" class=BUTTON_PRIMARY disabled=move || busy.get()
                        on:click=move |_| make()>"Make save point"</button>
            </div>
        </div>
    }
}

// ------------------------------------------------------------------ the timeline card

/// Settings → Data & backup → Save points: every restorable copy, grouped by day.
#[component]
pub fn SavePointsCard() -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let offset = local_clock().map_or(0, |c| c.utc_offset_minutes);
    let today = crate::components::task_board::today();
    let timeline = LocalResource::new(move || {
        version.track();
        api::list_save_points(offset)
    });
    let status = LocalResource::new(move || {
        version.track();
        api::get_backup_status()
    });
    let restore = Restore::new();
    let recover = RwSignal::new(None::<CheckpointInfo>);
    let make = expect_context::<SaveBoxState>();
    // Arriving from the status bar (`#save-points`) scrolls here.
    Effect::new(move |_| {
        if timeline.get().is_some() {
            let wanted = leptos::web_sys::window()
                .and_then(|w| w.location().hash().ok())
                .is_some_and(|h| h == "#save-points");
            if wanted {
                if let Some(el) = leptos::web_sys::window()
                    .and_then(|w| w.document())
                    .and_then(|d| d.get_element_by_id("save-points"))
                {
                    el.scroll_into_view();
                }
            }
        }
    });
    let db_encrypted = move || {
        status
            .get()
            .and_then(|s| s.ok())
            .is_some_and(|s| s.database_encrypted)
    };
    let _ = toasts;
    view! {
        <div id="save-points">
            <Card title="Save points"
                  description="Every copy you can go back to: the ones you name, the daily backups, the safety copies Minimap makes before risky steps, and Google Drive's checkpoints. Press Ctrl/Cmd+S anywhere to make one.">
                <div class="flex flex-wrap items-center gap-2">
                    <button class=BUTTON_PRIMARY on:click=move |_| make.open()>"Make a save point…"</button>
                </div>
                {move || match timeline.get() {
                    None => view! { <p class="text-muted">"Loading…"</p> }.into_any(),
                    Some(Err(e)) => view! { <p class="text-danger">{e.message}</p> }.into_any(),
                    Some(Ok(t)) => view! {
                        <Timeline timeline=t offset=offset today=today restore=restore recover=recover
                                  flag_plain=db_encrypted() />
                    }.into_any(),
                }}
                {move || restore.need_key.get().map(|p| view! { <KeyPrompt path=p restore=restore /> })}
                {move || restore.pending.get().map(|p| view! { <ConfirmRestore preview=p restore=restore /> })}
                {move || recover.get().map(|c| view! { <ConfirmRecover checkpoint=c recover=recover /> })}
            </Card>
        </div>
    }
}

#[component]
fn Timeline(
    timeline: SaveTimeline,
    offset: i32,
    today: Option<Date>,
    restore: Restore,
    recover: RwSignal<Option<CheckpointInfo>>,
    flag_plain: bool,
) -> impl IntoView {
    let note = timeline.drive_note.clone();
    let stale = timeline.stale;
    let empty = timeline.days.is_empty();
    let days = timeline
        .days
        .into_iter()
        .map(|day| view! { <Day day=day offset=offset today=today restore=restore recover=recover flag_plain=flag_plain /> })
        .collect_view();
    view! {
        {note.map(|n| view! { <p class="text-[12px] text-warning">{n}</p> })}
        {stale.map(|r| view! { <p class="text-[12px] text-warning">{format!("No save point in the last day. {}.", stale_text(r))}</p> })}
        {empty.then(|| view! {
            <p class="text-muted">"No save points yet. Make one with Ctrl/Cmd+S, or leave automatic backups on."</p>
        })}
        <div class="max-h-96 overflow-y-auto rounded-sm border border-line">{days}</div>
    }
}

#[component]
fn Day(
    day: SaveDay,
    offset: i32,
    today: Option<Date>,
    restore: Restore,
    recover: RwSignal<Option<CheckpointInfo>>,
    flag_plain: bool,
) -> impl IntoView {
    let heading = day_heading(day.date, today);
    let label = heading.clone();
    let rows = day
        .points
        .into_iter()
        .map(|p| view! { <Row point=p offset=offset restore=restore recover=recover flag_plain=flag_plain /> })
        .collect_view();
    view! {
        <div role="group" aria-label=label>
            <div class="border-b border-line bg-canvas px-2 py-1 text-[10px] font-semibold uppercase tracking-wider text-muted">
                {heading}
            </div>
            {rows}
        </div>
    }
}

#[component]
fn Row(
    point: SavePoint,
    offset: i32,
    restore: Restore,
    recover: RwSignal<Option<CheckpointInfo>>,
    flag_plain: bool,
) -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    let (chip, tone) = chip_of(&point);
    let name = name_of(&point);
    let at = time_text(point.at, offset);
    let prune = point.will_be_pruned;
    let (plain, restoring): (bool, Box<dyn Fn()>) = match &point.source {
        SaveSource::Local {
            path, encrypted, ..
        } => {
            let path = path.clone();
            (
                flag_plain && !*encrypted,
                Box::new(move || check(path.clone(), None, restore, toasts)),
            )
        }
        SaveSource::Drive {
            name, device_name, ..
        } => {
            let info = CheckpointInfo {
                name: name.clone(),
                device_name: device_name.clone(),
                created: utc_text(point.at),
                bytes: point.bytes,
            };
            (false, Box::new(move || recover.set(Some(info.clone()))))
        }
    };
    view! {
        <div class="flex items-center gap-2 border-b border-line px-2 py-1 last:border-b-0">
            <span class="w-12 shrink-0 tabular-nums">{at}</span>
            <span class=tone.chip()>{chip}</span>
            {plain.then(|| view! { <span class=Tone::Danger.chip() title="This backup is not encrypted">"unencrypted"</span> })}
            <span class="min-w-0 flex-1 truncate">
                {name.map(|n| view! { <span class="font-medium text-fg">{n}</span> })}
            </span>
            {prune.then(|| view! {
                <span class="shrink-0 text-[11px] text-muted" title="Minimap deletes older automatic copies to keep the newest ones">
                    "will be removed soon"
                </span>
            })}
            <span class="shrink-0 text-[11px] tabular-nums text-muted">{size_text(point.bytes)}</span>
            <button class=BUTTON on:click=move |_| restoring()>"Restore…"</button>
        </div>
    }
}

// ------------------------------------------------------------------ the status bar

/// "Last save point 2 h ago" beside the Drive state; amber, with the reason, when the newest is
/// over a day old. Click it for the timeline. Asks again when data changes and every 5 minutes.
#[component]
pub fn SaveStatus() -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let summary = RwSignal::new(None::<SaveSummary>);
    let tick = RwSignal::new(0u32);
    let loaded = LocalResource::new(move || {
        version.track();
        tick.track();
        api::get_last_save_point()
    });
    Effect::new(move |_| {
        if let Some(Ok(s)) = loaded.get() {
            summary.set(Some(s));
        }
    });
    let handle = set_interval_with_handle(move || tick.update(|t| *t += 1), REFRESH).ok();
    on_cleanup(move || {
        if let Some(h) = handle {
            h.clear();
        }
    });
    view! {
        {move || summary.get().map(|s| {
            let stale = s.stale;
            let text = status_text(&s);
            let title = match stale {
                Some(reason) => format!("{}. Click to see your save points", stale_text(reason)),
                None => "Click to see your save points".to_owned(),
            };
            let class = if stale.is_some() {
                "flex items-center gap-1.5 text-warning hover:text-fg"
            } else {
                "flex items-center gap-1.5 hover:text-fg"
            };
            view! {
                <A href="/settings?tab=data#save-points" attr:class=class attr:title=title>
                    <Icon name="save" size="h-3.5 w-3.5" />
                    <span>{text}</span>
                    {stale.map(|r| view! { <span>{format!("· {}", stale_text(r))}</span> })}
                </A>
            }
        })}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::{date, datetime};

    fn point(source: SaveSource) -> SavePoint {
        SavePoint {
            source,
            at: datetime!(2027-03-03 14:02 UTC),
            bytes: 2048,
            will_be_pruned: false,
        }
    }

    fn local(kind: BackupKind, label: Option<&str>) -> SavePoint {
        point(SaveSource::Local {
            path: "/b/x.db".into(),
            file_name: "x.db".into(),
            kind,
            label: label.map(str::to_owned),
            encrypted: false,
        })
    }

    #[test]
    fn the_age_reads_naturally() {
        assert_eq!(last_text(0), "just now");
        assert_eq!(last_text(12), "12 min ago");
        assert_eq!(last_text(59), "59 min ago");
        assert_eq!(last_text(60), "1 h ago");
        assert_eq!(last_text(120), "2 h ago");
        assert_eq!(last_text(1439), "23 h ago");
        assert_eq!(last_text(1440), "yesterday");
        assert_eq!(last_text(2879), "yesterday");
        assert_eq!(last_text(3 * 1440), "3 days ago");
    }

    #[test]
    fn the_status_line_names_the_age_or_says_there_is_none() {
        let s = SaveSummary {
            last: None,
            age_minutes: Some(120),
            stale: None,
        };
        assert_eq!(status_text(&s), "Last save point 2 h ago");
        assert_eq!(status_text(&SaveSummary::default()), "No save point yet");
    }

    #[test]
    fn every_reason_for_a_stale_line_has_words() {
        for r in [
            StaleReason::AutoOff,
            StaleReason::AutoFailing,
            StaleReason::DriveOffline,
        ] {
            assert!(!stale_text(r).is_empty());
        }
        assert_eq!(
            stale_text(StaleReason::AutoOff),
            "Automatic backups are off"
        );
    }

    #[test]
    fn times_and_days_are_on_the_persons_own_clock() {
        let at = datetime!(2027-03-03 23:30 UTC);
        assert_eq!(time_text(at, 0), "23:30");
        assert_eq!(time_text(at, 120), "01:30");
        assert_eq!(time_text(at, -300), "18:30");
        let d = date!(2027 - 03 - 03);
        assert_eq!(day_heading(d, Some(d)), "Today · Wed 2027-03-03");
        assert_eq!(
            day_heading(date!(2027 - 03 - 02), Some(d)),
            "Yesterday · Tue 2027-03-02"
        );
        assert_eq!(
            day_heading(date!(2027 - 02 - 25), Some(d)),
            "Thu 2027-02-25"
        );
        assert_eq!(day_heading(d, None), "Wed 2027-03-03");
    }

    #[test]
    fn each_kind_of_point_has_a_chip_and_a_named_one_shows_its_name() {
        assert_eq!(
            chip_of(&local(BackupKind::Manual, Some("before-q2-replan"))),
            ("Named".to_owned(), Tone::Accent)
        );
        assert_eq!(
            name_of(&local(BackupKind::Manual, Some("before-q2-replan"))).as_deref(),
            Some("before q2 replan")
        );
        assert_eq!(chip_of(&local(BackupKind::Manual, None)).0, "manual");
        assert_eq!(name_of(&local(BackupKind::Manual, None)), None);
        assert_eq!(chip_of(&local(BackupKind::Auto, None)).0, "daily");
        assert_eq!(
            chip_of(&local(BackupKind::PreRestore, None)).0,
            "before restore"
        );
        let drive = |this_device, cadence| {
            point(SaveSource::Drive {
                name: "d.db.enc".into(),
                device_name: "Laptop".into(),
                this_device,
                cadence,
            })
        };
        assert_eq!(
            chip_of(&drive(true, CheckpointCadence::Hourly)).0,
            "Drive hourly · Laptop (this computer)"
        );
        assert_eq!(
            chip_of(&drive(false, CheckpointCadence::Daily)).0,
            "Drive daily · Laptop"
        );
    }

    #[test]
    fn the_toast_names_the_save_point_or_the_time() {
        assert_eq!(
            made_text(Some("before Q2 replan"), "2027-03-03 14:02 UTC"),
            "Save point made: before Q2 replan."
        );
        assert_eq!(
            made_text(Some("  "), "2027-03-03 14:02 UTC"),
            "Save point made at 2027-03-03 14:02 UTC."
        );
        assert_eq!(made_text(None, "14:02"), "Save point made at 14:02.");
    }
}
