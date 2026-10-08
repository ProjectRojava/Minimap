//! Detail-pane body for a decision: status and date, the write-up, archive. What it affects and
//! what it replaces are links (the generic Links section below).

use leptos::{prelude::*, task::spawn_local};
use minimap_types::{Decision, DecisionStatus, UpdateDecision, Uuid};

use crate::{
    api,
    components::{
        detail_pane::Section,
        form::{date_patch, SelectField, TextField, BUTTON, BUTTON_DANGER},
        markdown_box::{saver, MarkdownField},
        people_panel::error_line,
    },
    labels::{decision_status_label, DECISION_STATUS_TINT},
    state::{finish, DataVersion, Selection, Toasts},
};

pub fn status_options() -> Vec<(String, String)> {
    DecisionStatus::ALL
        .iter()
        .map(|s| (s.as_str().to_owned(), decision_status_label(*s).to_owned()))
        .collect()
}

#[component]
pub fn DecisionPanel(id: Uuid) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    // The write-up loads once per opening so typing is never overwritten. Status and date follow
    // every write: superseding from the Links section changes the status, and deciding stamps a date.
    let text = LocalResource::new(move || api::get_decision(id));
    let state = LocalResource::new(move || {
        version.track();
        api::get_decision(id)
    });

    view! {
        <Section title="Status" always_open=true>
            {move || match state.get() {
                Some(Ok(d)) => view! { <StatusFields decision=d /> }.into_any(),
                Some(Err(e)) => error_line(e),
                None => view! { <p class="text-muted">"Loading…"</p> }.into_any(),
            }}
        </Section>
        <Section title="Decision" always_open=true>
            {move || match text.get() {
                Some(Ok(d)) => view! { <WriteUp decision=d /> }.into_any(),
                Some(Err(e)) => error_line(e),
                None => view! { <p class="text-muted">"Loading…"</p> }.into_any(),
            }}
        </Section>
        <ArchiveDecision id=id />
    }
}

#[component]
fn StatusFields(decision: Decision) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let id = decision.id;
    let save = move |patch: UpdateDecision| {
        spawn_local(async move {
            finish(api::update_decision(id, patch).await, toasts, version);
        });
    };
    let save_status = move |v: String| {
        if let Ok(status) = v.parse::<DecisionStatus>() {
            save(UpdateDecision {
                status: Some(status),
                ..Default::default()
            });
        }
    };
    let save_date = move |v: String| match date_patch(&v) {
        Ok(p) => save(UpdateDecision {
            decided_on: p,
            ..Default::default()
        }),
        Err(e) => {
            toasts.error(&e);
            version.bump();
        }
    };
    view! {
        <div class="grid grid-cols-2 gap-3">
            <SelectField label="Status" options=status_options()
                current=decision.status.as_str().to_owned() on_change=save_status
                tint=DECISION_STATUS_TINT />
            <TextField label="Decided on" kind="date"
                value=decision.decided_on.map(|d| d.to_string()).unwrap_or_default() on_commit=save_date />
        </div>
        <p class="text-[11px] text-muted">
            "Deciding stamps today's date. Link what it affects and what it replaces under Links (Affects, Supersedes)."
        </p>
    }
}

#[component]
fn WriteUp(decision: Decision) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let id = decision.id;
    let save = move |patch: UpdateDecision| {
        spawn_local(async move {
            finish(api::update_decision(id, patch).await, toasts, version);
        });
    };
    view! {
        <TextField label="Title" value=decision.title.clone()
            on_commit=move |v: String| save(UpdateDecision { title: Some(v), ..Default::default() }) />
        <MarkdownField label="Context: what prompted this?" value=decision.context.clone() node=minimap_types::NodeRef::new(minimap_types::NodeType::Decision, id)
            empty="Nothing written yet." rows=5
            save=saver(move |v: String| async move {
                finish(api::update_decision(id, UpdateDecision { context: Some(v), ..Default::default() }).await, toasts, version).is_some()
            }) />
        <MarkdownField label="Decision: what was decided?" value=decision.decision.clone() node=minimap_types::NodeRef::new(minimap_types::NodeType::Decision, id)
            empty="Nothing written yet." rows=5
            save=saver(move |v: String| async move {
                finish(api::update_decision(id, UpdateDecision { decision: Some(v), ..Default::default() }).await, toasts, version).is_some()
            }) />
        <MarkdownField label="Rationale: why?" value=decision.rationale.clone() node=minimap_types::NodeRef::new(minimap_types::NodeType::Decision, id)
            empty="Nothing written yet." rows=5
            save=saver(move |v: String| async move {
                finish(api::update_decision(id, UpdateDecision { rationale: Some(v), ..Default::default() }).await, toasts, version).is_some()
            }) />
    }
}

#[component]
fn ArchiveDecision(id: Uuid) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let selection = expect_context::<Selection>();
    let confirming = RwSignal::new(false);
    let confirm = move |_| {
        spawn_local(async move {
            if finish(api::archive_decision(id).await, toasts, version).is_some() {
                confirming.set(false);
                selection.close();
            }
        });
    };
    view! {
        <Section title="Archive" collapsed=true tone=crate::components::page::Tone::Danger>
            {move || if confirming.get() {
                view! {
                    <div class="space-y-2">
                        <p>"Archive this decision? Its links are archived with it."</p>
                        <div class="flex gap-2">
                            <button class=BUTTON_DANGER on:click=confirm>"Archive"</button>
                            <button class=BUTTON on:click=move |_| confirming.set(false)>"Cancel"</button>
                        </div>
                    </div>
                }.into_any()
            } else {
                view! { <button class=BUTTON on:click=move |_| confirming.set(true)>"Archive…"</button> }.into_any()
            }}
        </Section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_status_is_offered_once() {
        let options = status_options();
        assert_eq!(options.len(), DecisionStatus::ALL.len());
        for s in DecisionStatus::ALL {
            assert!(options.iter().any(|(v, _)| v == s.as_str()));
        }
    }
}
