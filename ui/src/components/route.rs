//! Route recalculation (spec 46): the What-if screen's Route view. For each project a slip
//! changes, the old route to the finish (dashed, faded) and the new one (accent), the new arrival
//! date, and a turn-by-turn list of where the slip enters, is absorbed, passes on and arrives.
//! The numbers are `ProjectRoute` from the impact report; this file lays them out and says them.

use leptos::prelude::*;
use minimap_types::{Date, NodeRef, NodeType, ProjectRoute, RouteBar, Turn, TurnKind, Uuid};

use crate::{
    components::page::{Icon, Tone},
    state::Selection,
    timeline::{day_text, BAR_H, ROW_H},
};

const EPS: f64 = 1e-9;
/// Pixels per working day, the axis height, and the most rows before only the route is drawn.
const PX_PER_DAY: f64 = 22.0;
const HEADER_H: f64 = 20.0;
const MAX_ROWS: usize = 12;
const MIN_BAR_W: f64 = 3.0;

// ------------------------------------------------------------------ pure text

/// "1 day", "5 days", "0.5 days".
pub fn days_text(n: f64) -> String {
    let n = (n * 10.0).round() / 10.0;
    if (n - 1.0).abs() < EPS {
        "1 day".to_owned()
    } else {
        format!("{n} days")
    }
}

/// "2", "0.5".
fn number_text(n: f64) -> String {
    format!("{}", (n * 10.0).round() / 10.0)
}

/// "1 working day", "3 working days".
fn working_days(n: f64) -> String {
    let n = (n * 10.0).round() / 10.0;
    if (n - 1.0).abs() < EPS {
        "1 working day".to_owned()
    } else {
        format!("{n} working days")
    }
}

/// Nothing changes: the same route, the same finish. The card is then one line.
pub fn is_quiet(route: &ProjectRoute) -> bool {
    !route.changed && route.delay_days <= EPS
}

/// The card's first line: "EU Region · recalculated: arrives 2027-04-14 instead of 2027-04-07 (+5
/// working days), 3 working days past target" / "Platform · no change to the route; finish
/// unchanged".
pub fn route_headline(route: &ProjectRoute) -> String {
    let title = &route.title;
    if is_quiet(route) {
        return format!("{title} · no change to the route; finish unchanged");
    }
    if route.delay_days <= EPS {
        return format!("{title} · the route changes; the finish does not");
    }
    let mut text = match (route.old_finish, route.new_finish) {
        (Some(old), Some(new)) => format!(
            "{title} · recalculated: arrives {new} instead of {old} (+{})",
            working_days(route.delay_days)
        ),
        _ => format!(
            "{title} · recalculated: arrives +{}",
            working_days(route.delay_days)
        ),
    };
    if let Some(late) = route.late_after {
        text.push_str(&format!(", {} past target", working_days(f64::from(late))));
    }
    text
}

/// What a step says, in plain words, after the task's name: "slips 5 days: the delay starts here",
/// "and 2 more tasks move 5 days later", "absorbs 2 of the 5 days (it has slack) and moves 3 days
/// later", "is now on the route", "the project now finishes here, 5 days later (...)".
pub fn turn_text(turn: &Turn, route: &ProjectRoute) -> String {
    match turn.kind {
        TurnKind::Slips => format!(
            "slips {}: the delay starts here",
            days_text(turn.incoming_days)
        ),
        TurnKind::PassesOn if turn.also > 0 => format!(
            "and {} more {} move {} later",
            turn.also,
            if turn.also == 1 { "task" } else { "tasks" },
            days_text(turn.delay_days)
        ),
        TurnKind::PassesOn => format!("moves {} later", days_text(turn.delay_days)),
        TurnKind::Absorbs => format!(
            "absorbs all {}: the delay stops here",
            days_text(turn.incoming_days)
        ),
        TurnKind::PartlyAbsorbs => format!(
            "absorbs {} of the {} (it has slack) and moves {} later",
            number_text(turn.absorbed_days),
            days_text(turn.incoming_days),
            days_text(turn.delay_days)
        ),
        TurnKind::JoinsRoute if turn.delay_days > EPS => format!(
            "is now on the route, and moves {} later",
            days_text(turn.delay_days)
        ),
        TurnKind::JoinsRoute => "is now on the route".to_owned(),
        TurnKind::LeavesRoute => "is no longer on the route".to_owned(),
        TurnKind::Arrives => {
            let mut text = if turn.delay_days > EPS {
                format!(
                    "the project now finishes here, {} later",
                    days_text(turn.delay_days)
                )
            } else {
                "the project still finishes here, on time".to_owned()
            };
            text.push_str(&format!(" ({})", day_text(turn.new_finish)));
            if let Some(late) = route.late_after {
                text.push_str(&format!(": {} past target", working_days(f64::from(late))));
            }
            text
        }
        TurnKind::More => format!("… {} more steps", turn.also),
    }
}

/// Only an arrival that makes the project newly late is red; a slip fully absorbed is good news.
pub fn turn_tone(turn: &Turn, route: &ProjectRoute) -> Tone {
    match turn.kind {
        TurnKind::Arrives if route.newly_late => Tone::Danger,
        TurnKind::Absorbs => Tone::Success,
        _ => Tone::Neutral,
    }
}

/// The icon of a step.
pub fn turn_icon(kind: TurnKind) -> &'static str {
    match kind {
        TurnKind::Slips => "step-start",
        TurnKind::PassesOn => "step-arrow",
        TurnKind::Absorbs | TurnKind::PartlyAbsorbs => "step-absorb",
        TurnKind::JoinsRoute => "step-join",
        TurnKind::LeavesRoute => "step-leave",
        TurnKind::Arrives => "step-flag",
        TurnKind::More => "step-more",
    }
}

/// What an icon means, for the key under the steps.
pub fn icon_meaning(icon: &str) -> &'static str {
    match icon {
        "step-start" => "where the slip starts",
        "step-arrow" => "moves later, passing the delay on",
        "step-absorb" => "has slack, so it absorbs some or all of the delay",
        "step-join" => "is now on the route",
        "step-leave" => "is no longer on the route",
        "step-flag" => "where the project now finishes",
        _ => "steps left out of a long route",
    }
}

/// The icons the steps use, once each and in a fixed order, with what each means.
pub fn step_key(turns: &[Turn]) -> Vec<(&'static str, &'static str)> {
    const ORDER: [&str; 7] = [
        "step-start",
        "step-arrow",
        "step-absorb",
        "step-join",
        "step-leave",
        "step-flag",
        "step-more",
    ];
    ORDER
        .into_iter()
        .filter(|icon| turns.iter().any(|t| turn_icon(t.kind) == *icon))
        .map(|icon| (icon, icon_meaning(icon)))
        .collect()
}

// ------------------------------------------------------------------ layout

/// A task's bars in pixels: where it was (old) and where it is now (new).
#[derive(Clone, Debug, PartialEq)]
pub struct BarPx {
    pub id: Uuid,
    pub title: String,
    pub y: f64,
    pub old_x: f64,
    pub old_w: f64,
    pub new_x: f64,
    pub new_w: f64,
    pub on_old_route: bool,
    pub on_new_route: bool,
    /// The old bar is drawn only when it is somewhere else or was on the old route.
    pub show_old: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RouteLayout {
    pub width: f64,
    pub height: f64,
    pub bars: Vec<BarPx>,
    /// The x of each week start and its label.
    pub ticks: Vec<(f64, String)>,
    pub today_x: f64,
    pub target_x: Option<f64>,
    /// The old and the new route as polylines through the bars of each chain.
    pub old_line: Vec<(f64, f64)>,
    pub new_line: Vec<(f64, f64)>,
}

fn width_of(from: f64, to: f64) -> f64 {
    ((to - from) * PX_PER_DAY).max(MIN_BAR_W)
}

/// The bars to draw: all of them, or when there are more than `MAX_ROWS`, the route's own.
fn drawn(bars: &[RouteBar]) -> Vec<&RouteBar> {
    if bars.len() <= MAX_ROWS {
        bars.iter().collect()
    } else {
        bars.iter()
            .filter(|b| b.on_old_route || b.on_new_route)
            .collect()
    }
}

/// Where everything goes. Rows are in the order of `route.bars` (by new start).
pub fn route_layout(route: &ProjectRoute) -> RouteLayout {
    let first = route.first_offset as f64;
    let x_of = |offset: f64| (offset - first) * PX_PER_DAY;
    let rows = drawn(&route.bars);
    let bars: Vec<BarPx> = rows
        .iter()
        .enumerate()
        .map(|(row, b)| {
            let (old_x, new_x) = (x_of(b.old_es), x_of(b.new_es));
            let (old_w, new_w) = (width_of(b.old_es, b.old_ef), width_of(b.new_es, b.new_ef));
            BarPx {
                id: b.id,
                title: b.title.clone(),
                y: HEADER_H + row as f64 * ROW_H + (ROW_H - BAR_H) / 2.0,
                old_x,
                old_w,
                new_x,
                new_w,
                on_old_route: b.on_old_route,
                on_new_route: b.on_new_route,
                show_old: (old_x - new_x).abs() > EPS
                    || (old_w - new_w).abs() > EPS
                    || b.on_old_route,
            }
        })
        .collect();
    // A polyline through the bars of a chain, in the order of the chain.
    let line = |chain: &[Uuid], x: fn(&BarPx) -> (f64, f64)| -> Vec<(f64, f64)> {
        chain
            .iter()
            .filter_map(|id| bars.iter().find(|b| b.id == *id))
            .flat_map(|b| {
                let (start, end) = x(b);
                let y = b.y + BAR_H / 2.0;
                [(start, y), (end, y)]
            })
            .collect()
    };
    let old_line = line(&route.before, |b| (b.old_x, b.old_x + b.old_w));
    let new_line = line(&route.after, |b| (b.new_x, b.new_x + b.new_w));
    let mut ticks = Vec::new();
    let mut last_weekday: Option<u8> = None;
    for (i, d) in route.days.iter().enumerate() {
        let weekday = d.weekday().number_days_from_monday();
        if last_weekday.is_none_or(|w| weekday <= w) {
            ticks.push((i as f64 * PX_PER_DAY, short_date(*d)));
        }
        last_weekday = Some(weekday);
    }
    RouteLayout {
        width: route.days.len() as f64 * PX_PER_DAY,
        height: HEADER_H + rows.len() as f64 * ROW_H,
        bars,
        ticks,
        today_x: x_of(0.0),
        target_x: route.target_offset.map(x_of),
        old_line,
        new_line,
    }
}

/// "Mar 1".
fn short_date(d: Date) -> String {
    let month = d.month().to_string();
    format!("{} {}", month.get(..3).unwrap_or(&month), d.day())
}

fn points(line: &[(f64, f64)]) -> String {
    line.iter()
        .map(|(x, y)| format!("{x:.1},{y:.1}"))
        .collect::<Vec<_>>()
        .join(" ")
}

// ------------------------------------------------------------------ components

/// The Route view: a card for each project the slip changes.
#[component]
pub fn RouteList(routes: Vec<ProjectRoute>) -> impl IntoView {
    if routes.is_empty() {
        return view! {
            <p class="px-4 py-4 text-muted">
                "No project's route is affected. The Details tab lists everything that moves."
            </p>
        }
        .into_any();
    }
    routes
        .into_iter()
        .map(|r| view! { <RouteCard route=r /> })
        .collect_view()
        .into_any()
}

/// One project: the headline, then (unless nothing changes) the two routes and the steps.
#[component]
fn RouteCard(route: ProjectRoute) -> impl IntoView {
    let headline = StoredValue::new(route_headline(&route));
    let quiet = is_quiet(&route);
    let tone = if route.newly_late {
        "text-danger"
    } else if route.delay_days > EPS {
        "text-fg"
    } else {
        "text-muted"
    };
    let body = (!quiet).then(|| {
        view! {
            <RouteTimeline route=route.clone() />
            <TurnList route=route.clone() />
        }
    });
    view! {
        <section class="border-b border-line px-4 py-3" aria-label=headline.get_value()>
            <h3 class=format!("text-[13px] font-medium {tone}")>{headline.get_value()}</h3>
            {body}
        </section>
    }
}

/// The two routes on one timeline: the old one dashed and faded, the new one in the accent.
#[component]
fn RouteTimeline(route: ProjectRoute) -> impl IntoView {
    let selection = expect_context::<Selection>();
    let l = route_layout(&route);
    let labels = l
        .bars
        .iter()
        .map(|b| {
            let id = b.id;
            let (title, tip) = (b.title.clone(), b.title.clone());
            let class = if b.on_new_route {
                "block w-full truncate px-1 text-left font-semibold"
            } else {
                "block w-full truncate px-1 text-left"
            };
            view! {
                <button class=class style=format!("height:{ROW_H}px;line-height:{ROW_H}px")
                        title=tip
                        on:click=move |_| selection.open(NodeRef::new(NodeType::Task, id))>
                    {title}
                </button>
            }
        })
        .collect_view();
    let ticks = l
        .ticks
        .iter()
        .map(|(x, label)| {
            let h = l.height;
            view! {
                <line class="week-line" x1=*x x2=*x y1=0 y2=h />
                <text x=*x + 2.0 y=12>{label.clone()}</text>
            }
        })
        .collect_view();
    let bars = l
        .bars
        .iter()
        .map(|b| {
            let id = b.id;
            view! {
                {b.show_old.then(|| view! {
                    <rect class="old-bar" x=b.old_x y=b.y width=b.old_w height=BAR_H rx="1" />
                })}
                <rect class=if b.on_new_route { "new-bar on-route" } else { "new-bar" }
                      x=b.new_x y=b.y width=b.new_w height=BAR_H rx="1"
                      on:click=move |_| selection.open(NodeRef::new(NodeType::Task, id)) />
            }
        })
        .collect_view();
    let target = l
        .target_x
        .map(|x| view! { <line class="target" x1=x x2=x y1=HEADER_H y2=l.height /> });
    view! {
        <div class="mt-2 flex">
            <div class="w-[9rem] shrink-0 overflow-hidden border-r border-line"
                 style=format!("padding-top:{HEADER_H}px")>{labels}</div>
            <div class="min-w-0 flex-1 overflow-x-auto">
                <svg class="route-svg" width=l.width height=l.height
                     role="img" aria-label="The old route dashed and the new route in colour">
                    <line class="head-line" x1=0 x2=l.width y1=HEADER_H y2=HEADER_H />
                    {ticks}
                    <line class="today" x1=l.today_x x2=l.today_x y1=HEADER_H y2=l.height />
                    {target}
                    {bars}
                    <polyline class="old-line" points=points(&l.old_line) />
                    <polyline class="new-line" points=points(&l.new_line) />
                </svg>
            </div>
        </div>
        <ChartKey />
    }
}

/// What the timeline's marks mean: a swatch and a few words each.
#[component]
fn ChartKey() -> impl IntoView {
    let item = "flex items-center gap-1.5";
    view! {
        <ul class="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1 text-[11px] text-muted" aria-label="Key">
            <li class=item>
                <span class="inline-block h-2.5 w-5 rounded-[1px] border border-dashed border-muted"></span>
                "Where it was before the slip"
            </li>
            <li class=item>
                <span class="inline-block h-2.5 w-5 rounded-[1px] bg-accent"></span>
                "On the route now"
            </li>
            <li class=item>
                <span class="inline-block h-2.5 w-5 rounded-[1px] bg-muted"></span>
                "Moved, but not on the route (it has slack)"
            </li>
            <li class=item>
                <span class="inline-block h-3 w-px bg-accent"></span>
                "Today"
            </li>
            <li class=item>
                <span class="inline-block h-3 w-0 border-l border-dashed border-danger"></span>
                "Target date"
            </li>
        </ul>
    }
}

/// The steps, in order, each in a sentence, with a key to the icons under them.
#[component]
fn TurnList(route: ProjectRoute) -> impl IntoView {
    let selection = expect_context::<Selection>();
    let key = step_key(&route.turns)
        .into_iter()
        .map(|(icon, meaning)| {
            view! {
                <li class="flex items-center gap-1.5">
                    <span class="text-fg"><Icon name=icon size="h-3.5 w-3.5" /></span>
                    {meaning}
                </li>
            }
        })
        .collect_view();
    let items = route
        .turns
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let node = t.task.node;
            let text = turn_text(t, &route);
            let tone = turn_tone(t, &route);
            let more = t.kind == TurnKind::More;
            view! {
                <li class="flex items-baseline gap-2 py-0.5">
                    <span class="w-4 shrink-0 text-right text-[11px] tabular-nums text-muted">{i + 1}</span>
                    <span class=format!("shrink-0 self-center {}", if tone == Tone::Neutral { "text-fg" } else { tone.text() })>
                        <Icon name=turn_icon(t.kind) size="h-3.5 w-3.5" />
                    </span>
                    <span class="min-w-0">
                        {(!more).then(|| {
                            let name = t.task.label.clone();
                            view! {
                                <button class="font-medium hover:underline"
                                        on:click=move |_| selection.open(node)>{name}</button>
                                " "
                            }
                        })}
                        <span class=if tone == Tone::Neutral { "text-muted" } else { tone.text() }>{text}</span>
                    </span>
                </li>
            }
        })
        .collect_view();
    view! {
        <h4 class="mt-3 text-[10px] font-semibold uppercase tracking-wider text-muted">"Step by step"</h4>
        <ol class="mt-1 text-[12px]" aria-label="Step by step">{items}</ol>
        <ul class="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1 border-t border-line pt-2 text-[11px] text-muted"
            aria-label="What the icons mean">{key}</ul>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::NodeSummary;
    use time::macros::date;

    fn node(n: u128, label: &str) -> NodeSummary {
        NodeSummary {
            node: NodeRef::new(NodeType::Task, Uuid::from_u128(n)),
            label: label.into(),
            archived: false,
        }
    }

    fn turn(kind: TurnKind, incoming: f64, absorbed: f64, delay: f64) -> Turn {
        Turn {
            task: node(1, "Gateway auth"),
            kind,
            incoming_days: incoming,
            absorbed_days: absorbed,
            delay_days: delay,
            new_finish: date!(2027 - 04 - 14),
            also: 0,
        }
    }

    fn bar(n: u128, old: (f64, f64), new: (f64, f64), on_old: bool, on_new: bool) -> RouteBar {
        RouteBar {
            id: Uuid::from_u128(n),
            title: format!("t{n}"),
            old_es: old.0,
            old_ef: old.1,
            new_es: new.0,
            new_ef: new.1,
            on_old_route: on_old,
            on_new_route: on_new,
        }
    }

    fn route(changed: bool, delay: f64) -> ProjectRoute {
        ProjectRoute {
            project_id: Uuid::from_u128(10),
            title: "EU Region".into(),
            before: vec![Uuid::from_u128(1), Uuid::from_u128(2)],
            after: vec![Uuid::from_u128(1), Uuid::from_u128(2)],
            changed,
            old_finish: Some(date!(2027 - 04 - 07)),
            new_finish: Some(date!(2027 - 04 - 14)),
            target_date: Some(date!(2027 - 04 - 09)),
            delay_days: delay,
            late_after: Some(3),
            newly_late: true,
            turns: Vec::new(),
            bars: vec![
                bar(1, (0.0, 2.0), (0.0, 2.0), true, true),
                bar(2, (2.0, 4.0), (2.0, 4.0), true, true),
            ],
            first_offset: 0,
            days: (0..10)
                .map(|i| date!(2027 - 03 - 01) + time::Duration::days(i))
                .collect(),
            target_offset: Some(8.0),
        }
    }

    #[test]
    fn the_headline_says_recalculated_with_both_dates_or_no_change() {
        let r = route(false, 5.0);
        assert_eq!(
            route_headline(&r),
            "EU Region · recalculated: arrives 2027-04-14 instead of 2027-04-07 (+5 working days), 3 working days past target"
        );
        let mut quiet = route(false, 0.0);
        quiet.late_after = None;
        assert!(is_quiet(&quiet));
        assert_eq!(
            route_headline(&quiet),
            "EU Region · no change to the route; finish unchanged"
        );
        let moved = route(true, 0.0);
        assert!(!is_quiet(&moved));
        assert_eq!(
            route_headline(&moved),
            "EU Region · the route changes; the finish does not"
        );
        let mut one = route(false, 1.0);
        one.late_after = None;
        assert!(route_headline(&one).ends_with("(+1 working day)"));
    }

    #[test]
    fn each_step_says_what_it_does_in_plain_words() {
        let r = route(false, 5.0);
        assert_eq!(
            turn_text(&turn(TurnKind::Slips, 5.0, 0.0, 5.0), &r),
            "slips 5 days: the delay starts here"
        );
        assert_eq!(
            turn_text(&turn(TurnKind::PassesOn, 5.0, 0.0, 5.0), &r),
            "moves 5 days later"
        );
        let mut many = turn(TurnKind::PassesOn, 5.0, 0.0, 5.0);
        many.also = 2;
        assert_eq!(turn_text(&many, &r), "and 2 more tasks move 5 days later");
        many.also = 1;
        assert_eq!(turn_text(&many, &r), "and 1 more task move 5 days later");
        assert_eq!(
            turn_text(&turn(TurnKind::PartlyAbsorbs, 5.0, 2.0, 3.0), &r),
            "absorbs 2 of the 5 days (it has slack) and moves 3 days later"
        );
        assert_eq!(
            turn_text(&turn(TurnKind::Absorbs, 1.0, 1.0, 0.0), &r),
            "absorbs all 1 day: the delay stops here"
        );
        assert_eq!(
            turn_text(&turn(TurnKind::JoinsRoute, 5.0, 0.0, 5.0), &r),
            "is now on the route, and moves 5 days later"
        );
        assert_eq!(
            turn_text(&turn(TurnKind::JoinsRoute, 0.0, 0.0, 0.0), &r),
            "is now on the route"
        );
        assert_eq!(
            turn_text(&turn(TurnKind::LeavesRoute, 0.0, 0.0, 0.0), &r),
            "is no longer on the route"
        );
        assert_eq!(
            turn_text(&turn(TurnKind::Arrives, 5.0, 0.0, 5.0), &r),
            "the project now finishes here, 5 days later (Wed 2027-04-14): 3 working days past target"
        );
        let mut calm = route(false, 0.0);
        calm.late_after = None;
        assert_eq!(
            turn_text(&turn(TurnKind::Arrives, 0.0, 0.0, 0.0), &calm),
            "the project still finishes here, on time (Wed 2027-04-14)"
        );
        let mut more = turn(TurnKind::More, 0.0, 0.0, 0.0);
        more.also = 6;
        assert_eq!(turn_text(&more, &r), "… 6 more steps");
    }

    #[test]
    fn the_key_lists_only_the_icons_the_steps_use_once_each_and_in_order() {
        let turns = [
            turn(TurnKind::Slips, 5.0, 0.0, 5.0),
            turn(TurnKind::PassesOn, 5.0, 0.0, 5.0),
            turn(TurnKind::Absorbs, 1.0, 1.0, 0.0),
            turn(TurnKind::PartlyAbsorbs, 5.0, 2.0, 3.0),
            turn(TurnKind::Arrives, 5.0, 0.0, 5.0),
        ];
        let key = step_key(&turns);
        let icons: Vec<&str> = key.iter().map(|k| k.0).collect();
        // Absorbs and partly absorbs share one icon, so one entry; nothing for kinds not used.
        assert_eq!(
            icons,
            ["step-start", "step-arrow", "step-absorb", "step-flag"]
        );
        assert!(key.iter().all(|(_, meaning)| !meaning.is_empty()));
        assert_eq!(icon_meaning("step-start"), "where the slip starts");
        assert!(step_key(&[]).is_empty());
        // Every icon a step can have is explained.
        for k in [
            TurnKind::Slips,
            TurnKind::PassesOn,
            TurnKind::Absorbs,
            TurnKind::JoinsRoute,
            TurnKind::LeavesRoute,
            TurnKind::Arrives,
            TurnKind::More,
        ] {
            assert!(step_key(&[turn(k, 0.0, 0.0, 0.0)]).len() == 1, "{k:?}");
        }
    }

    #[test]
    fn only_a_newly_late_arrival_is_red_and_full_absorption_is_good_news() {
        let r = route(false, 5.0);
        assert_eq!(
            turn_tone(&turn(TurnKind::Arrives, 5.0, 0.0, 5.0), &r),
            Tone::Danger
        );
        let mut fine = r.clone();
        fine.newly_late = false;
        assert_eq!(
            turn_tone(&turn(TurnKind::Arrives, 5.0, 0.0, 5.0), &fine),
            Tone::Neutral
        );
        assert_eq!(
            turn_tone(&turn(TurnKind::Absorbs, 1.0, 1.0, 0.0), &r),
            Tone::Success
        );
        assert_eq!(
            turn_tone(&turn(TurnKind::PassesOn, 5.0, 0.0, 5.0), &r),
            Tone::Neutral
        );
        assert_eq!(
            turn_tone(&turn(TurnKind::Slips, 5.0, 0.0, 5.0), &r),
            Tone::Neutral
        );
    }

    #[test]
    fn every_kind_of_step_has_its_own_glyph_where_it_should() {
        let kinds = [
            TurnKind::Slips,
            TurnKind::PassesOn,
            TurnKind::Absorbs,
            TurnKind::PartlyAbsorbs,
            TurnKind::JoinsRoute,
            TurnKind::LeavesRoute,
            TurnKind::Arrives,
            TurnKind::More,
        ];
        for k in kinds {
            assert!(crate::icons::paths(turn_icon(k)).is_some(), "{k:?}");
        }
        assert_eq!(
            turn_icon(TurnKind::Absorbs),
            turn_icon(TurnKind::PartlyAbsorbs)
        );
        assert_ne!(turn_icon(TurnKind::Slips), turn_icon(TurnKind::Arrives));
        assert_ne!(
            turn_icon(TurnKind::JoinsRoute),
            turn_icon(TurnKind::LeavesRoute)
        );
    }

    #[test]
    fn the_layout_puts_old_and_new_bars_on_one_axis_and_joins_each_route() {
        let mut r = route(true, 3.0);
        // The second task moves three days later.
        r.bars[1] = bar(2, (2.0, 4.0), (5.0, 7.0), true, true);
        let l = route_layout(&r);
        assert_eq!(l.width, 10.0 * PX_PER_DAY);
        assert_eq!(l.bars.len(), 2);
        // Row positions: below the axis, one row apart.
        assert_eq!(l.bars[1].y - l.bars[0].y, ROW_H);
        // The first task did not move; the second did.
        assert_eq!(l.bars[0].new_x, 0.0);
        assert_eq!(l.bars[1].old_x, 2.0 * PX_PER_DAY);
        assert_eq!(l.bars[1].new_x, 5.0 * PX_PER_DAY);
        assert_eq!(l.bars[1].new_w, 2.0 * PX_PER_DAY);
        // The old bar of a task that did not move is only drawn because it was on the old route.
        assert!(l.bars[0].show_old);
        r.bars[0].on_old_route = false;
        assert!(!route_layout(&r).bars[0].show_old);
        // Each route is a line through its tasks: a start and an end each.
        assert_eq!(l.old_line.len(), 4);
        assert_eq!(l.new_line.len(), 4);
        assert_eq!(l.new_line[2].0, 5.0 * PX_PER_DAY);
        assert_eq!(l.old_line[2].0, 2.0 * PX_PER_DAY);
        // Today and the target.
        assert_eq!(l.today_x, 0.0);
        assert_eq!(l.target_x, Some(8.0 * PX_PER_DAY));
        assert_eq!(l.height, HEADER_H + 2.0 * ROW_H);
        // A week tick at the start and at the next Monday (2027-03-01 is a Monday).
        assert_eq!(l.ticks[0].1, "Mar 1");
        assert_eq!(l.ticks.len(), 2);
    }

    #[test]
    fn too_many_rows_leave_only_the_route_and_a_tiny_bar_stays_visible() {
        let mut r = route(false, 1.0);
        r.bars = (1..=14)
            .map(|n| bar(n, (0.0, 0.0), (0.0, 0.0), n <= 2, n <= 2))
            .collect();
        let l = route_layout(&r);
        assert_eq!(l.bars.len(), 2);
        assert!(l.bars[0].new_w >= MIN_BAR_W);
        // Twelve or fewer are all drawn.
        r.bars.truncate(12);
        assert_eq!(route_layout(&r).bars.len(), 12);
    }

    #[test]
    fn days_read_naturally() {
        assert_eq!(days_text(1.0), "1 day");
        assert_eq!(days_text(2.0), "2 days");
        assert_eq!(days_text(0.5), "0.5 days");
        assert_eq!(days_text(2.04), "2 days");
    }
}
