//! The mark on a task that is waiting on someone (spec 56): who, in a chip that turns amber when
//! any of it is stale. It sits on board cards, list rows and This week rows; the tooltip lists
//! each person, what for and for how long.

use leptos::prelude::*;
use minimap_types::WaitingBadge;

use crate::components::{page::Icon, waiting_panel::age_text};

/// "Raj", or "Raj +1" when more than one person is being waited on (oldest first).
pub fn label(b: &WaitingBadge) -> String {
    match b.entries.as_slice() {
        [] => String::new(),
        [only] => only.person.clone(),
        [first, rest @ ..] => format!("{} +{}", first.person, rest.len()),
    }
}

/// "waiting on Raj, 5d": the label with the age of the oldest, for a list that has the room.
pub fn label_with_age(b: &WaitingBadge) -> String {
    match b.entries.first() {
        Some(oldest) => format!("waiting on {}, {}d", label(b), oldest.age_days),
        None => String::new(),
    }
}

/// One line per waiting-on: "Raj: Sign-off, 12 days (stale)".
pub fn hint(b: &WaitingBadge) -> String {
    b.entries
        .iter()
        .map(|e| {
            let state = if e.stale {
                " (stale)"
            } else if e.snoozed {
                " (snoozed)"
            } else {
                ""
            };
            format!(
                "{}: {}, {}{}",
                e.person,
                e.description,
                age_text(e.age_days),
                state
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The chip, or nothing when the task waits on no one. `with_age` is the longer text.
#[component]
pub fn WaitingChip(badge: Option<WaitingBadge>, #[prop(optional)] with_age: bool) -> impl IntoView {
    badge.map(|b| {
        let colours = if b.stale {
            "border-warning/40 bg-warning/10 text-warning"
        } else {
            "border-line text-muted"
        };
        let text = if with_age {
            label_with_age(&b)
        } else {
            label(&b)
        };
        let title = hint(&b);
        view! {
            <span class=format!("inline-flex max-w-full items-center gap-1 rounded-sm border px-1.5 text-[11px] leading-4 {colours}") title=title>
                <Icon name="waiting" size="h-3 w-3" />
                <span class="truncate">{text}</span>
            </span>
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::WaitingBadgeEntry;

    fn entry(person: &str, age: i64, stale: bool, snoozed: bool) -> WaitingBadgeEntry {
        WaitingBadgeEntry {
            person: person.into(),
            description: "Sign-off".into(),
            age_days: age,
            stale,
            snoozed,
        }
    }

    fn badge(entries: Vec<WaitingBadgeEntry>) -> WaitingBadge {
        let stale = entries.iter().any(|e| e.stale);
        WaitingBadge { entries, stale }
    }

    #[test]
    fn the_label_names_the_oldest_and_counts_the_rest() {
        assert_eq!(label(&badge(vec![entry("Raj", 3, false, false)])), "Raj");
        let two = badge(vec![
            entry("Raj", 12, true, false),
            entry("Ana", 2, false, false),
        ]);
        assert_eq!(label(&two), "Raj +1");
        assert_eq!(label_with_age(&two), "waiting on Raj +1, 12d");
    }

    #[test]
    fn the_hint_lists_each_with_how_long_and_its_state() {
        let b = badge(vec![
            entry("Raj", 12, true, false),
            entry("Ana", 1, false, false),
            entry("Li", 0, false, true),
        ]);
        assert_eq!(
            hint(&b),
            "Raj: Sign-off, 12 days (stale)\nAna: Sign-off, 1 day\nLi: Sign-off, today (snoozed)"
        );
    }
}
