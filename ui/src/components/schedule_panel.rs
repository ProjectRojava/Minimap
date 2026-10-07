//! The project's schedule: projected finish against the target, the critical path, and a
//! timeline (Days or Weeks) with the critical tasks highlighted. The numbers come from
//! `get_schedule`; layout is `timeline.rs`.

use leptos::prelude::*;
use minimap_types::{NodeRef, NodeType, Schedule, ScheduleScope, ScheduledTask, Uuid};

use crate::{
    api,
    components::{
        detail_pane::Section,
        form::{BUTTON, BUTTON_ON},
        people_panel::error_line,
    },
    state::{DataVersion, Selection},
    timeline::{
        day_text, describe, forecast_text, layout, visible_tasks, BarKind, Granularity, TickKind,
        HEADER_H, ROW_H,
    },
};

#[component]
pub fn SchedulePanel(project: Uuid) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let schedule = LocalResource::new(move || {
        version.track();
        api::get_schedule(ScheduleScope::Project(project))
    });
    view! {
        <Section title="Schedule">
            {move || match schedule.get() {
                Some(Ok(s)) => view! { <ScheduleBody schedule=s /> }.into_any(),
                Some(Err(e)) => error_line(e),
                None => view! { <p class="text-muted">"Loading…"</p> }.into_any(),
            }}
        </Section>
    }
}

/// A small triangle pointing down at `x`, sitting on the line under the date axis.
fn marker_path(x: f64) -> String {
    let y = crate::timeline::HEADER_H;
    format!(
        "M{} {}L{} {}L{} {}Z",
        x - 4.0,
        y - 7.0,
        x + 4.0,
        y - 7.0,
        x,
        y
    )
}

fn button_class(active: bool) -> String {
    format!("{BUTTON} {}", if active { BUTTON_ON } else { "" })
}

#[component]
fn ScheduleBody(schedule: Schedule) -> impl IntoView {
    let selection = expect_context::<Selection>();
    let granularity = RwSignal::new(Granularity::Days);
    let show_done = RwSignal::new(false);

    let forecast = schedule.projects.first().cloned();
    let summary = forecast.as_ref().map(forecast_text).unwrap_or_default();
    let late = forecast.as_ref().is_some_and(|f| f.late_by_days.is_some());
    let target_offset = forecast.as_ref().and_then(|f| f.target_offset);
    let open_count = schedule.tasks.iter().filter(|t| !t.done).count();
    // With no task at all there is nothing to draw: the message says what to do.
    let nothing_at_all = schedule.tasks.is_empty();

    let critical: Vec<ScheduledTask> = schedule
        .tasks
        .iter()
        .filter(|t| t.critical)
        .cloned()
        .collect();
    let critical_rows = critical
        .into_iter()
        .map(|t| {
            let node = NodeRef::new(NodeType::Task, t.id);
            let dates = format!("{} → {}", day_text(t.start), day_text(t.finish));
            view! {
                <li>
                    <button class="flex w-full gap-2 rounded-sm px-1 py-0.5 text-left hover:bg-hover"
                            on:click=move |_| selection.open(node)>
                        <span class="w-[11.5rem] shrink-0 tabular-nums text-muted">{dates}</span>
                        <span class="truncate">{t.title}</span>
                    </button>
                </li>
            }
        })
        .collect_view();
    let has_critical = schedule.tasks.iter().any(|t| t.critical);

    let schedule = StoredValue::new(schedule);
    // Rows (left) and the drawing (right) come from the same visible list so they line up.
    let rows = move || {
        schedule.with_value(|s| {
            visible_tasks(s, show_done.get())
                .into_iter()
                .map(|t| {
                    let tip = describe(t);
                    let class = if t.critical {
                        "block truncate px-1 font-semibold"
                    } else if t.done {
                        "block truncate px-1 text-faint"
                    } else {
                        "block truncate px-1"
                    };
                    let id = t.id;
                    let title = t.title.clone();
                    view! {
                        <button class=class style=format!("height:{ROW_H}px;line-height:{ROW_H}px")
                                title=tip
                                on:click=move |_| selection.open(NodeRef::new(NodeType::Task, id))>
                            {title}
                        </button>
                    }
                })
                .collect_view()
        })
    };
    let drawing = move || {
        schedule.with_value(|s| {
            let l = layout(s, granularity.get(), show_done.get(), target_offset);
            let weeks = granularity.get() == Granularity::Weeks;
            let ticks = l
                .ticks
                .iter()
                .map(|t| {
                    let x = t.x;
                    let h = l.height;
                    match t.kind {
                        TickKind::Week => view! {
                            <line class="week-line" x1=x x2=x y1=0 y2=h />
                            <text x=x + 2.0 y=if weeks { 20.0 } else { 11.0 }>{t.label.clone()}</text>
                        }
                        .into_any(),
                        TickKind::Day if t.label.is_empty() => {
                            view! { <line class="day-line" x1=x x2=x y1=HEADER_H y2=h /> }.into_any()
                        }
                        TickKind::Day => view! {
                            <text x=x y=HEADER_H - 6.0 text-anchor="middle">{t.label.clone()}</text>
                        }
                        .into_any(),
                    }
                })
                .collect_view();
            let bars = l
                .bars
                .iter()
                .map(|b| {
                    let id = b.id;
                    let class = format!(
                        "bar {}{}{}{}",
                        match b.kind {
                            BarKind::Critical => "critical",
                            BarKind::Normal => "normal",
                            BarKind::Done => "done",
                        },
                        if b.late { " late" } else { "" },
                        if b.unestimated { " unestimated" } else { "" },
                        if b.summary { " summary" } else { "" },
                    );
                    let slack = b.slack_end.map(|end| {
                        let y = b.y + crate::timeline::BAR_H / 2.0;
                        view! { <line class="slack" x1=b.x + b.w x2=end y1=y y2=y /> }
                    });
                    // A group is a thin bracket along the top of its row, over the work in it.
                    let (y, height) = if b.summary {
                        (b.y + crate::timeline::BAR_H / 2.0 - 2.0, 4.0)
                    } else {
                        (b.y, crate::timeline::BAR_H)
                    };
                    view! {
                        <rect class=class x=b.x y=y width=b.w height=height rx="1"
                              on:click=move |_| selection.open(NodeRef::new(NodeType::Task, id)) />
                        {slack}
                    }
                })
                .collect_view();
            let target = l.target_x.map(|x| {
                view! {
                    <line class="target" x1=x x2=x y1=HEADER_H y2=l.height />
                    <path class="target-marker" d=marker_path(x)>
                        <title>"Target date"</title>
                    </path>
                }
            });
            view! {
                <svg class="gantt" width=l.width height=l.height>
                    <line class="head-line" x1=0 x2=l.width y1=HEADER_H y2=HEADER_H />
                    {ticks}
                    <line class="today" x1=l.today_x x2=l.today_x y1=HEADER_H y2=l.height />
                    // A marker on the axis instead of a word, so it never lands on the dates.
                    <path class="today-marker" d=marker_path(l.today_x)>
                        <title>"Today"</title>
                    </path>
                    {target}
                    {bars}
                </svg>
            }
        })
    };

    view! {
        <p class=if late { "mb-2 font-medium text-danger" } else { "mb-2" }>{summary}</p>
        {(!has_critical && open_count > 0).then(|| view! { <p class="mb-2 text-muted">"No critical tasks."</p> })}
        {has_critical.then(|| view! {
            <h4 class="mb-1 text-[11px] uppercase tracking-wide text-muted">"Critical path"</h4>
            <ul class="mb-3 space-y-px">{critical_rows}</ul>
        })}
        {(open_count == 0).then(|| view! {
            <p class="text-muted">"Nothing open to schedule. Add tasks with estimates and \"blocks\" links."</p>
        })}
        {(!nothing_at_all).then(|| view! {
        <div class="mb-2 flex flex-wrap items-center gap-2">
            {[Granularity::Days, Granularity::Weeks].into_iter().map(|g| view! {
                <button class=move || button_class(granularity.get() == g)
                        on:click=move |_| granularity.set(g)>{g.label()}</button>
            }).collect_view()}
            <label class="ml-2 flex items-center gap-1 text-[11px] text-muted">
                <input type="checkbox" prop:checked=move || show_done.get()
                       on:change=move |ev| show_done.set(event_target_checked(&ev)) />
                "Show finished"
            </label>
        </div>
        <div class="flex">
            <div class="w-[8.5rem] shrink-0 border-r border-line"
                 style=format!("padding-top:{HEADER_H}px")>
                {rows}
            </div>
            <div class="min-w-0 flex-1 overflow-x-auto">{drawing}</div>
        </div>
        <p class="mt-1 text-[11px] text-muted">
            "Working days only. Solid bars are tasks; the dashed line after one is its slack. Dashed outlines have no estimate. The blue marker is today and the red one the target date."
        </p>
        })}
    }
}
