//! The backburner (spec 59) in the screens: the words, the `b` key and the Backburner box in a
//! task's panel. The board column is in `task_board.rs`. A task on the backburner keeps its
//! status, dates and links, and is left out of This week, capacity, health counts and the fog
//! until it is brought back; the schedule still counts it.

use leptos::{prelude::*, task::spawn_local};
use minimap_types::{Date, Uuid};

use crate::{
    api,
    components::form::BUTTON,
    state::{finish, DataVersion, Toasts},
};

// ------------------------------------------------------------------ pure text

/// The toast after putting a task on the backburner: how many open tasks still wait on it.
pub fn moved_text(held_up: u32) -> String {
    match held_up {
        0 => "Moved to Backburner.".to_owned(),
        1 => "Moved to Backburner. 1 task still waits on it.".to_owned(),
        n => format!("Moved to Backburner. {n} tasks still wait on it."),
    }
}

/// The toast after bringing one back.
pub const BACK_TEXT: &str = "Brought back from the backburner.";

/// How long a task has been there: "today", "1 d", "12 d". Empty when "today" can't be read.
pub fn since_text(since: Date, today: Option<Date>) -> String {
    match today.map(|t| (t - since).whole_days().max(0)) {
        Some(0) => "today".to_owned(),
        Some(days) => format!("{days} d"),
        None => String::new(),
    }
}

/// The tooltip of that age on a card.
pub fn since_title(since: Date) -> String {
    format!("On the backburner since {since}")
}

/// The sentence under the Backburner box.
pub fn help_text(since: Option<Date>) -> String {
    match since {
        None => "Set it aside for now. It keeps its status, dates and links, and leaves This week, capacity and the risks until you bring it back.".to_owned(),
        Some(d) => format!("On the backburner since {d}. It is left out of This week, capacity and the risks. The schedule still counts it. Bring it back when you are ready."),
    }
}

// ------------------------------------------------------------------ saving

/// Puts a task on the backburner, or brings it back, and says what happened (with *Undo*).
pub fn set_backburner(task: Uuid, on: bool, toasts: Toasts, version: DataVersion) {
    spawn_local(async move {
        if let Some(r) = finish(api::set_task_backburner(task, on).await, toasts, version) {
            toasts.info_with_undo(if on {
                moved_text(r.held_up)
            } else {
                BACK_TEXT.to_owned()
            });
        }
    });
}

// ------------------------------------------------------------------ the field

/// The Backburner box of a task's panel.
#[component]
pub fn BackburnerField(task: Uuid, since: Option<Date>) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let stored = RwSignal::new(since);
    let toggle = move |_| {
        let on = stored.get_untracked().is_none();
        spawn_local(async move {
            if let Some(r) = finish(api::set_task_backburner(task, on).await, toasts, version) {
                stored.set(r.task.backburner_since);
                toasts.info_with_undo(if on {
                    moved_text(r.held_up)
                } else {
                    BACK_TEXT.to_owned()
                });
            }
        });
    };
    view! {
        <div class="mt-2">
            <span class="block text-[11px] text-muted">"Backburner (b)"</span>
            <button type="button" class=BUTTON on:click=toggle>
                {move || if stored.get().is_some() { "Bring it back" } else { "Put on the backburner" }}
            </button>
            <p class="mt-1 text-[11px] text-muted">
                {move || help_text(stored.get())}
            </p>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::date;

    #[test]
    fn the_toast_says_how_many_tasks_still_wait() {
        assert_eq!(moved_text(0), "Moved to Backburner.");
        assert_eq!(
            moved_text(1),
            "Moved to Backburner. 1 task still waits on it."
        );
        assert_eq!(
            moved_text(3),
            "Moved to Backburner. 3 tasks still wait on it."
        );
    }

    #[test]
    fn the_age_is_in_days_and_never_negative() {
        let since = date!(2027 - 03 - 03);
        assert_eq!(since_text(since, Some(since)), "today");
        assert_eq!(since_text(since, Some(date!(2027 - 03 - 04))), "1 d");
        assert_eq!(since_text(since, Some(date!(2027 - 03 - 15))), "12 d");
        assert_eq!(since_text(since, Some(date!(2027 - 03 - 01))), "today");
        assert_eq!(since_text(since, None), "");
        assert!(since_title(since).contains("2027-03-03"));
    }

    #[test]
    fn the_sentences_say_what_will_happen() {
        assert!(help_text(None).contains("leaves This week"));
        let on = help_text(Some(date!(2027 - 03 - 03)));
        assert!(on.contains("since 2027-03-03") && on.contains("schedule still counts it"));
    }
}
