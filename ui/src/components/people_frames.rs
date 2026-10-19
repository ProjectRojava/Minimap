//! Your people (docs/product-design/06-your-people.md, spec 49): a compact frame for you and each
//! of your direct reports, on This week and People. The rules are `minimap_core::people_frames`;
//! the screen words them.
//!
//! This is about support, never performance. A frame shows load and neglect (yours, of them):
//! no scores, no ranking, no verdict on a person. The words here are checked by a test.

use std::collections::HashMap;

use leptos::prelude::*;
use leptos_router::hooks::use_navigate;
use minimap_types::{Frame, FrameFlag, FramesHint, NodeRef, PeopleFrames, Uuid};

use crate::{
    components::{avatar::PersonAvatar, form::BUTTON, page::Tone, people_panel::new_one_on_one},
    state::{DataVersion, ListNav, Selection, Toasts},
};

/// Pills on a frame before "+N".
pub const MAX_PILLS: usize = 3;

// ------------------------------------------------------------------ pure wording

/// "72% this week", or "no capacity set" when there is no capacity to measure it against.
pub fn load_text(frame: &Frame) -> String {
    match frame.load_pct {
        Some(pct) => format!("{pct}% this week"),
        None => "no capacity set".to_owned(),
    }
}

/// How the load bar is filled, as percentages of the track: the work, then (above 100%) the part
/// over capacity. Up to 100% the track fills from the left; above it the bar is full and an end
/// segment shows the excess in proportion (220% is 45% work, 55% over).
pub fn bar_fill(load_pct: Option<u32>) -> (u32, u32) {
    match load_pct {
        None | Some(0) => (0, 0),
        Some(l) if l <= 100 => (l, 0),
        Some(l) => {
            let work = (100.0 * 100.0 / f64::from(l)).round() as u32;
            (work, 100 - work)
        }
    }
}

/// One pill on a frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Pill {
    pub text: String,
    pub tone: Tone,
}

fn plural(n: u32, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The words of a flag. They are facts about the week, not judgements.
pub fn flag_text(flag: FrameFlag, frame: &Frame) -> Pill {
    match flag {
        FrameFlag::Over => Pill {
            text: "Over capacity".to_owned(),
            tone: Tone::Warning,
        },
        FrameFlag::ManyTasks => Pill {
            text: plural(frame.open_tasks, "open task", "open tasks"),
            tone: Tone::Warning,
        },
        FrameFlag::OneOnOneDue { days } => Pill {
            text: format!("1:1 due · {}", plural(days, "day", "days")),
            tone: Tone::Warning,
        },
        FrameFlag::WaitingStale { count } => Pill {
            text: format!(
                "Waiting on them · {} ({count} stale)",
                frame.waiting_on_them
            ),
            tone: Tone::Warning,
        },
        FrameFlag::Blocked { count } => Pill {
            text: format!("{count} blocked"),
            tone: Tone::Warning,
        },
        // Danger is the colour tasks use when they are late; this is about the tasks.
        FrameFlag::Overdue { count } => Pill {
            text: format!("{count} overdue"),
            tone: Tone::Danger,
        },
    }
}

/// The pills of a frame: its flags in their fixed order, then (when nothing waiting is stale) a
/// plain count of what you wait on them for. At most [`MAX_PILLS`], and how many more there are.
pub fn pills(frame: &Frame) -> (Vec<Pill>, usize) {
    let mut all: Vec<Pill> = frame.flags.iter().map(|f| flag_text(*f, frame)).collect();
    if frame.waiting_on_them > 0 && frame.waiting_stale == 0 {
        all.push(Pill {
            text: format!("Waiting on them · {}", frame.waiting_on_them),
            tone: Tone::Neutral,
        });
    }
    let more = all.len().saturating_sub(MAX_PILLS);
    all.truncate(MAX_PILLS);
    (all, more)
}

/// What the card says about someone.
pub fn name_text(frame: &Frame) -> String {
    if frame.is_self {
        "You".to_owned()
    } else {
        frame.person.label.clone()
    }
}

/// The nodes of a set of frames in the order they are drawn: you, your reports, then (with the
/// setting) the people under them. The People screen's cursor walks these first.
pub fn nodes_of(frames: &PeopleFrames) -> Vec<NodeRef> {
    std::iter::once(&frames.me)
        .chain(frames.reports.iter())
        .chain(frames.below.iter().flat_map(|s| s.frames.iter()))
        .map(|f| f.person.node)
        .collect()
}

/// The sentence under the frames when nobody reports to you: what to do, in words that fit where
/// it is read (on the People screen itself there is nowhere else to go).
pub fn hint_text(hint: FramesHint, on_people_screen: bool) -> &'static str {
    match (hint, on_people_screen) {
        (FramesHint::NoReports, true) => {
            "Nobody reports to you yet. Open a person in the list below and set their manager to \
             you (Organization → Manager): they appear here."
        }
        (FramesHint::NoReports, false) => {
            "Nobody reports to you yet. On the People screen, open a person and set their manager \
             to you: they appear here."
        }
    }
}

/// The summary a screen reader gets for a frame.
fn summary(frame: &Frame) -> String {
    let (pills, more) = pills(frame);
    let mut parts = vec![name_text(frame), load_text(frame)];
    parts.extend(pills.into_iter().map(|p| p.text));
    if more > 0 {
        parts.push(format!("and {more} more"));
    }
    parts.join(", ")
}

// ------------------------------------------------------------------ the frames

/// One person's frame: who, how loaded this week, and the facts worth a glance. Click it to open
/// the person; the buttons (shown when you point at it or tab into it) go on from there.
#[component]
pub fn FrameCard(
    frame: Frame,
    /// Its place in the screen's `j`/`k` list, when it has one.
    #[prop(default = None)]
    index: Option<usize>,
) -> impl IntoView {
    let selection = expect_context::<Selection>();
    let list = expect_context::<ListNav>();
    let toasts = expect_context::<Toasts>();
    let version = expect_context::<DataVersion>();
    let navigate = use_navigate();
    let node = frame.person.node;
    let id = node.id;
    let name = frame.person.label.clone();
    let is_self = frame.is_self;
    let (work, excess) = bar_fill(frame.load_pct);
    let (shown, more) = pills(&frame);
    let label = summary(&frame);
    let load = load_text(&frame);
    let on_cursor = move || index.is_some() && list.cursor.get() == index;
    let is_open = move || selection.0.get() == Some(node);
    let open = move || {
        if index.is_some() {
            list.cursor.set(index);
        }
        selection.open(node);
    };
    let tasks_path = format!("/tasks?person={id}");
    let capacity_path = format!("/capacity?person={id}");
    let (go_tasks, go_capacity) = (navigate.clone(), navigate);
    view! {
        <div role="group" tabindex="0" aria-label=label
             class=move || format!(
                 "group relative w-[13.5rem] shrink-0 cursor-default select-none rounded-sm border p-2 \
                  focus-visible:outline focus-visible:outline-1 focus-visible:outline-accent {}",
                 if is_open() { "border-accent bg-active" }
                 else if on_cursor() { "border-line-strong bg-hover" }
                 else { "border-line bg-panel hover:bg-hover" })
             on:click=move |_| open()
             on:keydown=move |e: leptos::ev::KeyboardEvent| {
                 if e.key() == "Enter" { e.prevent_default(); open(); }
             }>
            <div class="flex items-center gap-2">
                <span class="shrink-0 leading-none">
                    <PersonAvatar id=id name=name.clone() size=22 />
                </span>
                <span class="min-w-0 truncate font-medium">{name_text(&frame)}</span>
                <span class="min-w-0 flex-1 truncate text-[11px] text-muted">{frame.role_title.clone()}</span>
            </div>
            <div class="mt-1.5 flex items-center gap-2">
                <span class="flex h-1 min-w-0 flex-1 overflow-hidden rounded-full bg-line" aria-hidden="true">
                    <span class="h-full bg-accent" style=format!("width: {work}%")></span>
                    <span class="h-full bg-warning" style=format!("width: {excess}%")></span>
                </span>
                <span class="shrink-0 text-[11px] tabular-nums text-muted">{load}</span>
            </div>
            <div class="mt-1.5 flex min-h-[1.25rem] flex-wrap items-center gap-1">
                {shown.into_iter().map(|p| view! { <span class=p.tone.chip()>{p.text}</span> }).collect_view()}
                {(more > 0).then(|| view! { <span class="text-[11px] text-muted">{format!("+{more}")}</span> })}
            </div>
            <div class="absolute inset-x-1 bottom-1 hidden items-center justify-end gap-1 rounded-sm bg-panel/95 p-0.5 \
                        group-hover:flex group-focus-within:flex"
                 on:click=|ev| ev.stop_propagation()>
                {(!is_self).then(|| {
                    let name = name.clone();
                    view! {
                        <button class=BUTTON title="Start a 1:1 note with them"
                                on:click=move |_| new_one_on_one(id, name.clone(), toasts, version, selection)>"New 1:1"</button>
                    }
                })}
                <button class=BUTTON title="Their tasks"
                        on:click=move |_| go_tasks(&tasks_path, Default::default())>"Tasks"</button>
                <button class=BUTTON title="Their load, week by week"
                        on:click=move |_| go_capacity(&capacity_path, Default::default())>"Capacity"</button>
            </div>
        </div>
    }
}

/// A frame and its place in the screen's `j`/`k` list.
type Placed = (Frame, Option<usize>);

/// The frames of one set of people: you, your reports, and (with the setting) the people under
/// them, each group under its manager's name. `first_index` is where the first frame sits in the
/// screen's `j`/`k` list (This week has none).
#[component]
pub fn FramesStrip(
    frames: PeopleFrames,
    #[prop(default = None)] first_index: Option<usize>,
) -> impl IntoView {
    let navigate = use_navigate();
    // The People screen gives the frames a place in its list; This week does not.
    let on_people = first_index.is_some();
    let mut next = first_index;
    let mut take = || {
        let at = next;
        next = next.map(|n| n + 1);
        at
    };
    let me_index = take();
    let reports: Vec<Placed> = frames
        .reports
        .iter()
        .cloned()
        .map(|f| {
            let at = take();
            (f, at)
        })
        .collect();
    // A manager can be one of your reports or anyone further down.
    let manager_name: HashMap<Uuid, String> = frames
        .reports
        .iter()
        .chain(frames.below.iter().flat_map(|g| g.frames.iter()))
        .map(|f| (f.person.node.id, f.person.label.clone()))
        .collect();
    let second: Vec<(u32, String, Vec<Placed>)> = frames
        .below
        .iter()
        .map(|group| {
            let who = manager_name
                .get(&group.manager)
                .cloned()
                .unwrap_or_default();
            let cards = group
                .frames
                .iter()
                .cloned()
                .map(|f| {
                    let at = take();
                    (f, at)
                })
                .collect();
            (group.depth, who, cards)
        })
        .collect();
    let hint = frames.hint;
    view! {
        <div class="flex flex-wrap items-start gap-2 px-4 py-3">
            <FrameCard frame=frames.me.clone() index=me_index />
            {reports
                .into_iter()
                .map(|(f, at)| view! { <FrameCard frame=f index=at /> })
                .collect_view()}
        </div>
        {second
            .into_iter()
            .map(|(depth, who, cards)| view! {
                // Each level is indented one step further under its manager.
                <div class="px-4 pb-3" style=format!("padding-left: {}rem", 1.0 + depth as f64 * 1.0)>
                    <p class="mb-1 text-[11px] text-muted">{format!("Reporting to {who}")}</p>
                    <div class="flex flex-wrap items-start gap-2">
                        {cards
                            .into_iter()
                            .map(|(f, at)| view! { <FrameCard frame=f index=at /> })
                            .collect_view()}
                    </div>
                </div>
            })
            .collect_view()}
        {hint.map(|h| view! {
            <p class="px-4 pb-3 text-[12px] text-muted">
                {hint_text(h, on_people)} " "
                {(!on_people).then(|| view! {
                    <button class="text-accent hover:underline"
                            on:click=move |_| navigate("/people", Default::default())>"Open People"</button>
                })}
            </p>
        })}
    }
}

/// Loads the frames and keeps the last answer, so the strip never blanks while it refreshes.
pub fn use_frames() -> RwSignal<Option<PeopleFrames>> {
    let version = expect_context::<DataVersion>();
    let loaded = LocalResource::new(move || {
        version.track();
        crate::api::get_people_frames()
    });
    let kept = RwSignal::new(None::<PeopleFrames>);
    let toasts = expect_context::<Toasts>();
    Effect::new(move |_| match loaded.get() {
        Some(Ok(f)) => kept.set(f),
        Some(Err(e)) => toasts.error(&e),
        None => {}
    });
    kept
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::NodeSummary;
    use minimap_types::NodeType;

    fn frame() -> Frame {
        Frame {
            person: NodeSummary {
                node: NodeRef::new(NodeType::Person, Uuid::from_u128(1)),
                label: "Raj Patel".into(),
                archived: false,
            },
            role_title: "Security Lead".into(),
            is_self: false,
            load_pct: Some(72),
            over: false,
            many_tasks: false,
            open_tasks: 5,
            blocked_tasks: 0,
            overdue_tasks: 0,
            waiting_on_them: 0,
            waiting_stale: 0,
            last_one_on_one: None,
            one_on_one_due: None,
            flags: Vec::new(),
        }
    }

    #[test]
    fn load_reads_as_a_percentage_of_this_week_or_says_there_is_no_capacity() {
        assert_eq!(load_text(&frame()), "72% this week");
        let mut over = frame();
        over.load_pct = Some(112);
        assert_eq!(load_text(&over), "112% this week");
        over.load_pct = None;
        assert_eq!(load_text(&over), "no capacity set");
    }

    #[test]
    fn the_bar_fills_to_the_load_and_shows_the_excess_at_its_end() {
        assert_eq!(bar_fill(None), (0, 0));
        assert_eq!(bar_fill(Some(0)), (0, 0));
        assert_eq!(bar_fill(Some(72)), (72, 0));
        assert_eq!(bar_fill(Some(100)), (100, 0));
        assert_eq!(bar_fill(Some(200)), (50, 50));
        assert_eq!(bar_fill(Some(220)), (45, 55));
        for load in [101, 150, 999, 100_000] {
            let (work, excess) = bar_fill(Some(load));
            assert_eq!(work + excess, 100, "{load}");
            assert!(excess > 0 || load == 100_000, "{load}");
        }
    }

    #[test]
    fn flags_are_worded_as_facts_with_the_task_colour_only_for_overdue() {
        let mut f = frame();
        f.open_tasks = 12;
        f.waiting_on_them = 2;
        let words = |flag| flag_text(flag, &f);
        assert_eq!(words(FrameFlag::Over).text, "Over capacity");
        assert_eq!(words(FrameFlag::ManyTasks).text, "12 open tasks");
        assert_eq!(
            words(FrameFlag::OneOnOneDue { days: 18 }).text,
            "1:1 due · 18 days"
        );
        assert_eq!(
            words(FrameFlag::OneOnOneDue { days: 1 }).text,
            "1:1 due · 1 day"
        );
        assert_eq!(
            words(FrameFlag::WaitingStale { count: 1 }).text,
            "Waiting on them · 2 (1 stale)"
        );
        assert_eq!(words(FrameFlag::Blocked { count: 1 }).text, "1 blocked");
        assert_eq!(words(FrameFlag::Overdue { count: 2 }).text, "2 overdue");
        for flag in [
            FrameFlag::Over,
            FrameFlag::ManyTasks,
            FrameFlag::OneOnOneDue { days: 3 },
            FrameFlag::WaitingStale { count: 1 },
            FrameFlag::Blocked { count: 1 },
        ] {
            assert_eq!(words(flag).tone, Tone::Warning);
        }
        assert_eq!(words(FrameFlag::Overdue { count: 2 }).tone, Tone::Danger);
    }

    #[test]
    fn pills_follow_the_flags_cap_at_three_and_count_the_rest() {
        let mut f = frame();
        f.flags = vec![
            FrameFlag::Over,
            FrameFlag::OneOnOneDue { days: 20 },
            FrameFlag::Blocked { count: 1 },
            FrameFlag::Overdue { count: 2 },
        ];
        let (shown, more) = pills(&f);
        let texts: Vec<&str> = shown.iter().map(|p| p.text.as_str()).collect();
        assert_eq!(texts, ["Over capacity", "1:1 due · 20 days", "1 blocked"]);
        assert_eq!(more, 1);
    }

    #[test]
    fn what_you_wait_on_them_for_is_a_plain_count_unless_some_is_stale() {
        let mut f = frame();
        f.waiting_on_them = 2;
        let (shown, _) = pills(&f);
        assert_eq!(shown[0].text, "Waiting on them · 2");
        assert_eq!(shown[0].tone, Tone::Neutral);
        f.waiting_stale = 1;
        f.flags = vec![FrameFlag::WaitingStale { count: 1 }];
        let (shown, _) = pills(&f);
        assert_eq!(shown.len(), 1, "one pill, not two");
        assert_eq!(shown[0].text, "Waiting on them · 2 (1 stale)");
        assert!(pills(&frame()).0.is_empty());
    }

    #[test]
    fn you_are_called_you_and_the_others_by_name() {
        assert_eq!(name_text(&frame()), "Raj Patel");
        let mut me = frame();
        me.is_self = true;
        assert_eq!(name_text(&me), "You");
    }

    #[test]
    fn the_cursor_walks_you_then_your_reports_then_the_people_under_them() {
        let at = |n: u128| {
            let mut f = frame();
            f.person.node = NodeRef::new(NodeType::Person, Uuid::from_u128(n));
            f
        };
        let frames = PeopleFrames {
            me: at(1),
            reports: vec![at(2), at(3)],
            below: vec![minimap_types::ReportGroup {
                manager: Uuid::from_u128(3),
                depth: 1,
                frames: vec![at(4)],
            }],
            hint: None,
        };
        let ids: Vec<u128> = nodes_of(&frames).iter().map(|n| n.id.as_u128()).collect();
        assert_eq!(ids, [1, 2, 3, 4]);
    }

    #[test]
    fn the_hint_says_what_to_do_where_it_is_read() {
        let on_people = hint_text(FramesHint::NoReports, true);
        assert!(on_people.contains("list below") && on_people.contains("Manager"));
        let elsewhere = hint_text(FramesHint::NoReports, false);
        assert!(elsewhere.contains("On the People screen"));
        // Neither sends you to People from People.
        assert!(!on_people.contains("Set managers in People"));
    }

    /// Frames are about support, never performance: none of these words appears in anything the
    /// module says.
    #[test]
    fn the_words_never_judge_a_person() {
        const BANNED: [&str; 7] = [
            "health",
            "score",
            "performance",
            "productiv",
            "utilis",
            "utiliz",
            "rank",
        ];
        let source = include_str!("people_frames.rs");
        let code = source.split("#[cfg(test)]").next().unwrap_or(source);
        let mut said = String::new();
        for line in code.lines().filter(|l| !l.trim_start().starts_with("//")) {
            for text in line.split('"').skip(1).step_by(2) {
                said.push_str(&text.to_lowercase());
                said.push('\n');
            }
        }
        assert!(said.len() > 200, "the scan found almost no text");
        for word in BANNED {
            assert!(!said.contains(word), "the word {word:?} is on a frame");
        }
    }
}
