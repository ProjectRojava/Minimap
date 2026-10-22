//! The *Start date* dialog (spec 06). A task in progress always has a start date, so moving a task
//! that has none to In progress is refused by the backend (`start_date_needed`) and this asks for
//! the day instead: today to start with, any day on the calendar or typed. *Start* moves the task
//! to In progress with that day in one change (one undo step); *Cancel* or Esc leaves it where it
//! was.

use leptos::{ev, html, prelude::*, task::spawn_local};
use minimap_types::{timefmt::parse_date, AppError, Patch, TaskStatus, UpdateTask, Uuid};

use crate::{
    api,
    components::{
        date_field::DateField,
        form::{BUTTON, BUTTON_PRIMARY},
        task_board::today,
    },
    state::{finish_task, DataVersion, FinishDecision, Toasts},
};

/// The change that starts `task` on the day typed, or why that isn't a day.
pub fn start_patch(text: &str) -> Result<UpdateTask, AppError> {
    let day = parse_date(text.trim()).map_err(|_| AppError {
        code: "invalid".into(),
        message: "A start date looks like 2027-03-31".into(),
    })?;
    Ok(UpdateTask {
        status: Some(TaskStatus::InProgress),
        start_date: Patch::Set(day),
        ..Default::default()
    })
}

/// Mount once, inside the router.
#[component]
pub fn StartDateHost() -> impl IntoView {
    let gate = expect_context::<FinishDecision>();
    view! {
        {move || gate.start.get().map(|task| view! { <Dialog task=task /> })}
    }
}

#[component]
fn Dialog(task: Uuid) -> impl IntoView {
    let gate = expect_context::<FinishDecision>();
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let detail = LocalResource::new(move || api::get_task_detail(task));
    let text = RwSignal::new(today().map(|d| d.to_string()).unwrap_or_default());
    let busy = RwSignal::new(false);
    let primary = NodeRef::<html::Button>::new();
    Effect::new(move |_| {
        if let Some(el) = primary.get() {
            let _ = el.focus();
        }
    });
    let title = move || match detail.get() {
        Some(Ok(d)) => d.task.title,
        _ => String::new(),
    };
    let start = move || {
        if busy.get_untracked() {
            return;
        }
        let patch = match start_patch(&text.get_untracked()) {
            Ok(p) => p,
            Err(e) => return toasts.error(&e),
        };
        busy.set(true);
        spawn_local(async move {
            let result = api::update_task(task, patch).await;
            busy.set(false);
            if finish_task(result, task, toasts, version, gate).is_some() {
                gate.close_start();
            }
        });
    };
    let on_keydown = move |e: ev::KeyboardEvent| {
        e.stop_propagation();
        match e.key().as_str() {
            "Escape" => {
                e.prevent_default();
                gate.close_start();
            }
            "Enter" => {
                e.prevent_default();
                start();
            }
            _ => {}
        }
    };
    view! {
        <div class="fixed inset-0 z-[80] bg-scrim" on:mousedown=move |_| gate.close_start()></div>
        <div role="dialog" aria-label="Start date"
             class="fixed inset-x-0 mx-auto top-[12vh] z-[81] w-[26rem] max-w-[94vw] \
                    rounded-sm border border-line bg-panel p-4 text-[13px]"
             on:keydown=on_keydown>
            <h2 class="mb-1 text-[14px] font-semibold">"When did you start?"</h2>
            <p class="mb-3 text-muted">
                <span class="text-fg">{title}</span>
                " has no start date. A task in progress has one."
            </p>
            <DateField label="Start date" current=text.get_untracked()
                       on_commit=move |v: String| text.set(v) />
            <div class="mt-4 flex items-center justify-end gap-2">
                <button type="button" class=BUTTON on:click=move |_| gate.close_start()>"Cancel"</button>
                <button type="button" class=BUTTON_PRIMARY node_ref=primary
                    disabled=move || busy.get()
                    on:click=move |_| start()>"Start"</button>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_day_typed_starts_the_task_and_anything_else_is_refused() {
        let patch = start_patch(" 2027-03-31 ").unwrap();
        assert_eq!(patch.status, Some(TaskStatus::InProgress));
        assert_eq!(
            patch.start_date,
            Patch::Set(parse_date("2027-03-31").unwrap())
        );
        for bad in ["", "soon", "2027-13-01"] {
            let err = start_patch(bad).unwrap_err();
            assert_eq!(err.code, "invalid");
            assert!(err.message.contains("2027-03-31"));
        }
    }
}
