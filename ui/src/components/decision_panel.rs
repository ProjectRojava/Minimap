//! Detail-pane body for a decision: status and date, the write-up, archive. What it affects and
//! what it replaces are links (the generic Links section below).

use leptos::{prelude::*, task::spawn_local};
use minimap_types::{Decision, DecisionStatus, UpdateDecision, Uuid};

use crate::{
    api,
    components::{
        confirm_archive::{ConfirmArchive, UNDO_HINT},
        detail_pane::Section,
        form::{date_patch, SelectField, TextField, BUTTON_DANGER},
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
    view! {
        <Section title="Status" always_open=true>
            <StatusPart id=id />
        </Section>
        <Section title="Decision" always_open=true>
            <WritePart id=id />
        </Section>
        <ArchiveDecision id=id />
    }
}

/// Status and date. They follow every write: superseding from the Links section changes the
/// status, and deciding stamps a date. Also shown in a decision task's own pane (spec 41).
#[component]
pub fn StatusPart(id: Uuid) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let state = LocalResource::new(move || {
        version.track();
        api::get_decision(id)
    });
    move || match state.get() {
        Some(Ok(d)) => view! { <StatusFields decision=d /> }.into_any(),
        Some(Err(e)) => error_line(e),
        None => view! { <p class="text-muted">"Loading…"</p> }.into_any(),
    }
}

/// The write-up. It loads once per opening so typing is never overwritten.
#[component]
pub fn WritePart(id: Uuid) -> impl IntoView {
    let text = LocalResource::new(move || api::get_decision(id));
    move || match text.get() {
        Some(Ok(d)) => view! { <WriteUp decision=d /> }.into_any(),
        Some(Err(e)) => error_line(e),
        None => view! { <p class="text-muted">"Loading…"</p> }.into_any(),
    }
}

/// Who set the status and the date: `(true, ..)` is Minimap (finishing the task, or the date
/// stamped when a decision is decided without one), `(false, ..)` is the user. A proposed decision
/// with no date has nothing to say yet.
pub fn provenance(d: &Decision) -> Vec<(bool, String)> {
    let mut out = Vec::new();
    if d.status != DecisionStatus::Proposed {
        out.push(if d.auto_status {
            (
                true,
                "Status set automatically when its task was finished.".to_owned(),
            )
        } else {
            (false, "Status set by you.".to_owned())
        });
    }
    if d.decided_on.is_some() {
        out.push(if d.auto_decided_on {
            (true, "Date filled in automatically.".to_owned())
        } else {
            (false, "Date set by you.".to_owned())
        });
    }
    out
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
        <ul class="space-y-0.5 text-[11px] text-muted">
            {provenance(&decision).into_iter().map(|(auto, text)| view! {
                <li>
                    <span class=if auto { "mr-1 rounded-sm border border-accent/40 bg-accent/10 px-1 text-[10px] font-semibold uppercase text-accent" }
                                else { "mr-1 rounded-sm border border-line px-1 text-[10px] font-semibold uppercase text-faint" }>
                        {if auto { "auto" } else { "you" }}
                    </span>
                    {text}
                </li>
            }).collect_view()}
        </ul>
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
    let confirm = expect_context::<ConfirmArchive>();

    let archive = move || {
        spawn_local(async move {
            if finish(api::archive_decision(id).await, toasts, version).is_some() {
                selection.close();
            }
        });
    };
    view! {
        <Section title="Archive" collapsed=true tone=crate::components::page::Tone::Danger>
            <button class=BUTTON_DANGER
                on:click=move |_| confirm.ask("Archive this decision?", format!("It leaves the Decisions log and its links are archived with it. {UNDO_HINT}"), archive)>
                "Archive decision…"
            </button>
        </Section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decision(status: DecisionStatus, on: Option<&str>, auto: (bool, bool)) -> Decision {
        let now = time::OffsetDateTime::UNIX_EPOCH;
        Decision {
            id: Uuid::nil(),
            title: "t".into(),
            context: String::new(),
            decision: String::new(),
            rationale: String::new(),
            decided_on: on.map(|d| minimap_types::timefmt::parse_date(d).unwrap()),
            status,
            auto_status: auto.0,
            auto_decided_on: auto.1,
            created_at: now,
            updated_at: now,
            archived_at: None,
        }
    }

    #[test]
    fn the_pane_says_what_was_automatic_and_what_the_user_set() {
        let p = provenance(&decision(DecisionStatus::Proposed, None, (false, false)));
        assert!(p.is_empty());
        let p = provenance(&decision(
            DecisionStatus::Decided,
            Some("2027-03-01"),
            (true, true),
        ));
        assert_eq!(p.len(), 2);
        assert!(p.iter().all(|(auto, _)| *auto));
        let p = provenance(&decision(
            DecisionStatus::Decided,
            Some("2027-03-01"),
            (true, false),
        ));
        assert!(p[0].0 && !p[1].0);
        assert!(p[1].1.contains("you"));
        let p = provenance(&decision(
            DecisionStatus::Decided,
            Some("2027-03-01"),
            (false, false),
        ));
        assert!(p.iter().all(|(auto, _)| !*auto));
    }

    #[test]
    fn every_status_is_offered_once() {
        let options = status_options();
        assert_eq!(options.len(), DecisionStatus::ALL.len());
        for s in DecisionStatus::ALL {
            assert!(options.iter().any(|(v, _)| v == s.as_str()));
        }
    }
}
