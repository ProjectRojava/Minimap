//! The decision record in a task's pane (spec 41). A task of type Decision has a record in the
//! Decisions log, made with it; it is written here, and it stays in the log when the task is
//! finished or archived.

use leptos::{prelude::*, task::spawn_local};
use minimap_types::{DecisionStatus, TaskStatus, Uuid, DECISION_TYPE};

use crate::{
    api,
    components::{
        decision_panel::{StatusPart, WritePart},
        detail_pane::Section,
        form::BUTTON,
    },
    state::{finish, DataVersion, Toasts},
};

/// Whether to nudge: the task is finished but nothing was decided yet.
pub fn needs_deciding(task: TaskStatus, decision: DecisionStatus) -> bool {
    task == TaskStatus::Done && decision == DecisionStatus::Proposed
}

#[component]
pub fn TaskDecision(task: Uuid) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    // Follows every write: the type can be changed, the task finished, the record made.
    let detail = LocalResource::new(move || {
        version.track();
        api::get_task_detail(task)
    });
    // The write-up is built once per record, so typing is never overwritten by a reload.
    let record = Memo::new(move |_| {
        detail
            .get()
            .and_then(Result::ok)
            .and_then(|d| d.decision.map(|r| r.id))
    });
    let is_decision_task = Memo::new(move |_| {
        detail
            .get()
            .and_then(Result::ok)
            .is_some_and(|d| d.task.task_type.as_deref() == Some(DECISION_TYPE))
    });
    let nudge = Memo::new(move |_| {
        detail.get().and_then(Result::ok).is_some_and(|d| {
            d.decision
                .is_some_and(|r| needs_deciding(d.task.status, r.status))
        })
    });
    let make = move |_| {
        spawn_local(async move {
            finish(api::ensure_task_decision(task).await, toasts, version);
        });
    };

    move || {
        match (record.get(), is_decision_task.get()) {
        (Some(id), _) => view! {
            <Section title="Decision record" always_open=true>
                <p class="text-[11px] text-muted">
                    "This record is in the Decisions log and stays there when the task is done. Write the decision and the rationale before finishing the task: finishing it decides the record."
                </p>
                {move || nudge.get().then(|| view! {
                    <p class="text-warning">
                        "The task is done but nothing is decided yet. Write what was decided and set the status to Decided."
                    </p>
                })}
                <StatusPart id=id />
                <WritePart id=id />
            </Section>
        }
        .into_any(),
        (None, true) => view! {
            <Section title="Decision record" always_open=true>
                <p class="text-muted">"This task has no record in the Decisions log."</p>
                <button class=BUTTON on:click=make>"Create decision record"</button>
            </Section>
        }
        .into_any(),
        _ => ().into_any(),
    }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_finished_task_with_an_undecided_record_is_nudged() {
        assert!(needs_deciding(TaskStatus::Done, DecisionStatus::Proposed));
        assert!(!needs_deciding(TaskStatus::Done, DecisionStatus::Decided));
        assert!(!needs_deciding(TaskStatus::Todo, DecisionStatus::Proposed));
    }
}
