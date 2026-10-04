//! "What if this slips?": build a scenario (one or more tasks or projects running late), see
//! what it pushes (tasks, projects, objectives, people) with before -> after dates, and
//! optionally record it in the plan. Nothing is saved until "Apply to plan" is confirmed.

use leptos::{prelude::*, task::spawn_local};
use minimap_types::{
    ApplyPreview, DateChange, ImpactObjective, ImpactPerson, ImpactProject, ImpactReport,
    ImpactSummary, ImpactTask, NodeRef, NodeType, SearchFilter, SearchHit, Slip,
};

use crate::{
    api,
    components::{
        form::{BUTTON, BUTTON_PRIMARY, COMPACT_INPUT},
        page::{EmptyState, PageHeader, CHIP},
        what_if_button::DEFAULT_SLIP_DAYS,
    },
    nav::type_label,
    state::{finish, DataVersion, Scenario, Selection, Toasts},
    timeline::day_text,
};

// ------------------------------------------------------------------ pure text

fn plural(n: f64, unit: &str) -> String {
    let n = (n * 10.0).round() / 10.0;
    if (n - 1.0).abs() < 1e-9 {
        format!("1 {unit}")
    } else {
        format!("{n} {unit}s")
    }
}

/// "+2 days", "+0.5 days".
pub fn delay_text(days: f64) -> String {
    format!("+{}", plural(days, "day"))
}

fn late_text(late: Option<u32>) -> String {
    late.map_or(String::new(), |n| {
        format!("late by {}", plural(f64::from(n), "working day"))
    })
}

/// The headline under the scenario.
pub fn summary_text(s: &ImpactSummary) -> String {
    let mut parts = Vec::new();
    if s.moved_tasks == 0 {
        parts.push("Nothing finishes later".to_owned());
    } else {
        parts.push(format!(
            "{} move{} (worst {})",
            plural(f64::from(s.moved_tasks), "task"),
            if s.moved_tasks == 1 { "s" } else { "" },
            delay_text(s.worst_delay_days)
        ));
    }
    if s.absorbing_tasks > 0 {
        parts.push(format!(
            "{} absorb{} it",
            plural(f64::from(s.absorbing_tasks), "task"),
            if s.absorbing_tasks == 1 { "s" } else { "" }
        ));
    }
    let mut late = Vec::new();
    if s.newly_late_tasks > 0 {
        late.push(plural(f64::from(s.newly_late_tasks), "task"));
    }
    if s.newly_late_projects > 0 {
        late.push(plural(f64::from(s.newly_late_projects), "project"));
    }
    if s.newly_late_objectives > 0 {
        late.push(plural(f64::from(s.newly_late_objectives), "objective"));
    }
    if !late.is_empty() {
        parts.push(format!("newly late: {}", late.join(", ")));
    }
    if s.people > 0 {
        parts.push(plural(f64::from(s.people), "person").replace("persons", "people"));
    }
    parts.join(" · ")
}

/// "Mon 2027-03-01 → Wed 2027-03-03", or the single date when nothing changes.
fn move_text(old: minimap_types::Date, new: minimap_types::Date) -> String {
    if old == new {
        day_text(old)
    } else {
        format!("{} → {}", day_text(old), day_text(new))
    }
}

/// One line per change "apply" would make.
pub fn change_text(c: &DateChange) -> String {
    let start = match c.old_start {
        Some(old) => format!("starts {} → {}", day_text(old), day_text(c.new_start)),
        None => format!("starts {}", day_text(c.new_start)),
    };
    let due = match (c.old_due, c.new_due) {
        (Some(old), Some(new)) => format!(" · due {} → {}", day_text(old), day_text(new)),
        _ => String::new(),
    };
    format!("{start}{due}")
}

// ------------------------------------------------------------------ the screen

#[component]
pub fn WhatIf() -> impl IntoView {
    let scenario = expect_context::<Scenario>().0;
    let version = expect_context::<DataVersion>();
    let report = LocalResource::new(move || {
        version.track();
        let slips = scenario.get();
        async move {
            if slips.is_empty() {
                Ok(None)
            } else {
                api::run_impact_analysis(slips).await.map(Some)
            }
        }
    });
    let labels = move || -> Vec<String> {
        match report.get() {
            Some(Ok(Some(r))) => r.slips.iter().map(|(n, _)| n.label.clone()).collect(),
            _ => Vec::new(),
        }
    };

    let rows = move || {
        let names = labels();
        scenario
            .get()
            .into_iter()
            .enumerate()
            .map(|(i, slip)| {
                let name = names.get(i).cloned().unwrap_or_else(|| "…".to_owned());
                let kind = type_label(slip.node.node_type);
                let on_days = move |ev| {
                    if let Ok(d) = event_target_value(&ev).trim().parse::<u32>() {
                        scenario.update(|s| {
                            if let Some(x) = s.get_mut(i) {
                                x.days = d.max(1);
                            }
                        });
                    }
                };
                let remove = move |_| {
                    scenario.update(|s| {
                        if i < s.len() {
                            s.remove(i);
                        }
                    })
                };
                view! {
                    <div class="flex items-center gap-3 h-7">
                        <span class="w-16 shrink-0 text-[10px] uppercase tracking-wide text-muted">{kind}</span>
                        <span class="min-w-0 flex-1 truncate font-medium">{name}</span>
                        <label class="flex items-center gap-1 text-muted">
                            "slips"
                            <input type="number" min="1" class=format!("{COMPACT_INPUT} w-16")
                                   prop:value=slip.days.to_string() on:change=on_days />
                            "working days"
                        </label>
                        <button class="rounded px-1.5 text-muted hover:bg-hover" aria-label="Remove"
                                on:click=remove>"✕"</button>
                    </div>
                }
            })
            .collect_view()
    };

    view! {
        <div class="flex flex-col h-full">
            <PageHeader icon="what-if" title="What if this slips?" subtitle="See what a delay would push, before it happens">
                <span class=format!("{CHIP} ml-auto")>"Read-only until you apply it"</span>
            </PageHeader>
            <div class="flex-1 overflow-y-auto">
                <section class="px-4 py-3 border-b border-line">
                    <h2 class="mb-1 text-[11px] font-semibold uppercase tracking-wide text-muted">"Scenario"</h2>
                    {rows}
                    {move || scenario.with(Vec::is_empty).then(|| view! {
                        <p class="mb-2 text-muted">"Add a task or project that might run late."</p>
                    })}
                    <SlipPicker />
                </section>
                {move || match report.get() {
                    None => view! { <p class="p-4 text-muted">"Working it out…"</p> }.into_any(),
                    Some(Err(e)) => view! { <p class="p-4 text-danger">{e.message}</p> }.into_any(),
                    Some(Ok(None)) => view! {
                        <EmptyState icon="what-if" title="Pick something that might slip"
                            hint="Search for a task or project above, or use \u{201c}What if this slips?\u{201d} on any task or project." />
                    }.into_any(),
                    Some(Ok(Some(r))) => view! { <Report report=r /> }.into_any(),
                }}
            </div>
        </div>
    }
}

/// Search box that adds a task or project to the scenario.
#[component]
fn SlipPicker() -> impl IntoView {
    let scenario = expect_context::<Scenario>().0;
    let query = RwSignal::new(String::new());
    let hits = RwSignal::new(Vec::<SearchHit>::new());
    let latest = StoredValue::new(0_u64);
    let on_input = move |ev| {
        let text: String = event_target_value(&ev);
        query.set(text.clone());
        let ticket = latest.get_value() + 1;
        latest.set_value(ticket);
        if text.trim().is_empty() {
            hits.set(Vec::new());
            return;
        }
        let filter = SearchFilter {
            types: vec![NodeType::Task, NodeType::Project],
            limit: Some(8),
            ..Default::default()
        };
        spawn_local(async move {
            if let Ok(found) = api::search(text, filter).await {
                if latest.get_value() == ticket {
                    hits.set(found);
                }
            }
        });
    };
    let add = move |hit: SearchHit| {
        scenario.update(|s| {
            if !s.iter().any(|x| x.node == hit.node) {
                s.push(Slip {
                    node: hit.node,
                    days: DEFAULT_SLIP_DAYS,
                });
            }
        });
        query.set(String::new());
        hits.set(Vec::new());
    };
    let results = move || {
        hits.get()
            .into_iter()
            .map(|h| {
                let picked = h.clone();
                view! {
                    <div class="flex items-baseline gap-2 px-2 h-7 cursor-default hover:bg-hover"
                         on:mousedown=move |ev| { ev.prevent_default(); add(picked.clone()); }>
                        <span class="w-16 shrink-0 text-[10px] uppercase tracking-wide text-muted">
                            {type_label(h.node.node_type)}
                        </span>
                        <span class="truncate">{h.label}</span>
                    </div>
                }
            })
            .collect_view()
    };
    view! {
        <div class="mt-1 max-w-md">
            <input type="search" class=format!("{COMPACT_INPUT} w-full")
                   placeholder="Add a task or project…" autocomplete="off"
                   prop:value=move || query.get() on:input=on_input />
            <div class="rounded-sm border-line">{results}</div>
        </div>
    }
}

#[component]
fn Report(report: ImpactReport) -> impl IntoView {
    let selection = expect_context::<Selection>();
    let summary = summary_text(&report.summary);
    let nothing = report.tasks.is_empty();
    let ImpactReport {
        tasks,
        projects,
        objectives,
        people,
        ..
    } = report;
    let open = move |node: NodeRef| move |_| selection.open(node);

    let task_rows = tasks
        .iter()
        .map(|t| task_row(t, open(NodeRef::new(NodeType::Task, t.id))))
        .collect_view();
    let project_rows = projects
        .iter()
        .map(|p| project_row(p, open(NodeRef::new(NodeType::Project, p.id))))
        .collect_view();
    let objective_rows = objectives
        .iter()
        .map(|o| objective_row(o, open(NodeRef::new(NodeType::Objective, o.id))))
        .collect_view();
    let people_rows = people
        .iter()
        .map(|p| person_row(p, open(NodeRef::new(NodeType::Person, p.id))))
        .collect_view();
    let (has_projects, has_objectives, has_people) = (
        !projects.is_empty(),
        !objectives.is_empty(),
        !people.is_empty(),
    );
    let (n_tasks, n_projects, n_objectives, n_people) =
        (tasks.len(), projects.len(), objectives.len(), people.len());

    view! {
        <section class="px-4 py-3 border-b border-line">
            <p class="font-medium">{summary}</p>
            <ApplyBar />
        </section>
        {(!nothing).then(|| view! {
            <Group title=format!("Tasks ({n_tasks})")>{task_rows}</Group>
        })}
        {has_projects.then(|| view! {
            <Group title=format!("Projects ({n_projects})")>{project_rows}</Group>
        })}
        {has_objectives.then(|| view! {
            <Group title=format!("Objectives ({n_objectives})")>{objective_rows}</Group>
        })}
        {has_people.then(|| view! {
            <Group title=format!("People ({n_people})")>{people_rows}</Group>
        })}
        <p class="px-4 py-3 text-[11px] text-muted">
            "Working days only. A task that absorbs the slip uses up slack and stops it there. \u{201c}Newly late\u{201d} means on time now, late after the slip."
        </p>
    }
}

#[component]
fn Group(title: String, children: Children) -> impl IntoView {
    view! {
        <section class="border-b border-line">
            <h2 class="px-4 pt-3 pb-1 text-[11px] font-semibold uppercase tracking-wide text-muted">{title}</h2>
            {children()}
        </section>
    }
}

const ROW: &str = "grid w-full items-center gap-3 px-4 min-h-7 cursor-default hover:bg-hover \
                   grid-cols-[minmax(0,1fr)_16rem_14rem]";

fn task_row(t: &ImpactTask, on_click: impl Fn(leptos::ev::MouseEvent) + 'static) -> impl IntoView {
    let effect = if t.delay_days > 1e-9 {
        delay_text(t.delay_days)
    } else {
        format!("absorbed {}: stops here", plural(t.absorbed_days, "day"))
    };
    let absorbed = (t.delay_days > 1e-9 && t.absorbed_days > 1e-9)
        .then(|| format!("(absorbed {})", plural(t.absorbed_days, "day")));
    let late = late_text(t.late_after);
    let is_late = t.late_after.is_some();
    let effect_class = if t.delay_days > 1e-9 {
        ""
    } else {
        "text-muted"
    };
    view! {
        <div class=ROW on:click=on_click>
            <span class="truncate">
                <span class="font-medium">{t.title.clone()}</span>
                {t.direct.then(|| view! { <span class="ml-1 text-[11px] text-muted">"slipped"</span> })}
                <span class="ml-2 text-muted">{t.project_title.clone().unwrap_or_default()}</span>
            </span>
            <span class="tabular-nums text-muted">{move_text(t.old_finish, t.new_finish)}</span>
            <span class="truncate">
                <span class=effect_class>{effect}</span>
                <span class="ml-1 text-muted">{absorbed}</span>
                {is_late.then(|| view! {
                    <span class="ml-2 text-danger">{late}{if t.newly_late { " (new)" } else { "" }}</span>
                })}
            </span>
        </div>
    }
}

fn project_row(
    p: &ImpactProject,
    on_click: impl Fn(leptos::ev::MouseEvent) + 'static,
) -> impl IntoView {
    let dates = match (p.old_finish, p.new_finish) {
        (Some(a), Some(b)) => move_text(a, b),
        _ => String::new(),
    };
    let target = p
        .target_date
        .map(|d| format!("target {d}"))
        .unwrap_or_default();
    let late = late_text(p.late_after);
    let is_late = p.late_after.is_some();
    view! {
        <div class=ROW on:click=on_click>
            <span class="truncate">
                <span class="font-medium">{p.title.clone()}</span>
                {p.direct.then(|| view! { <span class="ml-1 text-[11px] text-muted">"slipped"</span> })}
                <span class="ml-2 text-muted">{target}</span>
            </span>
            <span class="tabular-nums text-muted">{dates}</span>
            <span class="truncate">
                {delay_text(p.delay_days)}
                {is_late.then(|| view! {
                    <span class="ml-2 text-danger">{late}{if p.newly_late { " (new)" } else { "" }}</span>
                })}
            </span>
        </div>
    }
}

fn objective_row(
    o: &ImpactObjective,
    on_click: impl Fn(leptos::ev::MouseEvent) + 'static,
) -> impl IntoView {
    let dates = match (o.old_finish, o.new_finish) {
        (Some(a), Some(b)) => move_text(a, b),
        _ => String::new(),
    };
    let via = format!("via {}", o.contributors.join(", "));
    let late = late_text(o.late_after);
    let is_late = o.late_after.is_some();
    let target = o
        .target_date
        .map(|d| format!("target {d}"))
        .unwrap_or_default();
    view! {
        <div class=ROW on:click=on_click>
            <span class="truncate">
                <span class="font-medium">{o.title.clone()}</span>
                <span class="ml-2 text-muted">{target}</span>
                <span class="ml-2 text-muted">{via}</span>
            </span>
            <span class="tabular-nums text-muted">{dates}</span>
            <span class="truncate">
                {delay_text(o.delay_days)}
                {is_late.then(|| view! {
                    <span class="ml-2 text-danger">{late}{if o.newly_late { " (new)" } else { "" }}</span>
                })}
            </span>
        </div>
    }
}

fn person_row(
    p: &ImpactPerson,
    on_click: impl Fn(leptos::ev::MouseEvent) + 'static,
) -> impl IntoView {
    let tasks = p
        .tasks
        .iter()
        .map(|t| format!("{} ({})", t.title, delay_text(t.delay_days)))
        .collect::<Vec<_>>()
        .join(", ");
    view! {
        <div class=ROW on:click=on_click>
            <span class="truncate font-medium">{p.name.clone()}</span>
            <span class="truncate text-muted">{plural(p.tasks.len() as f64, "task")}</span>
            <span class="truncate" title=tasks.clone()>{format!("{} · {tasks}", delay_text(p.delay_days))}</span>
        </div>
    }
}

/// "Apply to plan…": shows exactly which dates would change, then asks.
#[component]
fn ApplyBar() -> impl IntoView {
    let scenario = expect_context::<Scenario>().0;
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let preview = RwSignal::new(Option::<ApplyPreview>::None);

    let ask = move |_| {
        let slips = scenario.get_untracked();
        spawn_local(async move {
            match api::preview_apply_slips(slips).await {
                Ok(p) => preview.set(Some(p)),
                Err(e) => toasts.error(&e),
            }
        });
    };
    let apply = move |_| {
        let slips = scenario.get_untracked();
        spawn_local(async move {
            if let Some(done) = finish(api::apply_slips(slips).await, toasts, version) {
                preview.set(None);
                scenario.set(Vec::new());
                toasts.info(format!(
                    "Recorded in the plan: {}",
                    plural(f64::from(done.tasks_changed), "task")
                ));
            }
        });
    };
    let body = move || match preview.get() {
        None => view! {
            <button class=format!("{BUTTON} mt-2") on:click=ask>"Apply to plan…"</button>
        }
        .into_any(),
        Some(p) if p.changes.is_empty() => view! {
            <p class="mt-2 text-muted">"Nothing to record."</p>
        }
        .into_any(),
        Some(p) => {
            let n = p.changes.len();
            let lines = p
                .changes
                .iter()
                .map(|c| {
                    view! {
                        <li class="truncate">
                            <span class="font-medium">{c.title.clone()}</span>
                            <span class="ml-2 text-muted">{change_text(c)}</span>
                        </li>
                    }
                })
                .collect_view();
            view! {
                    <div class="mt-2 space-y-2">
                        <p>
                            "Recording this sets a start date on "{plural(n as f64, "task")}
                            " and moves their due dates. Targets are not changed, and tasks pushed along by links move by themselves."
                        </p>
                        <ul class="space-y-px">{lines}</ul>
                        <div class="flex gap-2">
                            <button class=BUTTON_PRIMARY on:click=apply>"Apply these changes"</button>
                            <button class=BUTTON on:click=move |_| preview.set(None)>"Cancel"</button>
                        </div>
                    </div>
                }
                .into_any()
        }
    };
    view! { {body} }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::date;

    fn summary() -> ImpactSummary {
        ImpactSummary {
            moved_tasks: 3,
            absorbing_tasks: 1,
            newly_late_tasks: 1,
            newly_late_projects: 1,
            newly_late_objectives: 0,
            people: 2,
            worst_delay_days: 5.0,
        }
    }

    #[test]
    fn delays_read_naturally() {
        assert_eq!(delay_text(1.0), "+1 day");
        assert_eq!(delay_text(5.0), "+5 days");
        assert_eq!(delay_text(0.5), "+0.5 days");
    }

    #[test]
    fn the_headline_lists_what_matters() {
        assert_eq!(
            summary_text(&summary()),
            "3 tasks move (worst +5 days) · 1 task absorbs it · newly late: 1 task, 1 project · 2 people"
        );
        let mut one = summary();
        one.moved_tasks = 1;
        one.absorbing_tasks = 0;
        one.newly_late_tasks = 0;
        one.newly_late_projects = 0;
        one.people = 1;
        one.worst_delay_days = 1.0;
        assert_eq!(summary_text(&one), "1 task moves (worst +1 day) · 1 person");
        let mut none = summary();
        none.moved_tasks = 0;
        none.absorbing_tasks = 2;
        none.newly_late_tasks = 0;
        none.newly_late_projects = 0;
        none.people = 0;
        assert_eq!(
            summary_text(&none),
            "Nothing finishes later · 2 tasks absorb it"
        );
    }

    #[test]
    fn changes_show_before_and_after() {
        let c = DateChange {
            task_id: minimap_types::Uuid::nil(),
            title: "A".into(),
            old_start: None,
            new_start: date!(2027 - 03 - 03),
            old_due: Some(date!(2027 - 03 - 05)),
            new_due: Some(date!(2027 - 03 - 09)),
        };
        assert_eq!(
            change_text(&c),
            "starts Wed 2027-03-03 · due Fri 2027-03-05 → Tue 2027-03-09"
        );
        let c = DateChange {
            old_due: None,
            new_due: None,
            old_start: Some(date!(2027 - 03 - 01)),
            ..c
        };
        assert_eq!(change_text(&c), "starts Mon 2027-03-01 → Wed 2027-03-03");
    }

    #[test]
    fn moves_collapse_when_nothing_changes() {
        let d = date!(2027 - 03 - 01);
        assert_eq!(move_text(d, d), "Mon 2027-03-01");
        assert_eq!(
            move_text(d, date!(2027 - 03 - 02)),
            "Mon 2027-03-01 → Tue 2027-03-02"
        );
    }
}
