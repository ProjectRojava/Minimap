//! The *Resolve waiting-ons* dialog (spec 56). Finishing a task is never held up by what it is
//! waiting for; afterwards, if something is still open on it, this asks whether it arrived. One
//! click resolves them all in one step (one undo); Esc or *Leave open* keeps them on Waiting On.

use leptos::{ev, html, prelude::*, task::spawn_local};
use minimap_types::Uuid;

use crate::{
    api,
    components::{
        avatar::PersonLabel,
        form::{BUTTON, BUTTON_PRIMARY},
        waiting_panel::age_text,
    },
    state::{finish, DataVersion, FinishDecision, Toasts},
};

/// "Resolve it too?" / "Resolve them too?": the question for `n` open waiting-ons.
pub fn question(n: usize) -> &'static str {
    if n == 1 {
        "Resolve it too?"
    } else {
        "Resolve them too?"
    }
}

/// "Resolved 2 waiting-ons. Ctrl/Cmd+Z undoes it."
pub fn done_text(n: u32) -> String {
    format!(
        "Resolved {n} waiting-on{}. Ctrl/Cmd+Z undoes it.",
        if n == 1 { "" } else { "s" }
    )
}

/// Mount once, inside the router.
#[component]
pub fn FinishWaitingHost() -> impl IntoView {
    let gate = expect_context::<FinishDecision>();
    view! {
        {move || gate.waiting.get().map(|task| view! { <Dialog task=task /> })}
    }
}

#[component]
fn Dialog(task: Uuid) -> impl IntoView {
    let gate = expect_context::<FinishDecision>();
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let rows = LocalResource::new(move || api::get_task_waiting(task));
    let detail = LocalResource::new(move || api::get_task_detail(task));
    let busy = RwSignal::new(false);
    let primary = NodeRef::<html::Button>::new();
    Effect::new(move |_| {
        if let Some(el) = primary.get() {
            let _ = el.focus();
        }
    });
    let open = move || {
        rows.get()
            .and_then(Result::ok)
            .map(|r| {
                r.into_iter()
                    .filter(|r| r.waiting.resolved_on.is_none())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    let title = move || match detail.get() {
        Some(Ok(d)) => d.task.title,
        _ => String::new(),
    };
    let resolve = move || {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        spawn_local(async move {
            let result = api::resolve_task_waiting_ons(task).await;
            busy.set(false);
            if let Some(n) = finish(result, toasts, version) {
                gate.close_waiting();
                toasts.info(done_text(n));
            }
        });
    };
    let on_keydown = move |e: ev::KeyboardEvent| {
        e.stop_propagation();
        if e.key() == "Escape" {
            e.prevent_default();
            gate.close_waiting();
        }
    };
    view! {
        <div class="fixed inset-0 z-[80] bg-scrim" on:mousedown=move |_| gate.close_waiting()></div>
        <div role="dialog" aria-label="Resolve waiting-ons"
             class="fixed inset-x-0 mx-auto top-[12vh] z-[81] w-[30rem] max-w-[94vw] \
                    rounded-sm border border-line bg-panel p-4 text-[13px]"
             on:keydown=on_keydown>
            <h2 class="mb-1 text-[14px] font-semibold">{move || question(open().len())}</h2>
            <p class="mb-3 text-muted">
                <span class="text-fg">{title}</span>
                " is done. It was still waiting on:"
            </p>
            <ul class="mb-3 space-y-1">
                {move || open().into_iter().map(|r| view! {
                    <li>
                        <span class="font-medium"><PersonLabel id=r.person.node.id name=r.person.label.clone() /></span>
                        " · " {r.waiting.description.clone()}
                        <span class="text-muted">{format!(" · {}", age_text(r.age_days))}</span>
                    </li>
                }).collect_view()}
            </ul>
            <div class="flex items-center justify-end gap-2">
                <button type="button" class=BUTTON on:click=move |_| gate.close_waiting()>"Leave open"</button>
                <button type="button" class=BUTTON_PRIMARY node_ref=primary
                    disabled=move || busy.get() || open().is_empty()
                    on:click=move |_| resolve()>"Resolve"</button>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_question_and_the_toast_agree_with_the_count() {
        assert_eq!(question(1), "Resolve it too?");
        assert_eq!(question(3), "Resolve them too?");
        assert_eq!(done_text(1), "Resolved 1 waiting-on. Ctrl/Cmd+Z undoes it.");
        assert_eq!(
            done_text(2),
            "Resolved 2 waiting-ons. Ctrl/Cmd+Z undoes it."
        );
    }
}
