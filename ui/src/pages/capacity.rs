//! Capacity: who is loaded how much, week by week. A heatmap of people x weeks coloured by load
//! (scheduled working days of their open tasks against their weekly capacity); click a cell to
//! see what makes it up. Separately flags people with more open tasks than the limit in Settings.

use leptos::prelude::*;
use minimap_types::{Capacity as Data, Date, NodeRef, NodeType, PersonCapacity, Uuid, WeekLoad};

use crate::{
    api,
    calendar::MONTHS,
    components::{
        form::{BUTTON, BUTTON_ON},
        page::{EmptyState, PageHeader, Tone},
    },
    state::{DataVersion, Selection},
    timeline::day_text,
};

// ------------------------------------------------------------------ pure logic

/// How a week's load looks on the heatmap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Band {
    Empty,
    /// Under half.
    Light,
    /// Comfortable.
    Good,
    /// Nearly or exactly full.
    Full,
    /// Over capacity.
    Over,
    /// Well over (more than a quarter above).
    Heavy,
}

pub fn band_of(load_pct: f64) -> Band {
    if load_pct <= 0.0 {
        Band::Empty
    } else if load_pct < 50.0 {
        Band::Light
    } else if load_pct < 85.0 {
        Band::Good
    } else if load_pct <= 100.0 {
        Band::Full
    } else if load_pct <= 125.0 {
        Band::Over
    } else {
        Band::Heavy
    }
}

pub const LEGEND: [(Band, &str); 5] = [
    (Band::Light, "under 50%"),
    (Band::Good, "50-85%"),
    (Band::Full, "85-100%"),
    (Band::Over, "over 100%"),
    (Band::Heavy, "over 125%"),
];

pub fn band_class(b: Band) -> &'static str {
    match b {
        Band::Empty => "text-faint",
        Band::Light => "bg-success/10 text-muted",
        Band::Good => "bg-success/20 text-success",
        Band::Full => "bg-warning/15 text-warning",
        Band::Over => "bg-warning/30 font-semibold text-warning",
        Band::Heavy => "bg-danger/25 font-semibold text-danger",
    }
}

pub fn cell_text(load_pct: f64) -> String {
    if load_pct <= 0.0 {
        "–".to_owned()
    } else {
        format!("{}%", load_pct.round() as i64)
    }
}

/// "Mar 1" for a week's Monday.
pub fn week_label(d: Date) -> String {
    let month = MONTHS[(d.month() as usize).saturating_sub(1).min(11)];
    format!("{} {}", &month[..3], d.day())
}

/// "3 days", "0.5 days", "1 day".
pub fn days_text(days: f64) -> String {
    let d = (days * 10.0).round() / 10.0;
    if (d - 1.0).abs() < 1e-9 {
        "1 day".to_owned()
    } else {
        format!("{d} days")
    }
}

/// What to say about a person besides the weeks: the open-task flag and the data caveat.
pub fn person_notes(p: &PersonCapacity, limit: u32) -> Vec<String> {
    let mut notes = Vec::new();
    if p.over_task_limit {
        notes.push(format!("{} open tasks (limit {limit})", p.active_tasks));
    }
    if p.unestimated_tasks > 0 {
        notes.push(format!(
            "{} without an estimate, counted as 1 day",
            p.unestimated_tasks
        ));
    }
    notes
}

/// The heading of the selected cell: "Priya · week of Mar 1: 140% (7 of 5 days)".
pub fn cell_heading(name: &str, w: &WeekLoad) -> String {
    format!(
        "{name} · week of {}: {}% ({} of {})",
        week_label(w.week_start),
        w.load_pct.round() as i64,
        trim(w.load_days),
        days_text(w.capacity_days)
    )
}

fn trim(x: f64) -> String {
    let r = (x * 10.0).round() / 10.0;
    if r.fract() == 0.0 {
        format!("{}", r as i64)
    } else {
        format!("{r}")
    }
}

const SPANS: [u32; 3] = [4, 8, 12];

// ------------------------------------------------------------------ the screen

#[component]
pub fn Capacity() -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let from = RwSignal::new(Option::<Date>::None);
    let weeks = RwSignal::new(8u32);
    let selected = RwSignal::new(Option::<(Uuid, Date)>::None);
    let data = LocalResource::new(move || {
        version.track();
        api::get_capacity(from.get(), None, Some(weeks.get()))
    });
    view! {
        <div class="flex h-full flex-col">
            {move || match data.get() {
                None => view! { <p class="p-6 text-muted">"Working it out…"</p> }.into_any(),
                Some(Err(e)) => view! { <p class="p-6 text-danger">{e.message}</p> }.into_any(),
                Some(Ok(c)) => view! { <Body data=c from=from weeks=weeks selected=selected /> }.into_any(),
            }}
        </div>
    }
}

#[component]
fn Body(
    data: Data,
    from: RwSignal<Option<Date>>,
    weeks: RwSignal<u32>,
    selected: RwSignal<Option<(Uuid, Date)>>,
) -> impl IntoView {
    let selection = expect_context::<Selection>();
    let (prev, next) = (data.prev_from, data.next_from);
    let current_week = data.current_week_start;
    let limit = data.task_limit;
    let template = format!(
        "grid-template-columns: 13rem repeat({}, minmax(3.75rem, 1fr))",
        data.weeks.len()
    );

    let head = data
        .weeks
        .iter()
        .map(|w| {
            let is_now = *w == current_week;
            let class = if is_now {
                "px-1 py-1 text-center text-[10px] font-semibold uppercase tracking-wider text-accent"
            } else {
                "px-1 py-1 text-center text-[10px] font-semibold uppercase tracking-wider text-muted"
            };
            view! { <div class=class>{week_label(*w)}</div> }
        })
        .collect_view();

    let rows = data
        .people
        .iter()
        .map(|p| {
            let person_id = p.person.node.id;
            let node = p.person.node;
            let notes = person_notes(p, limit);
            let flagged = p.over_task_limit;
            let cells = p
                .weeks
                .iter()
                .map(|w| {
                    let week = w.week_start;
                    let band = band_of(w.load_pct);
                    let is_sel = move || selected.get() == Some((person_id, week));
                    let class = move || {
                        format!(
                            "m-0.5 h-8 rounded-sm text-center tabular-nums text-[12px] {} {}",
                            band_class(band),
                            if is_sel() { "ring-1 ring-accent" } else { "hover:ring-1 hover:ring-line-strong" }
                        )
                    };
                    let label = format!("{}: {}", week_label(week), cell_text(w.load_pct));
                    view! {
                        <button class=class title=label
                                on:click=move |_| selected.set(Some((person_id, week)))>
                            {cell_text(w.load_pct)}
                        </button>
                    }
                })
                .collect_view();
            view! {
                <div class="contents">
                    <button class="flex min-w-0 flex-col justify-center border-b border-line px-4 py-1 text-left hover:bg-hover"
                            on:click=move |_| selection.open(node)>
                        <span class="flex items-center gap-2">
                            <span class="truncate font-medium">{p.person.label.clone()}</span>
                            {flagged.then(|| view! { <span class=Tone::Warning.chip()>"many tasks"</span> })}
                        </span>
                        <span class="truncate text-[11px] text-muted">
                            {if notes.is_empty() {
                                format!("{} open", p.active_tasks)
                            } else {
                                notes.join(" · ")
                            }}
                        </span>
                    </button>
                    {cells}
                </div>
            }
        })
        .collect_view();

    let legend = LEGEND
        .iter()
        .map(|(b, text)| {
            view! {
                <span class="flex items-center gap-1">
                    <span class=format!("inline-block h-3 w-5 rounded-sm {}", band_class(*b))></span>
                    {*text}
                </span>
            }
        })
        .collect_view();

    let detail = {
        let people = data.people.clone();
        move || {
            let (pid, week) = selected.get()?;
            let p = people.iter().find(|p| p.person.node.id == pid)?;
            let w = p.weeks.iter().find(|w| w.week_start == week)?;
            let heading = cell_heading(&p.person.label, w);
            let band = band_of(w.load_pct);
            let tasks = w
                .tasks
                .iter()
                .map(|t| {
                    let node = NodeRef::new(NodeType::Task, t.id);
                    let project = t.project_title.clone().unwrap_or_default();
                    let alloc = (t.allocation_pct != 100).then(|| format!("{}% allocated", t.allocation_pct));
                    let when = format!("{} → {}", day_text(t.start), day_text(t.finish));
                    view! {
                        <button class="flex w-full items-baseline gap-3 px-4 h-8 border-b border-line text-left hover:bg-hover"
                                on:click=move |_| selection.open(node)>
                            <span class="w-20 shrink-0 tabular-nums font-medium">{days_text(t.days)}</span>
                            <span class="min-w-0 truncate">{t.title.clone()}</span>
                            <span class="min-w-0 truncate text-muted">{project}</span>
                            {t.unestimated.then(|| view! { <span class=Tone::Neutral.chip()>"no estimate"</span> })}
                            <span class="ml-auto shrink-0 text-[11px] text-muted">{alloc}</span>
                            <span class="shrink-0 text-[11px] tabular-nums text-muted">{when}</span>
                        </button>
                    }
                })
                .collect_view();
            let empty = w.tasks.is_empty();
            Some(view! {
                <section class="border-t border-line">
                    <h2 class=format!("px-4 py-2 text-[13px] font-semibold {}", if matches!(band, Band::Over | Band::Heavy) { "text-warning" } else { "" })>
                        {heading}
                    </h2>
                    {empty.then(|| view! { <p class="px-4 pb-3 text-muted">"Nothing scheduled that week."</p> })}
                    {tasks}
                </section>
            })
        }
    };

    let warnings = data
        .warnings
        .iter()
        .map(|w| view! { <p class="px-4 py-1 text-danger">{w.clone()}</p> })
        .collect_view();
    let empty = data.people.is_empty();
    let span_buttons = SPANS
        .iter()
        .map(|n| {
            let n = *n;
            view! {
                <button class=move || format!("{BUTTON} {}", if weeks.get() == n { BUTTON_ON } else { "" })
                        on:click=move |_| weeks.set(n)>{format!("{n} weeks")}</button>
            }
        })
        .collect_view();
    let at_start = from.get_untracked().is_none();

    view! {
        <PageHeader icon="capacity" title="Capacity"
                    subtitle="Who is loaded how much, week by week">
            <button class=BUTTON aria-label="Earlier" on:click=move |_| from.set(Some(prev))>"‹"</button>
            <span class="tabular-nums text-muted">
                {format!("{} → {}", week_label(data.from), week_label(data.to))}
            </span>
            <button class=BUTTON aria-label="Later" on:click=move |_| from.set(Some(next))>"›"</button>
            <button class=BUTTON disabled=at_start on:click=move |_| from.set(None)>"This week"</button>
            <span class="ml-auto flex items-center gap-1">{span_buttons}</span>
        </PageHeader>
        {warnings}
        <div class="min-h-0 flex-1 overflow-y-auto">
            {empty.then(|| view! {
                <EmptyState icon="capacity" title="No people yet"
                    hint="Add people and assign them tasks with estimates; their weekly load appears here." />
            })}
            {(!empty).then(|| view! {
                <div class="overflow-x-auto">
                    <div class="grid min-w-max items-stretch" style=template>
                        <div class="px-4 py-1 text-[10px] font-semibold uppercase tracking-wider text-muted">"Person"</div>
                        {head}
                        {rows}
                    </div>
                </div>
                <div class="flex flex-wrap items-center gap-3 border-t border-line px-4 py-2 text-[11px] text-muted">
                    <span class="font-semibold uppercase tracking-wider">"Load"</span>
                    {legend}
                </div>
            })}
            {detail}
            <p class="px-4 py-3 text-[11px] text-muted">
                {format!(
                    "Load is the working days of an open task that fall in a week (from the schedule), times its allocation, \
                     against the person's weekly hours at {} hours a day. Click a cell to see what makes it up. \
                     \"Many tasks\" means more than {limit} open tasks (change it in Settings); it works even when estimates are missing.",
                    trim(data.hours_per_day)
                )}
            </p>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{NodeSummary, Uuid};
    use time::macros::date;

    fn person(active: u32, over: bool, unest: u32) -> PersonCapacity {
        PersonCapacity {
            person: NodeSummary {
                node: NodeRef::new(NodeType::Person, Uuid::nil()),
                label: "P".into(),
                archived: false,
            },
            weekly_capacity_hours: 40.0,
            weeks: vec![],
            peak_pct: 0.0,
            peak_week: None,
            over_weeks: 0,
            active_tasks: active,
            over_task_limit: over,
            unestimated_tasks: unest,
        }
    }

    #[test]
    fn bands_follow_the_load() {
        assert_eq!(band_of(0.0), Band::Empty);
        assert_eq!(band_of(20.0), Band::Light);
        assert_eq!(band_of(49.9), Band::Light);
        assert_eq!(band_of(50.0), Band::Good);
        assert_eq!(band_of(84.9), Band::Good);
        assert_eq!(band_of(85.0), Band::Full);
        assert_eq!(band_of(100.0), Band::Full, "exactly full is not over");
        assert_eq!(band_of(100.1), Band::Over);
        assert_eq!(band_of(125.0), Band::Over);
        assert_eq!(band_of(125.1), Band::Heavy);
        // Every band has its own look, and the legend covers all but Empty.
        let looks: std::collections::HashSet<_> = [
            Band::Empty,
            Band::Light,
            Band::Good,
            Band::Full,
            Band::Over,
            Band::Heavy,
        ]
        .iter()
        .map(|b| band_class(*b))
        .collect();
        assert_eq!(looks.len(), 6);
        assert_eq!(LEGEND.len(), 5);
    }

    #[test]
    fn cells_and_labels_read_naturally() {
        assert_eq!(cell_text(0.0), "–");
        assert_eq!(cell_text(139.6), "140%");
        assert_eq!(week_label(date!(2027 - 03 - 01)), "Mar 1");
        assert_eq!(week_label(date!(2027 - 12 - 27)), "Dec 27");
        assert_eq!(days_text(1.0), "1 day");
        assert_eq!(days_text(2.5), "2.5 days");
        assert_eq!(days_text(5.0), "5 days");
    }

    #[test]
    fn people_notes_mention_the_flag_and_the_data_caveat() {
        assert!(person_notes(&person(3, false, 0), 10).is_empty());
        assert_eq!(
            person_notes(&person(12, true, 0), 10),
            ["12 open tasks (limit 10)"]
        );
        assert_eq!(
            person_notes(&person(12, true, 4), 10),
            [
                "12 open tasks (limit 10)",
                "4 without an estimate, counted as 1 day"
            ]
        );
    }

    #[test]
    fn the_selected_cell_says_what_it_is() {
        let w = WeekLoad {
            week_start: date!(2027 - 03 - 01),
            load_days: 7.0,
            capacity_days: 5.0,
            load_pct: 140.0,
            over: true,
            tasks: vec![],
        };
        assert_eq!(
            cell_heading("Priya", &w),
            "Priya · week of Mar 1: 140% (7 of 5 days)"
        );
    }
}
