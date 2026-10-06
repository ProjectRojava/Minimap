//! The "Repeats" field of a task or note (spec 27): type `day`, `mon`, `2w`, `2w:fri`, `month` or
//! `month:15`, or leave it empty to stop repeating. A repeating note also has the template its
//! next notes start from.

use leptos::{prelude::*, task::spawn_local};
use minimap_types::{AppError, NodeRef, NodeType, Recurrence};

use crate::{
    api,
    components::form::INPUT,
    state::{DataVersion, Toasts},
};

/// The sentence under the field: what the rule is and what it does.
pub fn help_text(node_type: NodeType, rule: Option<&Recurrence>) -> String {
    match (rule, node_type) {
        (None, _) => "Doesn't repeat. Type day, mon, 2w or month to make it repeat.".to_owned(),
        (Some(r), NodeType::Note) => format!(
            "Repeats {}: a new note is made on each date, with the template below and this note's open checklist items.",
            r.describe()
        ),
        (Some(r), _) => format!(
            "Repeats {}: finishing it makes the next one, with the same details.",
            r.describe()
        ),
    }
}

#[component]
pub fn RepeatField(node: NodeRef, current: Option<Recurrence>) -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    let version = expect_context::<DataVersion>();
    let rule = RwSignal::new(current);
    let shorthand = move || rule.get().map(|r| r.shorthand()).unwrap_or_default();
    let text = RwSignal::new(shorthand());
    let template = RwSignal::new(
        rule.get_untracked()
            .and_then(|r| r.template)
            .unwrap_or_default(),
    );

    let apply = move |result: Result<Option<Recurrence>, AppError>| match result {
        Ok(saved) => {
            rule.set(saved);
            text.set(shorthand());
            version.bump();
        }
        Err(e) => {
            toasts.error(&e);
            text.set(shorthand());
        }
    };
    let commit_rule = move |raw: String| {
        if raw.trim() == shorthand() {
            return;
        }
        spawn_local(async move { apply(api::set_recurrence(node, raw, None).await) });
    };
    let commit_template = move |raw: String| {
        let Some(r) = rule.get_untracked() else {
            return;
        };
        spawn_local(async move {
            apply(api::set_recurrence(node, r.shorthand(), Some(raw)).await);
        });
    };
    let is_note = node.node_type == NodeType::Note;

    view! {
        <label class="mt-2 block">
            <span class="mb-0.5 block text-[11px] text-muted">"Repeats (day, mon, 2w, month)"</span>
            <input class=INPUT type="text" placeholder="no" prop:value=move || text.get()
                on:input=move |ev| text.set(event_target_value(&ev))
                on:change=move |ev| commit_rule(event_target_value(&ev)) />
        </label>
        <p class="mt-1 text-[11px] text-muted">
            {move || help_text(node.node_type, rule.get().as_ref())}
        </p>
        {move || (is_note && rule.get().is_some()).then(|| view! {
            <label class="mt-2 block">
                <span class="mb-0.5 block text-[11px] text-muted">"Template for the next notes"</span>
                <textarea class=INPUT rows="4" prop:value=move || template.get()
                    on:input=move |ev| template.set(event_target_value(&ev))
                    on:change=move |ev| commit_template(event_target_value(&ev))></textarea>
            </label>
        })}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::Cadence;

    #[test]
    fn the_help_says_what_will_happen() {
        assert!(help_text(NodeType::Task, None).starts_with("Doesn't repeat."));
        let weekly: Recurrence = Cadence::Weekly {
            every: 1,
            weekday: 0,
        }
        .into();
        let task = help_text(NodeType::Task, Some(&weekly));
        assert!(
            task.starts_with("Repeats every Monday: finishing it makes the next one"),
            "{task}"
        );
        let note = help_text(NodeType::Note, Some(&weekly));
        assert!(
            note.starts_with("Repeats every Monday: a new note is made on each date"),
            "{note}"
        );
    }
}
