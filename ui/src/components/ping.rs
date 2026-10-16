//! Pings (docs/product-design/05-pings.md, ADR-0020): a marker on any item with a day to come
//! back to it. The rules are `minimap_core::pings`; the screen looks marks up by id from the one
//! `PingsView` the shell loads, asks for a ping in a small box (`m`), and says what a ping means.
//!
//! A ping is private bookkeeping: it never changes the item. The glyph (a map pin) carries the
//! meaning; colour is second (accent when due, muted when still to come).

use leptos::{ev, html, prelude::*, task::spawn_local};
use minimap_types::{AppError, Date, NodeRef, NodeSummary, Ping, PingsView, Uuid};

use crate::{
    api,
    components::{
        date_field::DateField,
        form::{BUTTON, BUTTON_ON, BUTTON_PRIMARY, INPUT},
        page::Icon,
    },
    state::{finish, DataVersion, Toasts},
    timeline::day_text,
};

const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

// ------------------------------------------------------------------ pure wording

/// The days the box offers as buttons, as `(label, when)`: `when` is what `ping_item` accepts.
/// *Friday* is left out on Friday, Saturday and Sunday, where *Next Mon* takes its place.
/// `weekday` counts from Monday = 0.
pub fn quick_days(weekday: u8) -> Vec<(&'static str, &'static str)> {
    let mut days = vec![("Today", "today"), ("Tomorrow", "tomorrow")];
    if weekday < 4 {
        days.push(("Friday", "fri"));
    } else {
        days.push(("Next Mon", "next-mon"));
    }
    days.push(("Next week", "next-week"));
    days
}

/// The hover text of a mark: "Pinged for Fri 2027-03-05: check the vendor contract".
pub fn mark_title(ping: &Ping) -> String {
    let when = day_text(ping.on_date);
    if ping.note.is_empty() {
        format!("Pinged for {when}")
    } else {
        format!("Pinged for {when}: {}", ping.note)
    }
}

/// When a ping on This week was made and how long it has waited: "pinged Mon", or "pinged Mon ·
/// due 3 days ago" once its day has passed. A ping from more than six days ago names its date.
pub fn ping_age_text(ping: &Ping, today: Date) -> String {
    let made = ping.created_at.date();
    let age = (today - made).whole_days();
    let when = if (0..7).contains(&age) {
        WEEKDAYS[made.weekday().number_days_from_monday() as usize].to_owned()
    } else {
        made.to_string()
    };
    let late = (today - ping.on_date).whole_days();
    match late {
        n if n <= 0 => format!("pinged {when}"),
        1 => format!("pinged {when} · due yesterday"),
        n => format!("pinged {when} · due {n} days ago"),
    }
}

/// "Pinged for Fri 2027-03-05. Ctrl/Cmd+Z undoes it." after saving.
pub fn saved_message(ping: &Ping) -> String {
    format!(
        "Pinged for {}. Press Ctrl/Cmd+Z to undo.",
        day_text(ping.on_date)
    )
}

// ------------------------------------------------------------------ the shell's view

/// Every live ping of an active item. Provided once by the shell; it reloads when data changes
/// and keeps the last answer meanwhile, so marks never blink.
#[derive(Clone, Copy)]
pub struct PingsCtx {
    pub view: RwSignal<PingsView>,
}

impl PingsCtx {
    pub fn provide() -> Self {
        let version = expect_context::<DataVersion>();
        let view = RwSignal::new(PingsView::default());
        let loaded = LocalResource::new(move || {
            version.track();
            api::list_pings()
        });
        Effect::new(move |_| {
            if let Some(Ok(v)) = loaded.get() {
                view.set(v);
            }
        });
        let ctx = Self { view };
        provide_context(ctx);
        ctx
    }

    /// The live ping on an item and whether its day has come (tracks, so call it inside a
    /// closure).
    pub fn mark(&self, id: Uuid) -> Option<(Ping, bool)> {
        self.view.with(|v| {
            v.due
                .iter()
                .map(|r| (r, true))
                .chain(v.upcoming.iter().map(|r| (r, false)))
                .find(|(r, _)| r.ping.target.id == id)
                .map(|(r, due)| (r.ping.clone(), due))
        })
    }
}

/// The ping to show on `id`, if it has a live one.
pub fn mark_for(id: Uuid) -> Option<(Ping, bool)> {
    use_context::<PingsCtx>().and_then(|ctx| ctx.mark(id))
}

/// The pin itself.
#[component]
pub fn PingMark(ping: Ping, due: bool) -> impl IntoView {
    let tone = if due { "text-accent" } else { "text-muted" };
    view! {
        <span class=format!("inline-flex shrink-0 items-center {tone}") title=mark_title(&ping)
              role="img" aria-label=mark_title(&ping)>
            <Icon name="ping" size="h-3.5 w-3.5" />
        </span>
    }
}

/// The pin on an item, when it has a live ping (nothing otherwise).
#[component]
pub fn PingMarkFor(id: Uuid) -> impl IntoView {
    view! {
        {move || mark_for(id).map(|(ping, due)| view! { <PingMark ping=ping due=due /> })}
    }
}

// ------------------------------------------------------------------ the box

/// The item the Ping box is open for, if any: one at a time, drawn once at the top of the app.
/// `show` is the palette's *Show pings*: This week scrolls to its Pings card when it sees it.
#[derive(Clone, Copy)]
pub struct PingBoxState {
    pub node: RwSignal<Option<NodeRef>>,
    pub show: RwSignal<bool>,
}

impl PingBoxState {
    pub fn new() -> Self {
        Self {
            node: RwSignal::new(None),
            show: RwSignal::new(false),
        }
    }

    pub fn open(&self, node: NodeRef) {
        self.node.set(Some(node));
    }

    pub fn close(&self) {
        self.node.set(None);
    }
}

impl Default for PingBoxState {
    fn default() -> Self {
        Self::new()
    }
}

/// Mount once, inside the router.
#[component]
pub fn PingBoxHost() -> impl IntoView {
    let state = expect_context::<PingBoxState>();
    view! {
        {move || state.node.get().map(|node| view! { <PingBox node=node /> })}
    }
}

#[component]
fn PingBox(node: NodeRef) -> impl IntoView {
    let state = expect_context::<PingBoxState>();
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let summary = LocalResource::new(move || api::get_node_summary(node));
    let existing = LocalResource::new(move || api::get_ping_for(node.id));
    let note = RwSignal::new(String::new());
    let when = RwSignal::new("today".to_owned());
    let custom = RwSignal::new(false);
    let busy = RwSignal::new(false);
    let input = leptos::prelude::NodeRef::<html::Input>::new();
    // A ping the item already has is the starting point; typing is never overwritten after that.
    let filled = RwSignal::new(false);
    Effect::new(move |_| {
        if filled.get_untracked() {
            return;
        }
        if let Some(found) = existing.get() {
            if let Ok(Some(p)) = found {
                note.set(p.note);
                when.set(p.on_date.to_string());
                custom.set(true);
            }
            filled.set(true);
            if let Some(el) = input.get() {
                let _ = el.focus();
            }
        }
    });
    let label = move || match summary.get() {
        Some(Ok(NodeSummary { label, .. })) => label,
        _ => String::new(),
    };
    let weekday =
        crate::components::task_board::today().map_or(0, |d| d.weekday().number_days_from_monday());
    let buttons = quick_days(weekday)
        .into_iter()
        .map(|(text, w)| {
            view! {
                <button type="button"
                    class=move || format!("{BUTTON} {}", if !custom.get() && when.get() == w { BUTTON_ON } else { "" })
                    on:click=move |_| { custom.set(false); when.set(w.to_owned()); }>
                    {text}
                </button>
            }
        })
        .collect_view();
    let save = move || {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        let (text, on) = (note.get_untracked(), when.get_untracked());
        spawn_local(async move {
            let result = api::ping_item(node, text, on).await;
            busy.set(false);
            let saved = result.as_ref().ok().map(|r| r.ping.clone());
            if finish(result, toasts, version).is_some() {
                state.close();
                if let Some(p) = saved {
                    toasts.info(saved_message(&p));
                }
            }
        });
    };
    let clear = move |_| {
        let Some(Ok(Some(p))) = existing.get_untracked() else {
            return;
        };
        spawn_local(async move {
            let result: Result<(), AppError> = api::clear_ping(p.id).await;
            if finish(result, toasts, version).is_some() {
                state.close();
                toasts.info("Ping cleared. Press Ctrl/Cmd+Z to undo.".to_owned());
            }
        });
    };
    let has_ping = move || matches!(existing.get(), Some(Ok(Some(_))));
    let on_keydown = move |e: ev::KeyboardEvent| {
        e.stop_propagation();
        if e.key() == "Escape" {
            e.prevent_default();
            state.close();
        }
    };
    view! {
        <div class="fixed inset-0 z-[80] bg-scrim" on:mousedown=move |_| state.close()></div>
        <div role="dialog" aria-label="Ping"
             class="fixed inset-x-0 mx-auto top-[14vh] z-[81] w-[28rem] max-w-[94vw] \
                    rounded-sm border border-line bg-panel p-4 text-[13px]"
             on:keydown=on_keydown>
            <h2 class="mb-1 flex items-center gap-2 text-[14px] font-semibold">
                <Icon name="ping" size="h-4 w-4" />
                <span class="min-w-0 truncate">"Ping " {label}</span>
            </h2>
            <p class="mb-3 text-muted">
                "Come back to it on a day. It shows on This week then, and changes nothing about it."
            </p>
            <label class="mb-1 block text-[11px] text-muted">"When"</label>
            <div class="mb-3 flex flex-wrap items-center gap-1.5">
                {buttons}
                <button type="button"
                    class=move || format!("{BUTTON} {}", if custom.get() { BUTTON_ON } else { "" })
                    on:click=move |_| custom.set(true)>"Pick a date…"</button>
                {move || custom.get().then(|| {
                    let start = when.get_untracked();
                    let start = if crate::calendar::parse_ymd(&start).is_some() { start } else { String::new() };
                    view! {
                        <DateField current=start compact=true placeholder="YYYY-MM-DD"
                            on_commit=Callback::new(move |text: String| {
                                if !text.trim().is_empty() { when.set(text); }
                            }) />
                    }
                })}
            </div>
            <label class="mb-1 block text-[11px] text-muted">"Note (optional)"</label>
            <input class=format!("{INPUT} mb-3") node_ref=input maxlength="200"
                placeholder="check the vendor contract"
                prop:value=move || note.get()
                on:input=move |ev| note.set(event_target_value(&ev))
                on:keydown=move |e: ev::KeyboardEvent| {
                    if e.key() == "Enter" { e.prevent_default(); save(); }
                } />
            <div class="flex items-center justify-end gap-2">
                {move || has_ping().then(|| view! {
                    <button type="button" class=format!("{BUTTON} mr-auto") on:click=clear>"Clear ping"</button>
                })}
                <span class="text-[11px] text-faint">"Enter to save"</span>
                <button type="button" class=BUTTON on:click=move |_| state.close()>"Cancel"</button>
                <button type="button" class=BUTTON_PRIMARY disabled=move || busy.get()
                    on:click=move |_| save()>"Ping"</button>
            </div>
        </div>
    }
}

/// The *Ping…* button in a pane's header: opens the box for the item.
#[component]
pub fn PingButton(node: NodeRef) -> impl IntoView {
    let state = expect_context::<PingBoxState>();
    view! {
        <button class="flex items-center gap-1 rounded px-1.5 py-0.5 text-[12px] text-muted hover:bg-hover hover:text-fg"
                title="Come back to this on a day (m)" aria-label="Ping this"
                on:click=move |_| state.open(node)>
            <Icon name="ping" size="h-3.5 w-3.5" />
            <span>"Ping…"</span>
        </button>
    }
}

/// The ping an item has, as a strip under its pane's header, with *Clear* and *Change…*. Nothing
/// when the item has no live ping.
#[component]
pub fn PanePing(node: NodeRef) -> impl IntoView {
    let state = expect_context::<PingBoxState>();
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let found = LocalResource::new(move || {
        version.track();
        api::get_ping_for(node.id)
    });
    let today = crate::components::task_board::today();
    view! {
        {move || match found.get() {
            Some(Ok(Some(ping))) => {
                let due = today.is_some_and(|t| ping.on_date <= t);
                let id = ping.id;
                let tone = if due { "text-accent" } else { "text-muted" };
                let clear = move |_| {
                    spawn_local(async move {
                        let result = api::clear_ping(id).await;
                        if finish(result, toasts, version).is_some() {
                            toasts.info("Ping cleared. Press Ctrl/Cmd+Z to undo.".to_owned());
                        }
                    });
                };
                view! {
                    <div class="flex items-center gap-2 border-b border-line bg-accent/5 px-4 py-1.5 text-[12px]">
                        <span class=format!("flex shrink-0 items-center gap-1 font-medium {tone}")>
                            <Icon name="ping" size="h-3.5 w-3.5" />
                            {if due { "Ping due" } else { "Ping" }}
                        </span>
                        <span class="min-w-0 flex-1 truncate" title=mark_title(&ping)>
                            {pane_text(&ping)}
                        </span>
                        <button class=BUTTON on:click=clear>"Clear"</button>
                        <button class=BUTTON on:click=move |_| state.open(node)>"Change…"</button>
                    </div>
                }
                .into_any()
            }
            _ => ().into_any(),
        }}
    }
}

/// "for Fri 2027-03-05: check the vendor contract", the pane strip's words.
pub fn pane_text(ping: &Ping) -> String {
    let when = day_text(ping.on_date);
    if ping.note.is_empty() {
        format!("for {when}")
    } else {
        format!("for {when}: {}", ping.note)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{NodeType, PingRow};
    use time::{macros::date, Duration, OffsetDateTime};

    fn ping(on: Date, made: Date, note: &str) -> Ping {
        let at = OffsetDateTime::new_utc(made, time::Time::MIDNIGHT);
        Ping {
            id: Uuid::from_u128(1),
            target: NodeRef::new(NodeType::Project, Uuid::from_u128(2)),
            note: note.into(),
            on_date: on,
            cleared_on: None,
            created_at: at,
            updated_at: at,
            archived_at: None,
        }
    }

    #[test]
    fn friday_is_offered_until_thursday_then_next_monday() {
        for weekday in 0..4 {
            let days = quick_days(weekday);
            assert_eq!(
                days,
                [
                    ("Today", "today"),
                    ("Tomorrow", "tomorrow"),
                    ("Friday", "fri"),
                    ("Next week", "next-week")
                ],
                "{weekday}"
            );
        }
        for weekday in 4..7 {
            let days = quick_days(weekday);
            assert!(days.contains(&("Next Mon", "next-mon")), "{weekday}");
            assert!(!days.iter().any(|(l, _)| *l == "Friday"), "{weekday}");
        }
    }

    #[test]
    fn a_mark_says_the_day_and_the_note() {
        let p = ping(
            date!(2027 - 03 - 05),
            date!(2027 - 03 - 01),
            "check the vendor contract",
        );
        assert_eq!(
            mark_title(&p),
            "Pinged for Fri 2027-03-05: check the vendor contract"
        );
        let bare = ping(date!(2027 - 03 - 05), date!(2027 - 03 - 01), "");
        assert_eq!(mark_title(&bare), "Pinged for Fri 2027-03-05");
    }

    #[test]
    fn the_age_names_the_weekday_it_was_made_and_how_late_it_is() {
        let today = date!(2027 - 03 - 03);
        let monday = date!(2027 - 03 - 01);
        assert_eq!(ping_age_text(&ping(today, monday, ""), today), "pinged Mon");
        assert_eq!(
            ping_age_text(&ping(today - Duration::days(1), monday, ""), today),
            "pinged Mon · due yesterday"
        );
        assert_eq!(
            ping_age_text(&ping(today - Duration::days(3), monday, ""), today),
            "pinged Mon · due 3 days ago"
        );
        // Old ones name the date; one for the future is just "pinged".
        let old = date!(2027 - 02 - 10);
        assert_eq!(
            ping_age_text(&ping(today, old, ""), today),
            "pinged 2027-02-10"
        );
        assert_eq!(
            ping_age_text(&ping(today + Duration::days(2), monday, ""), today),
            "pinged Mon"
        );
    }

    #[test]
    fn the_pane_strip_says_the_day_and_the_note() {
        let p = ping(date!(2027 - 03 - 05), date!(2027 - 03 - 01), "check it");
        assert_eq!(pane_text(&p), "for Fri 2027-03-05: check it");
        let bare = ping(date!(2027 - 03 - 05), date!(2027 - 03 - 01), "");
        assert_eq!(pane_text(&bare), "for Fri 2027-03-05");
    }

    #[test]
    fn the_saved_message_names_the_day_and_undo() {
        let p = ping(date!(2027 - 03 - 05), date!(2027 - 03 - 01), "");
        assert_eq!(
            saved_message(&p),
            "Pinged for Fri 2027-03-05. Press Ctrl/Cmd+Z to undo."
        );
    }

    #[test]
    fn a_lookup_finds_due_and_upcoming_pings_by_item() {
        let due = ping(date!(2027 - 03 - 03), date!(2027 - 03 - 01), "a");
        let mut later = ping(date!(2027 - 03 - 09), date!(2027 - 03 - 01), "b");
        later.id = Uuid::from_u128(5);
        later.target = NodeRef::new(NodeType::Person, Uuid::from_u128(6));
        let row = |p: &Ping| PingRow {
            ping: p.clone(),
            target: NodeSummary {
                node: p.target,
                label: "x".into(),
                archived: false,
            },
        };
        let owner = Owner::new();
        owner.with(|| {
            let ctx = PingsCtx {
                view: RwSignal::new(PingsView {
                    due: vec![row(&due)],
                    upcoming: vec![row(&later)],
                }),
            };
            assert_eq!(ctx.mark(Uuid::from_u128(2)).map(|(_, d)| d), Some(true));
            assert_eq!(ctx.mark(Uuid::from_u128(6)).map(|(_, d)| d), Some(false));
            assert!(ctx.mark(Uuid::from_u128(99)).is_none());
        });
    }
}
