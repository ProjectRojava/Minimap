//! A row for an ongoing objective whose review is due (spec 30), on This week and in the weekly
//! review: its name, how late (or when) the review falls, and *Mark reviewed*.

use leptos::{prelude::*, task::spawn_local};
use minimap_types::{timefmt::parse_date, Patch, ReviewDue, UpdateObjective};

use crate::{
    api,
    calendar::format_ymd,
    components::{date_field::today_ymd, form::BUTTON_SOFT, page::Tone},
    state::{finish, DataVersion, Selection, Toasts},
};

/// "overdue 12d" or "due 2027-03-05", with its tone.
pub fn due_text(item: &ReviewDue) -> (Tone, String) {
    match item.overdue_days {
        Some(n) => (Tone::Danger, format!("overdue {n}d")),
        None => (Tone::Warning, format!("due {}", item.due)),
    }
}

#[component]
pub fn ReviewRow(item: ReviewDue) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let selection = expect_context::<Selection>();
    let id = item.objective.node.id;
    let node = item.objective.node;
    let (tone, text) = due_text(&item);
    let reviewed = move |_| {
        let (y, m, d) = today_ymd();
        let Ok(today) = parse_date(&format_ymd(y, m, d)) else {
            return;
        };
        spawn_local(async move {
            let patch = UpdateObjective {
                last_reviewed_on: Patch::Set(today),
                ..Default::default()
            };
            finish(api::update_objective(id, patch).await, toasts, version);
        });
    };
    view! {
        <div class="flex items-center gap-3 border-b border-line px-4 h-8">
            <button class="min-w-0 flex-1 truncate text-left font-medium hover:underline"
                    on:click=move |_| selection.open(node)>{item.objective.label}</button>
            <span class=tone.chip()>{text}</span>
            <button class=BUTTON_SOFT on:click=reviewed>"Mark reviewed"</button>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{NodeRef, NodeSummary, NodeType, Uuid};
    use time::macros::date;

    fn item(overdue: Option<u32>) -> ReviewDue {
        ReviewDue {
            objective: NodeSummary {
                node: NodeRef::new(NodeType::Objective, Uuid::nil()),
                label: "Maintenance".into(),
                archived: false,
            },
            due: date!(2027 - 03 - 05),
            overdue_days: overdue,
        }
    }

    #[test]
    fn the_pill_says_how_late_or_when() {
        assert_eq!(
            due_text(&item(Some(12))),
            (Tone::Danger, "overdue 12d".to_owned())
        );
        assert_eq!(
            due_text(&item(None)),
            (Tone::Warning, "due 2027-03-05".to_owned())
        );
    }
}
