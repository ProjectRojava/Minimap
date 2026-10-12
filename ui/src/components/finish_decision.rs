//! The *Finish decision* dialog (spec 42). A task of type Decision can't be finished until its
//! record says what was decided and why; wherever a task is moved to Done and that is missing,
//! this asks for the two texts, then saves them, finishes the task and decides the record in one
//! step.

use leptos::{ev, html, prelude::*, task::spawn_local};
use minimap_types::Uuid;

use crate::{
    api,
    components::form::{BUTTON, BUTTON_PRIMARY, INPUT},
    state::{finish, DataVersion, FinishDecision, Toasts},
};

/// Both boxes need words (the backend checks the same).
pub fn can_submit(decision: &str, rationale: &str) -> bool {
    !decision.trim().is_empty() && !rationale.trim().is_empty()
}

/// Mount once, inside the router.
#[component]
pub fn FinishDecisionHost() -> impl IntoView {
    let gate = expect_context::<FinishDecision>();
    view! {
        {move || gate.0.get().map(|task| view! { <Dialog task=task /> })}
    }
}

#[component]
fn Dialog(task: Uuid) -> impl IntoView {
    let gate = expect_context::<FinishDecision>();
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let detail = LocalResource::new(move || api::get_task_detail(task));
    let decision = RwSignal::new(String::new());
    let rationale = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let first = NodeRef::<html::Textarea>::new();
    // What is already written is the starting point; typing is never overwritten after that.
    let filled = RwSignal::new(false);
    Effect::new(move |_| {
        if filled.get_untracked() {
            return;
        }
        if let Some(Ok(d)) = detail.get() {
            if let Some(r) = d.decision {
                decision.set(r.decision);
                rationale.set(r.rationale);
            }
            filled.set(true);
            if let Some(el) = first.get() {
                let _ = el.focus();
            }
        }
    });
    let title = move || match detail.get() {
        Some(Ok(d)) => d.task.title,
        _ => String::new(),
    };
    let submit = move || {
        let (d, r) = (decision.get_untracked(), rationale.get_untracked());
        if busy.get_untracked() || !can_submit(&d, &r) {
            return;
        }
        busy.set(true);
        spawn_local(async move {
            let result = api::finish_decision_task(task, d, r).await;
            busy.set(false);
            if finish(result, toasts, version).is_some() {
                gate.close();
                toasts.info("Decided, and the task is done. Ctrl/Cmd+Z undoes it.".to_owned());
            }
        });
    };
    let on_keydown = move |e: ev::KeyboardEvent| {
        e.stop_propagation();
        if e.key() == "Escape" {
            e.prevent_default();
            gate.close();
        } else if e.key() == "Enter" && (e.ctrl_key() || e.meta_key()) {
            e.prevent_default();
            submit();
        }
    };
    view! {
        <div class="fixed inset-0 z-[80] bg-scrim" on:mousedown=move |_| gate.close()></div>
        <div role="dialog" aria-label="Finish decision"
             class="fixed inset-x-0 mx-auto top-[12vh] z-[81] w-[34rem] max-w-[94vw] \
                    rounded-sm border border-line bg-panel p-4 text-[13px]"
             on:keydown=on_keydown>
            <h2 class="mb-1 text-[14px] font-semibold">"Finish decision"</h2>
            <p class="mb-3 text-muted">
                "Before " <span class="text-fg">{title}</span>
                " is done, write what was decided and why. It goes on the decision record, which is marked decided."
            </p>
            <label class="mb-1 block text-[11px] text-muted">"Decision: what was decided?"</label>
            <textarea class=format!("{INPUT} mb-3 h-24 resize-y") node_ref=first
                prop:value=move || decision.get()
                on:input=move |ev| decision.set(event_target_value(&ev)) />
            <label class="mb-1 block text-[11px] text-muted">"Rationale: why?"</label>
            <textarea class=format!("{INPUT} mb-3 h-24 resize-y")
                prop:value=move || rationale.get()
                on:input=move |ev| rationale.set(event_target_value(&ev)) />
            <div class="flex items-center justify-end gap-2">
                <span class="mr-auto text-[11px] text-faint">"Ctrl/Cmd+Enter to save"</span>
                <button type="button" class=BUTTON on:click=move |_| gate.close()>"Cancel"</button>
                <button type="button" class=BUTTON_PRIMARY
                    disabled=move || busy.get() || !can_submit(&decision.get(), &rationale.get())
                    on:click=move |_| submit()>"Decide and mark done"</button>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_texts_are_needed_and_blanks_do_not_count() {
        assert!(can_submit("Postgres", "We know it"));
        assert!(!can_submit("Postgres", "  "));
        assert!(!can_submit("", "why"));
    }
}
