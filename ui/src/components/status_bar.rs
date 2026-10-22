//! The status bar along the bottom: Google Drive, the newest save point, whether the database is
//! encrypted and how many pings are due, all as small icons. Words appear only for what needs
//! you (Drive not connected, offline or failing; a save point gone stale), or always when the
//! person asks for them (Settings -> Appearance). Click Drive or the save point for a small panel
//! with the detail and what to do. Icons never rest on colour alone: every Drive state has its
//! own shape (`icons.rs`).
//!
//! The Drive state itself is `sync_status.rs`; the save points are `save_points.rs`.

use leptos::{html, prelude::*, task::spawn_local, web_sys};
use leptos_router::components::A;
use minimap_types::{SaveSummary, SyncState, SyncStatus};

use crate::{
    api,
    components::{
        form::{BUTTON, BUTTON_PRIMARY},
        page::{Icon, Tone},
        ping::PingsCtx,
        save_points::{stale_text, status_text, use_save_summary, SaveBoxState},
        sync_status::{state_hint, state_tone, SyncCtx},
    },
    settings_tab::Tab,
    state::{DataVersion, Toasts},
};

// ------------------------------------------------------------------ pure rules

/// The icon that stands for a Drive state.
pub fn drive_icon(state: SyncState) -> &'static str {
    match state {
        SyncState::LocalOnly => "laptop",
        SyncState::Saved => "cloud-check",
        SyncState::Saving => "cloud-up",
        SyncState::Syncing => "cloud-down",
        SyncState::Offline => "cloud-off",
        SyncState::Error | SyncState::NeedsAttention => "cloud-alert",
    }
}

/// Whether the icon is busy: saving, or taking in changes from another computer.
pub fn drive_pulses(state: SyncState) -> bool {
    matches!(state, SyncState::Saving | SyncState::Syncing)
}

/// Whether Drive's state needs words next to its icon even when the person asked for icons only:
/// everything that is not "fine" or "working on it".
pub fn drive_needs_words(state: SyncState) -> bool {
    !matches!(
        state,
        SyncState::Saved | SyncState::Saving | SyncState::Syncing
    )
}

/// What a screen reader and the hover say about Drive: the state, then what it means.
pub fn drive_title(status: &SyncStatus) -> String {
    format!("{}. {}", status.summary, state_hint(status))
}

/// How long a save point stays on the ring: the ring is full when it is new and empty after this.
pub const RING_MINUTES: u64 = 48 * 60;

/// How much of the ring around the save icon is left, 0.0 to 1.0. Empty when there is no save
/// point at all.
pub fn ring_fraction(age_minutes: Option<u64>) -> f64 {
    match age_minutes {
        None => 0.0,
        Some(age) => 1.0 - (age.min(RING_MINUTES) as f64 / RING_MINUTES as f64),
    }
}

/// Whether the save point needs words: it is stale, or there has never been one.
pub fn save_needs_words(summary: &SaveSummary) -> bool {
    summary.stale.is_some() || summary.age_minutes.is_none()
}

/// "Last save point 3 days ago · Automatic backups are off".
pub fn save_words(summary: &SaveSummary) -> String {
    match summary.stale {
        Some(reason) => format!("{} · {}", status_text(summary), stale_text(reason)),
        None => status_text(summary),
    }
}

/// The hover and screen-reader text of the save icon.
pub fn save_title(summary: &SaveSummary) -> String {
    format!("{}. Click for details", save_words(summary))
}

pub fn pings_title(due: usize) -> String {
    match due {
        1 => "1 ping is due. Open This week".to_owned(),
        n => format!("{n} pings are due. Open This week"),
    }
}

pub fn media_text(waiting: u32) -> String {
    match waiting {
        1 => "1 attachment is waiting to upload.".to_owned(),
        n => format!("{n} attachments are waiting to upload."),
    }
}

// ------------------------------------------------------------------ icons only, or words too

/// Whether the status bar shows words beside every icon, or only beside the ones that need you.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Words {
    WhenNeeded,
    Always,
}

impl Words {
    pub const KEY: &'static str = "minimap.status_words";

    pub fn as_str(self) -> &'static str {
        match self {
            Words::WhenNeeded => "needed",
            Words::Always => "always",
        }
    }

    /// Anything unknown is the default: icons only.
    pub fn from_str_or_default(word: &str) -> Self {
        match word {
            "always" => Words::Always,
            _ => Words::WhenNeeded,
        }
    }
}

fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window().and_then(|w| w.local_storage().ok().flatten())
}

/// The remembered choice (a convenience of this computer: nothing breaks without it).
#[derive(Clone, Copy)]
pub struct StatusWords(pub RwSignal<Words>);

impl StatusWords {
    pub fn provide() -> Self {
        let saved = local_storage()
            .and_then(|s| s.get_item(Words::KEY).ok().flatten())
            .map_or(Words::WhenNeeded, |w| Words::from_str_or_default(&w));
        let me = Self(RwSignal::new(saved));
        provide_context(me);
        me
    }

    pub fn always(&self) -> bool {
        self.0.get() == Words::Always
    }

    pub fn set(&self, words: Words) {
        self.0.set(words);
        if let Some(storage) = local_storage() {
            let _ = storage.set_item(Words::KEY, words.as_str());
        }
    }
}

// ------------------------------------------------------------------ the bar

/// Which panel is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Panel {
    Drive,
    Save,
}

const ITEM: &str = "flex items-center gap-1.5 rounded-sm hover:text-fg";

#[component]
pub fn StatusBar() -> impl IntoView {
    let ctx = expect_context::<SyncCtx>();
    let words = expect_context::<StatusWords>();
    let version = expect_context::<DataVersion>();
    let open = RwSignal::new(None::<Panel>);
    let save = use_save_summary();

    // Small memos, so the icons redraw when their own part changes and not with every poll.
    let state = Memo::new(move |_| ctx.status.get().map(|s| s.state));
    let has_warning = Memo::new(move |_| ctx.status.get().is_some_and(|s| s.warning.is_some()));

    let encrypted = RwSignal::new(false);
    let info = LocalResource::new(move || {
        version.track();
        api::get_data_info()
    });
    Effect::new(move |_| {
        if let Some(Ok(i)) = info.get() {
            encrypted.set(i.encrypted);
        }
    });
    let pings_due = move || {
        use_context::<PingsCtx>()
            .map(|p| p.view.with(|v| v.due.len()))
            .unwrap_or(0)
    };

    let toggle = move |which: Panel| {
        open.update(|o| *o = if *o == Some(which) { None } else { Some(which) })
    };
    let raised = move |which: Panel| {
        if open.get() == Some(which) {
            "z-50"
        } else {
            ""
        }
    };

    view! {
        <footer class="relative flex h-6 shrink-0 items-center gap-3 border-t border-line bg-panel px-3 text-[11px] text-muted"
                aria-label="Status">
            // A click anywhere else closes the panel.
            {move || open.get().map(|_| view! {
                <button type="button" tabindex="-1" aria-hidden="true"
                        class="fixed inset-0 z-40 cursor-default"
                        on:click=move |_| open.set(None) />
            })}

            // ---- Google Drive
            <div class=move || format!("relative flex items-center {}", raised(Panel::Drive))>
                {move || state.get().map(|st| {
                    let tone = state_tone(st);
                    let busy = drive_pulses(st);
                    let show_words = move || words.always() || drive_needs_words(st);
                    view! {
                        <button type="button"
                                class=format!("{ITEM} {}", tone_text(tone))
                                aria-haspopup="dialog"
                                aria-expanded=move || (open.get() == Some(Panel::Drive)).to_string()
                                aria-label=move || ctx.status.get().map(|s| drive_title(&s)).unwrap_or_default()
                                title=move || ctx.status.get().map(|s| drive_title(&s)).unwrap_or_default()
                                on:click=move |_| toggle(Panel::Drive)>
                            <span class="relative inline-flex">
                                <span class=if busy { "inline-flex motion-safe:animate-pulse" } else { "inline-flex" }>
                                    <Icon name=drive_icon(st) size="h-3.5 w-3.5" />
                                </span>
                                {move || has_warning.get().then(|| view! {
                                    <span class="absolute -right-0.5 -top-0.5 h-1.5 w-1.5 rounded-full bg-warning"
                                          aria-hidden="true"></span>
                                })}
                            </span>
                            {move || show_words().then(|| view! {
                                <span class=tone.chip()>
                                    {move || ctx.status.get().map(|s| s.summary)}
                                </span>
                            })}
                        </button>
                    }
                })}
                // Tells a screen reader when Drive changes state, without a sound or a toast.
                <span class="sr-only" aria-live="polite">
                    {move || ctx.status.get().map(|s| s.summary)}
                </span>
                {move || (open.get() == Some(Panel::Drive)).then(|| view! { <DrivePanel open=open /> })}
            </div>

            // ---- the newest save point, in a ring that empties as it ages
            <div class=move || format!("relative flex items-center {}", raised(Panel::Save))>
                {move || save.get().map(|s| {
                    let stale = s.stale.is_some();
                    let show_words = words.always() || save_needs_words(&s);
                    let title = save_title(&s);
                    let text = save_words(&s);
                    view! {
                        <button type="button"
                                class=format!("{ITEM} {}", if stale { "text-warning" } else { "" })
                                aria-haspopup="dialog"
                                aria-expanded=move || (open.get() == Some(Panel::Save)).to_string()
                                aria-label=title.clone()
                                title=title
                                on:click=move |_| toggle(Panel::Save)>
                            <Ring name="save" fraction=ring_fraction(s.age_minutes) />
                            {show_words.then(|| view! { <span>{text}</span> })}
                        </button>
                    }
                })}
                {move || (open.get() == Some(Panel::Save)).then(|| view! { <SavePanel open=open save=save /> })}
            </div>

            // ---- the database is encrypted
            {move || encrypted.get().then(|| view! {
                <A href=Tab::Security.path() attr:class=ITEM
                   attr:title="Your database is encrypted. Open Security settings"
                   attr:aria-label="Your database is encrypted. Open Security settings">
                    <Icon name="lock" size="h-3.5 w-3.5" />
                    {move || words.always().then(|| view! { <span>"Encrypted"</span> })}
                </A>
            })}

            // ---- pings that have come due
            {move || {
                let due = pings_due();
                (due > 0).then(|| view! {
                    <A href="/" attr:class=format!("{ITEM} text-accent")
                       attr:title=pings_title(due) attr:aria-label=pings_title(due)>
                        <Icon name="ping" size="h-3.5 w-3.5" />
                        <span>{due}</span>
                    </A>
                })
            }}

            // ---- the words icons-only leaves out, when asked for
            {move || words.always().then(|| view! {
                <span class="contents">
                    {move || ctx.status.get().map(|s| {
                        let local_only = s.state == SyncState::LocalOnly;
                        let warning = s.warning.clone();
                        view! {
                            {local_only.then(|| view! {
                                <span class="hidden sm:inline">
                                    "Your data is stored on this device only. "
                                    <A href=Tab::Data.path() attr:class="text-accent hover:underline">"Connect Google Drive"</A>
                                </span>
                            })}
                            {warning.map(|w| {
                                let tip = w.clone();
                                view! { <span class="truncate text-warning" title=tip>{w}</span> }
                            })}
                        }
                    })}
                    <span class="ml-auto shrink-0">
                        {move || ctx.status.get().map(|s| s.device_name)}
                    </span>
                </span>
            })}
        </footer>
    }
}

/// The text colour of a tone, for an icon: neutral stays muted like the rest of the bar.
fn tone_text(tone: Tone) -> &'static str {
    match tone {
        Tone::Neutral => "",
        t => t.text(),
    }
}

/// An icon inside a ring that is full when new and empties as the age grows (`ring_fraction`).
#[component]
fn Ring(name: &'static str, fraction: f64) -> impl IntoView {
    const R: f64 = 9.0;
    let circumference = 2.0 * std::f64::consts::PI * R;
    let paths = crate::icons::paths(name).unwrap_or(&[]);
    view! {
        <svg class="h-[18px] w-[18px] shrink-0" viewBox="-2 -2 20 20" fill="none"
             stroke="currentColor" stroke-width="1.4" stroke-linecap="round"
             stroke-linejoin="round" aria-hidden="true">
            <circle cx="8" cy="8" r=R stroke-width="1" opacity="0.25" />
            <circle cx="8" cy="8" r=R stroke-width="1.6" stroke-linecap="butt"
                    transform="rotate(-90 8 8)"
                    stroke-dasharray=format!("{:.2} {:.2}", fraction * circumference, circumference) />
            {paths.iter().map(|d| view! { <path d=*d /> }).collect_view()}
        </svg>
    }
}

// ------------------------------------------------------------------ the panels

/// A small dialog above the bar. Esc closes it; focus moves into it when it opens.
#[component]
fn Popover(
    label: &'static str,
    open: RwSignal<Option<Panel>>,
    children: Children,
) -> impl IntoView {
    let el = NodeRef::<html::Div>::new();
    Effect::new(move |_| {
        if let Some(e) = el.get() {
            let _ = e.focus();
        }
    });
    view! {
        <div node_ref=el tabindex="-1" role="dialog" aria-label=label
             class="absolute bottom-full left-0 z-50 mb-1 w-72 space-y-2 rounded-sm border border-line-strong bg-panel p-3 text-[12px] text-fg shadow-lg outline-none"
             on:keydown=move |ev| {
                 if ev.key() == "Escape" {
                     ev.stop_propagation();
                     open.set(None);
                 }
             }>
            {children()}
        </div>
    }
}

#[component]
fn DrivePanel(open: RwSignal<Option<Panel>>) -> impl IntoView {
    let ctx = expect_context::<SyncCtx>();
    let toasts = expect_context::<Toasts>();
    let version = expect_context::<DataVersion>();
    let syncing = RwSignal::new(false);
    let connected = Memo::new(move |_| {
        ctx.status
            .get()
            .is_some_and(|s| s.state != SyncState::LocalOnly)
    });
    let sync = move |_| {
        syncing.set(true);
        spawn_local(async move {
            if let Err(e) = api::sync_now().await {
                toasts.error(&e);
            }
            syncing.set(false);
            ctx.refresh.run(());
            version.bump();
        });
    };
    let close = move |_| open.set(None);
    view! {
        <Popover label="Google Drive" open=open>
            <div class="flex items-center gap-2">
                {move || ctx.status.get().map(|s| view! {
                    <Icon name=drive_icon(s.state) size="h-4 w-4" />
                    <span class=state_tone(s.state).chip()>{s.summary}</span>
                })}
            </div>
            <p class="text-muted">
                {move || ctx.status.get().map(|s| state_hint(&s))}
            </p>
            {move || ctx.status.get().and_then(|s| s.warning).map(|w| view! { <p class="text-warning">{w}</p> })}
            {move || ctx.status.get().filter(|s| s.media_waiting > 0).map(|s| view! {
                <p class="text-muted">{media_text(s.media_waiting)}</p>
            })}
            <p class="text-[11px] text-muted">
                "This computer: "
                {move || ctx.status.get().map(|s| s.device_name)}
            </p>
            <div class="flex flex-wrap items-center gap-2">
                {move || if connected.get() {
                    view! {
                        <button type="button" class=BUTTON on:click=sync disabled=move || syncing.get()>
                            {move || if syncing.get() { "Syncing…" } else { "Sync now" }}
                        </button>
                        <A href=Tab::Data.path() attr:class=BUTTON on:click=close>"Google Drive settings"</A>
                    }.into_any()
                } else {
                    view! {
                        <A href=Tab::Data.path() attr:class=BUTTON_PRIMARY on:click=close>"Connect Google Drive"</A>
                    }.into_any()
                }}
            </div>
        </Popover>
    }
}

#[component]
fn SavePanel(open: RwSignal<Option<Panel>>, save: RwSignal<Option<SaveSummary>>) -> impl IntoView {
    let make = expect_context::<SaveBoxState>();
    let close = move |_| open.set(None);
    view! {
        <Popover label="Save points" open=open>
            {move || save.get().map(|s| view! {
                <p>{status_text(&s)}</p>
                {s.stale.map(|r| view! { <p class="text-warning">{stale_text(r)}</p> })}
            })}
            <p class="text-muted">
                "A save point is a copy you can go back to. Press Ctrl/Cmd+S to make one with a name."
            </p>
            <div class="flex flex-wrap items-center gap-2">
                <button type="button" class=BUTTON_PRIMARY
                        on:click=move |_| { open.set(None); make.open(); }>
                    "Make a save point…"
                </button>
                <A href="/settings?tab=data#save-points" attr:class=BUTTON on:click=close>"See save points"</A>
            </div>
        </Popover>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::StaleReason;

    const ALL: [SyncState; 7] = [
        SyncState::LocalOnly,
        SyncState::Saved,
        SyncState::Saving,
        SyncState::Syncing,
        SyncState::Offline,
        SyncState::Error,
        SyncState::NeedsAttention,
    ];

    #[test]
    fn every_drive_state_has_an_icon_that_exists() {
        for s in ALL {
            assert!(crate::icons::paths(drive_icon(s)).is_some(), "{s:?}");
        }
    }

    #[test]
    fn states_are_told_apart_by_shape_not_only_colour() {
        // Error and "needs you" share a shape (both say: look here); every other pair differs.
        let mut names: Vec<_> = ALL.iter().map(|s| drive_icon(*s)).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 6);
        assert_eq!(
            drive_icon(SyncState::Error),
            drive_icon(SyncState::NeedsAttention)
        );
    }

    #[test]
    fn only_working_states_pulse_and_only_trouble_speaks() {
        for s in ALL {
            let working = matches!(s, SyncState::Saving | SyncState::Syncing);
            assert_eq!(drive_pulses(s), working);
            assert_eq!(
                drive_needs_words(s),
                !working && s != SyncState::Saved,
                "{s:?}"
            );
        }
        assert!(drive_needs_words(SyncState::LocalOnly));
        assert!(drive_needs_words(SyncState::Offline));
        assert!(!drive_needs_words(SyncState::Saved));
    }

    #[test]
    fn the_ring_empties_with_age_and_stays_in_range() {
        assert_eq!(ring_fraction(None), 0.0);
        assert_eq!(ring_fraction(Some(0)), 1.0);
        assert_eq!(ring_fraction(Some(RING_MINUTES / 2)), 0.5);
        assert_eq!(ring_fraction(Some(RING_MINUTES)), 0.0);
        assert_eq!(ring_fraction(Some(RING_MINUTES * 10)), 0.0);
        assert!(ring_fraction(Some(60)) > ring_fraction(Some(600)));
    }

    #[test]
    fn a_save_point_speaks_when_stale_or_missing() {
        let fresh = SaveSummary {
            age_minutes: Some(120),
            ..Default::default()
        };
        assert!(!save_needs_words(&fresh));
        assert_eq!(save_words(&fresh), "Last save point 2 h ago");
        let stale = SaveSummary {
            age_minutes: Some(3 * 1440),
            stale: Some(StaleReason::AutoOff),
            ..Default::default()
        };
        assert!(save_needs_words(&stale));
        assert_eq!(
            save_words(&stale),
            "Last save point 3 days ago · Automatic backups are off"
        );
        let never = SaveSummary::default();
        assert!(save_needs_words(&never));
        assert_eq!(save_words(&never), "No save point yet");
    }

    #[test]
    fn words_round_trip_and_default_to_icons_only() {
        for w in [Words::WhenNeeded, Words::Always] {
            assert_eq!(Words::from_str_or_default(w.as_str()), w);
        }
        assert_eq!(Words::from_str_or_default("nonsense"), Words::WhenNeeded);
    }

    #[test]
    fn counts_are_worded_in_the_singular_and_plural() {
        assert_eq!(pings_title(1), "1 ping is due. Open This week");
        assert_eq!(pings_title(3), "3 pings are due. Open This week");
        assert_eq!(media_text(1), "1 attachment is waiting to upload.");
        assert_eq!(media_text(2), "2 attachments are waiting to upload.");
    }
}
