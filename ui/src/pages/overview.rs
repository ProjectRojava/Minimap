//! The Overview: what is at risk and why, without asking. Objectives with computed health and
//! the projects under them, the top risks, overloaded people and stale waiting-ons. Every row
//! opens its detail pane.

use leptos::prelude::*;
use minimap_types::{
    HealthLevel, NodeRef, NodeType, ObjectiveHealthRow, OverloadedPerson, PortfolioOverview,
    ProjectHealthRow, RiskItem, WaitingOnRow,
};

use crate::{
    api,
    components::{
        health::{
            counts_text, finish_text, load_text, reasons_text, risk_kind_label, risk_priority_text,
            HealthMark,
        },
        page::{EmptyState, PageHeader, CHIP},
        waiting_panel::age_text,
    },
    labels::objective_status_label,
    state::{DataVersion, Selection},
};

#[component]
pub fn Overview() -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let overview = LocalResource::new(move || {
        version.track();
        api::get_portfolio_overview()
    });
    view! {
        <div class="h-full overflow-y-auto">
            {move || match overview.get() {
                None => view! { <p class="p-6 text-muted">"Working it out…"</p> }.into_any(),
                Some(Err(e)) => view! { <p class="p-6 text-danger">{e.message}</p> }.into_any(),
                Some(Ok(o)) => view! { <Body overview=o /> }.into_any(),
            }}
        </div>
    }
}

/// A count of projects at one health level.
#[component]
fn StatTile(label: &'static str, value: u32, level: HealthLevel) -> impl IntoView {
    let number = if level == HealthLevel::Red && value > 0 {
        "text-danger"
    } else {
        "text-fg"
    };
    view! {
        <div class="rounded-sm border border-line bg-panel px-3 py-2">
            <div class="flex items-center gap-1 text-[10px] font-semibold uppercase tracking-wider text-muted">
                <HealthMark level=level />{label}
            </div>
            <div class=format!("text-[22px] font-semibold leading-7 tabular-nums {number}")>{value}</div>
        </div>
    }
}

#[component]
fn Block(title: String, children: Children) -> impl IntoView {
    view! {
        <section class="border-b border-line">
            <h2 class="px-4 pt-4 pb-1 text-[10px] font-semibold uppercase tracking-wider text-muted">{title}</h2>
            {children()}
        </section>
    }
}

const ROW: &str = "flex items-baseline gap-2 px-4 min-h-8 py-1 cursor-default hover:bg-hover";

#[component]
fn Body(overview: PortfolioOverview) -> impl IntoView {
    let selection = expect_context::<Selection>();
    let open = move |node: NodeRef| move |_| selection.open(node);

    let PortfolioOverview {
        today,
        counts,
        objectives,
        unlinked_projects,
        risks,
        more_risks,
        overloaded,
        stale_waiting,
        warnings,
        thresholds,
        ..
    } = overview;
    let counts_line = counts_text(&counts);
    let (red, amber, green, idle) = (counts.red, counts.amber, counts.green, counts.idle);
    let nothing = objectives.is_empty() && unlinked_projects.is_empty();
    let t = thresholds;
    let legend = format!(
        "Health: late {}/{} working days past the target · blocked or overdue {}/{}% of open tasks · \
         no estimate {}/{}% (amber/red). Change these in Settings.",
        t.late_amber_days,
        t.late_red_days,
        t.risky_amber_pct,
        t.risky_red_pct,
        t.unestimated_amber_pct,
        t.unestimated_red_pct
    );

    let warning_rows = warnings
        .iter()
        .map(|w| view! { <p class="px-4 py-1 text-danger">{w.clone()}</p> })
        .collect_view();
    let risk_rows = risks
        .iter()
        .map(|r| risk_row(r, open(r.node.node)))
        .collect_view();
    let objective_rows = objectives
        .iter()
        .map(|o| objective_block(o, selection))
        .collect_view();
    let unlinked_rows = unlinked_projects
        .iter()
        .map(|p| project_row(p, false, open(p.project.node)))
        .collect_view();
    let people_rows = overloaded
        .iter()
        .map(|p| person_row(p, open(p.person.node)))
        .collect_view();
    let waiting_rows = stale_waiting
        .iter()
        .map(|w| waiting_row(w, open(NodeRef::new(NodeType::WaitingOn, w.waiting.id))))
        .collect_view();
    let (has_risks, has_unlinked, has_people, has_waiting) = (
        !risks.is_empty(),
        !unlinked_projects.is_empty(),
        !overloaded.is_empty(),
        !stale_waiting.is_empty(),
    );
    let more =
        (more_risks > 0).then(|| format!("{more_risks} more at risk, lower down the ranking"));
    let n_waiting = stale_waiting.len();

    view! {
        <PageHeader icon="overview" title="Overview" subtitle="What is at risk and why, without asking">
            <span class="text-muted">{counts_line}</span>
            <span class=format!("{CHIP} ml-auto")>{format!("as of {today}")}</span>
        </PageHeader>
        <div class="grid grid-cols-2 gap-2 border-b border-line px-4 py-3 sm:grid-cols-4">
            <StatTile label="Red" value=red level=HealthLevel::Red />
            <StatTile label="Amber" value=amber level=HealthLevel::Amber />
            <StatTile label="Green" value=green level=HealthLevel::Green />
            <StatTile label="Not scored" value=idle level=HealthLevel::Idle />
        </div>
        {warning_rows}
        {nothing.then(|| view! {
            <EmptyState icon="overview" title="Nothing to show yet"
                hint="Add objectives and projects, link projects to objectives (Links → Contributes to), and give them target dates and task estimates: health appears here." />
        })}
        {has_risks.then(|| view! {
            <Block title="Top risks".to_owned()>
                {risk_rows}
                {more.map(|m| view! { <p class="px-4 py-1 text-[11px] text-muted">{m}</p> })}
            </Block>
        })}
        {(!objectives.is_empty()).then(|| view! {
            <Block title="Objectives".to_owned()>{objective_rows}</Block>
        })}
        {has_unlinked.then(|| view! {
            <Block title="Projects without an objective".to_owned()>{unlinked_rows}</Block>
        })}
        {has_people.then(|| view! {
            <Block title="Overloaded this week".to_owned()>{people_rows}</Block>
        })}
        {has_waiting.then(|| view! {
            <Block title=format!("Stale waiting-ons ({n_waiting})")>{waiting_rows}</Block>
        })}
        <p class="px-4 py-3 text-[11px] text-muted">{legend}</p>
    }
}

fn risk_row(r: &RiskItem, on_click: impl Fn(leptos::ev::MouseEvent) + 'static) -> impl IntoView {
    view! {
        <div class=ROW on:click=on_click>
            <HealthMark level=r.health.level score=r.health.score />
            <span class="w-14 shrink-0 text-[10px] uppercase tracking-wide text-muted">{risk_kind_label(r.kind)}</span>
            <span class="max-w-[18rem] shrink-0 truncate font-medium">{r.node.label.clone()}</span>
            <span class="min-w-0 flex-1 truncate text-muted">{reasons_text(&r.health, 2)}</span>
            <span class="shrink-0 text-[11px] text-muted">{risk_priority_text(r)}</span>
        </div>
    }
}

fn objective_block(o: &ObjectiveHealthRow, selection: Selection) -> impl IntoView {
    let node = o.objective.node;
    let target = o
        .target_date
        .map(|d| format!("target {d}"))
        .unwrap_or_default();
    let projects = o
        .projects
        .iter()
        .map(|p| {
            let n = p.project.node;
            project_row(p, true, move |_| selection.open(n))
        })
        .collect_view();
    view! {
        <div>
            <div class=ROW on:click=move |_| selection.open(node)>
                <HealthMark level=o.health.level score=o.health.score />
                <span class="max-w-[18rem] shrink-0 truncate font-medium">{o.objective.label.clone()}</span>
                <span class="shrink-0 text-[11px] text-muted">
                    {format!("P{} · you say {}", o.priority, objective_status_label(o.status))}
                </span>
                <span class="shrink-0 text-[11px] text-muted">{target}</span>
                <span class="min-w-0 flex-1 truncate text-muted">{reasons_text(&o.health, 2)}</span>
            </div>
            {projects}
        </div>
    }
}

fn project_row(
    p: &ProjectHealthRow,
    nested: bool,
    on_click: impl Fn(leptos::ev::MouseEvent) + 'static,
) -> impl IntoView {
    let class = if nested {
        "flex items-baseline gap-2 pl-9 pr-4 min-h-8 py-1 cursor-default hover:bg-hover"
    } else {
        ROW
    };
    let weight = p.weight.map(|w| format!("weight {w}")).unwrap_or_default();
    view! {
        <div class=class on:click=on_click>
            <HealthMark level=p.health.level score=p.health.score />
            <span class="max-w-[18rem] shrink-0 truncate">{p.project.label.clone()}</span>
            <span class="shrink-0 tabular-nums text-[11px] text-muted">{finish_text(p)}</span>
            <span class="min-w-0 flex-1 truncate text-muted">{reasons_text(&p.health, 2)}</span>
            <span class="shrink-0 text-[11px] text-faint">{weight}</span>
        </div>
    }
}

fn person_row(
    p: &OverloadedPerson,
    on_click: impl Fn(leptos::ev::MouseEvent) + 'static,
) -> impl IntoView {
    view! {
        <div class=ROW on:click=on_click>
            <span class="w-4 shrink-0 text-center text-danger" title="Over capacity">"●"</span>
            <span class="max-w-[18rem] shrink-0 truncate font-medium">{p.person.label.clone()}</span>
            <span class="text-muted">{load_text(p)}</span>
        </div>
    }
}

fn waiting_row(
    w: &WaitingOnRow,
    on_click: impl Fn(leptos::ev::MouseEvent) + 'static,
) -> impl IntoView {
    let overdue = if w.overdue {
        "past its expected date"
    } else {
        ""
    };
    view! {
        <div class=ROW on:click=on_click>
            <span class="w-4 shrink-0 text-center text-fg">"◐"</span>
            <span class="w-28 shrink-0 truncate text-muted">{w.person.label.clone()}</span>
            <span class="min-w-0 flex-1 truncate">{w.waiting.description.clone()}</span>
            <span class="shrink-0 text-[11px] text-muted">
                {format!("waiting {}", age_text(w.age_days))}
                {if overdue.is_empty() { String::new() } else { format!(" · {overdue}") }}
            </span>
        </div>
    }
}
