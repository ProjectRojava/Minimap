//! The strip of coloured pills at the top of a task, project or objective panel: where it
//! stands (status), how much it matters (priority), whether its date has passed, and the
//! objectives it serves. The same tones as the lists, so the panel can be read at a glance
//! without going through the fields.

use leptos::prelude::*;
use minimap_types::{Date, Uuid};

use crate::{
    api,
    components::{
        objective_colour::{use_objective_colours, ObjectiveChips},
        page::Tone,
        task_board::today,
    },
    labels::{
        date_tone, objective_status_label, objective_status_tone, priority_short, priority_tone,
        project_status_label, project_status_tone, task_status_label, task_status_tone,
    },
    state::DataVersion,
};

/// "due 2027-03-09", "due today" or "overdue 2027-03-01", for the date's tone.
fn date_text(what: &str, date: Date, tone: Tone, today: Option<Date>) -> String {
    match tone {
        Tone::Danger => format!("overdue {date}"),
        Tone::Warning if Some(date) == today => format!("{what} today"),
        _ => format!("{what} {date}"),
    }
}

#[component]
fn Pill(tone: Tone, children: Children) -> impl IntoView {
    view! { <span class=tone.chip()>{children()}</span> }
}

/// The objectives a project serves, as coloured chips (nothing when it serves none).
#[component]
fn ServedObjectives(project: Option<Uuid>) -> impl IntoView {
    let colours = use_objective_colours();
    move || {
        let objectives = colours.of_project(project);
        (!objectives.is_empty()).then(|| view! { <ObjectiveChips objectives=objectives /> })
    }
}

#[component]
pub fn TaskSummary(id: Uuid) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let task = LocalResource::new(move || {
        version.track();
        api::get_task(id)
    });
    move || {
        task.get().and_then(|r| r.ok()).map(|t| {
            let open = !matches!(
                t.status,
                minimap_types::TaskStatus::Done | minimap_types::TaskStatus::Cancelled
            );
            let due = t.due_date.map(|d| {
                let today = today();
                let tone = date_tone(d, today, open);
                (tone, date_text("due", d, tone, today))
            });
            view! {
                <div class="mb-3 flex flex-wrap items-center gap-1.5">
                    <Pill tone=task_status_tone(t.status)>{task_status_label(t.status)}</Pill>
                    <Pill tone=priority_tone(t.priority)>{priority_short(t.priority)}</Pill>
                    {due.map(|(tone, text)| view! { <Pill tone=tone>{text}</Pill> })}
                    <ServedObjectives project=t.project_id />
                </div>
            }
        })
    }
}

#[component]
pub fn ProjectSummary(id: Uuid) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let project = LocalResource::new(move || {
        version.track();
        api::get_project(id)
    });
    move || {
        project.get().and_then(|r| r.ok()).map(|p| {
            let open = matches!(
                p.status,
                minimap_types::ProjectStatus::Planned
                    | minimap_types::ProjectStatus::Active
                    | minimap_types::ProjectStatus::Paused
            );
            let target = p.target_date.map(|d| {
                let today = today();
                let tone = date_tone(d, today, open);
                (tone, date_text("target", d, tone, today))
            });
            view! {
                <div class="mb-3 flex flex-wrap items-center gap-1.5">
                    <Pill tone=project_status_tone(p.status)>{project_status_label(p.status)}</Pill>
                    <Pill tone=priority_tone(p.priority)>{priority_short(p.priority)}</Pill>
                    {target.map(|(tone, text)| view! { <Pill tone=tone>{text}</Pill> })}
                    <ServedObjectives project=Some(p.id) />
                </div>
            }
        })
    }
}

#[component]
pub fn ObjectiveSummary(id: Uuid) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let objective = LocalResource::new(move || {
        version.track();
        api::get_objective(id)
    });
    move || {
        objective.get().and_then(|r| r.ok()).map(|o| {
            let open = o.status != minimap_types::ObjectiveStatus::Done;
            let target = o.target_date.map(|d| {
                let today = today();
                let tone = date_tone(d, today, open);
                (tone, date_text("target", d, tone, today))
            });
            view! {
                <div class="mb-3 flex flex-wrap items-center gap-1.5">
                    <Pill tone=objective_status_tone(o.status)>{objective_status_label(o.status)}</Pill>
                    <Pill tone=priority_tone(o.priority)>{priority_short(o.priority)}</Pill>
                    {target.map(|(tone, text)| view! { <Pill tone=tone>{text}</Pill> })}
                </div>
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(day: u8) -> Date {
        Date::from_calendar_date(2027, time::Month::March, day).unwrap()
    }

    #[test]
    fn the_date_pill_says_what_is_wrong_with_it() {
        let today = Some(d(3));
        assert_eq!(
            date_text("due", d(1), Tone::Danger, today),
            "overdue 2027-03-01"
        );
        assert_eq!(date_text("due", d(3), Tone::Warning, today), "due today");
        assert_eq!(
            date_text("target", d(9), Tone::Neutral, today),
            "target 2027-03-09"
        );
    }
}
