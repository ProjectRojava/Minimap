//! Focus (spec 37) in the screens: the star on a board card or list row, and the Focus field in a
//! task's panel. A task in focus is kept on This week every day, whatever its due date.

use leptos::{prelude::*, task::spawn_local};
use minimap_types::{Date, Focus, FocusChoice, Uuid};

use crate::{
    api,
    components::{
        date_field::DateField,
        form::{date_patch, SelectField},
        task_board::today,
    },
    icons,
    state::{finish, DataVersion, Toasts},
};

// ------------------------------------------------------------------ pure text and rules

/// The star is lit: the task is in focus and its last day (if it has one) has not passed. With no
/// date for "today" (it can't be read) a focus counts as lit.
pub fn is_on(focus: Option<&Focus>, today: Option<Date>) -> bool {
    match (focus, today) {
        (None, _) => false,
        (Some(f), Some(today)) => f.is_active(today),
        (Some(_), None) => true,
    }
}

/// What clicking the star (or `f`) does: a task in focus is taken out, any other is pinned.
pub fn toggle_choice(on: bool) -> FocusChoice {
    if on {
        FocusChoice::Off
    } else {
        FocusChoice::Pinned
    }
}

/// The tooltip of the star.
pub fn star_title(focus: Option<&Focus>, on: bool) -> String {
    match (focus, on) {
        (Some(f), true) => format!("In focus {}. Click to take it out (f)", f.describe()),
        _ => "Put in focus: keep it on This week every day (f)".to_owned(),
    }
}

/// The choices in the Focus box, as `(value, label)`.
pub fn options() -> Vec<(String, String)> {
    [
        ("off", "Not in focus"),
        ("pinned", "Every day, until I take it out"),
        ("today", "Today only"),
        ("until", "Every day, until a date…"),
    ]
    .iter()
    .map(|(v, l)| ((*v).to_owned(), (*l).to_owned()))
    .collect()
}

/// The box's value for what is stored.
pub fn mode_of(focus: Option<&Focus>, today: Option<Date>) -> &'static str {
    let choice = match today {
        Some(today) => FocusChoice::of(focus, today),
        None => match focus {
            None => FocusChoice::Off,
            Some(Focus { until: None }) => FocusChoice::Pinned,
            Some(Focus { until: Some(date) }) => FocusChoice::Until { date: *date },
        },
    };
    match choice {
        FocusChoice::Off => "off",
        FocusChoice::Pinned => "pinned",
        FocusChoice::Today => "today",
        FocusChoice::Until { .. } => "until",
    }
}

/// The sentence under the Focus box.
pub fn help_text(focus: Option<&Focus>, today: Option<Date>) -> String {
    match focus {
        None => "Not in focus. Put it in focus to see it on This week every day, however far off its due date is.".to_owned(),
        Some(f) if !is_on(Some(f), today) => format!(
            "Its focus ended {}. Pick a choice to bring it back.",
            f.until.map(|d| d.to_string()).unwrap_or_default()
        ),
        Some(Focus { until: None }) => "In focus: on This week every day until you take it out, or finish the task.".to_owned(),
        Some(Focus { until: Some(d) }) if Some(*d) == today => "In focus today only.".to_owned(),
        Some(Focus { until: Some(d) }) => format!("In focus: on This week every day through {d}, then it drops off by itself."),
    }
}

// ------------------------------------------------------------------ saving

/// Saves a focus choice for a task and refreshes the screen; says why on a refusal.
pub fn set_focus(task: Uuid, choice: FocusChoice, toasts: Toasts, version: DataVersion) {
    spawn_local(async move {
        finish(api::set_task_focus(task, choice).await, toasts, version);
    });
}

// ------------------------------------------------------------------ the star

/// A small star that puts a task in focus (pinned) or takes it out. Lit while the task is in
/// focus; otherwise it shows on hover of its row or card. It never opens the pane.
#[component]
pub fn FocusStar(
    task: Uuid,
    focus: Option<Focus>,
    /// Extra classes for where it sits (an absolute corner of a card, say).
    #[prop(optional)]
    place: &'static str,
) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let on = is_on(focus.as_ref(), today());
    let title = star_title(focus.as_ref(), on);
    let paths = icons::paths("star").unwrap_or(&[]);
    view! {
        <button type="button" draggable="false" title=title.clone() aria-label=title
                aria-pressed=on.to_string()
                class=format!(
                    "flex h-5 w-5 shrink-0 items-center justify-center rounded-sm hover:bg-hover {place} {}",
                    if on { "text-accent" }
                    else { "text-muted opacity-0 hover:text-fg focus:opacity-100 group-hover:opacity-100" })
                on:mousedown=|ev| ev.stop_propagation()
                on:click=move |ev| {
                    ev.stop_propagation();
                    set_focus(task, toggle_choice(on), toasts, version);
                }>
            <svg class="h-3.5 w-3.5" viewBox="0 0 16 16" stroke="currentColor" stroke-width="1.4"
                 stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"
                 fill=if on { "currentColor" } else { "none" }>
                {paths.iter().map(|d| view! { <path d=*d /> }).collect_view()}
            </svg>
        </button>
    }
}

// ------------------------------------------------------------------ the field

/// The "Focus" box of a task's panel: how it stays in front of you, and until when.
#[component]
pub fn FocusField(task: Uuid, focus: Option<Focus>) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let stored = RwSignal::new(focus);
    let mode = RwSignal::new(mode_of(focus.as_ref(), today()));
    let until_text = focus
        .and_then(|f| f.until)
        .map(|d| d.to_string())
        .unwrap_or_default();

    let save = move |choice: FocusChoice| {
        spawn_local(async move {
            if let Some(saved) = finish(api::set_task_focus(task, choice).await, toasts, version) {
                stored.set(saved.focus);
            }
        });
    };
    let on_mode = move |v: String| {
        let picked = options().into_iter().find(|(value, _)| *value == v);
        let Some((value, _)) = picked else { return };
        match value.as_str() {
            "off" => save(FocusChoice::Off),
            "pinned" => save(FocusChoice::Pinned),
            "today" => save(FocusChoice::Today),
            // Waits for a date: nothing is saved until one is typed or picked.
            _ => {}
        }
        mode.set(match value.as_str() {
            "off" => "off",
            "pinned" => "pinned",
            "today" => "today",
            _ => "until",
        });
    };
    let on_date = move |text: String| match date_patch(&text) {
        Ok(minimap_types::Patch::Set(date)) => save(FocusChoice::Until { date }),
        Ok(_) => {}
        Err(e) => {
            toasts.error(&e);
            version.bump();
        }
    };

    view! {
        <div class="mt-2">
            <SelectField label="Focus (keep on This week)" options=options()
                current=mode.get_untracked().to_owned() on_change=on_mode />
            {move || (mode.get() == "until").then(|| view! {
                <div class="mt-1.5">
                    <DateField label="Last day in focus" current=until_text.clone()
                        on_commit=on_date placeholder="YYYY-MM-DD" />
                </div>
            })}
            <p class="mt-1 text-[11px] text-muted">
                {move || help_text(stored.get().as_ref(), today())}
            </p>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::date;

    const TODAY: Date = date!(2027 - 03 - 03);

    fn until(d: Date) -> Option<Focus> {
        Some(Focus { until: Some(d) })
    }

    #[test]
    fn the_star_is_lit_while_the_focus_lasts() {
        assert!(!is_on(None, Some(TODAY)));
        assert!(is_on(Some(&Focus::PINNED), Some(TODAY)));
        assert!(is_on(until(TODAY).as_ref(), Some(TODAY)));
        assert!(!is_on(until(date!(2027 - 03 - 02)).as_ref(), Some(TODAY)));
        // No date for "today": a focus is taken as lit rather than hidden.
        assert!(is_on(until(date!(2020 - 01 - 01)).as_ref(), None));
    }

    #[test]
    fn clicking_the_star_pins_or_takes_out() {
        assert_eq!(toggle_choice(false), FocusChoice::Pinned);
        assert_eq!(toggle_choice(true), FocusChoice::Off);
    }

    #[test]
    fn the_box_shows_the_choice_that_made_the_focus() {
        assert_eq!(mode_of(None, Some(TODAY)), "off");
        assert_eq!(mode_of(Some(&Focus::PINNED), Some(TODAY)), "pinned");
        assert_eq!(mode_of(until(TODAY).as_ref(), Some(TODAY)), "today");
        assert_eq!(
            mode_of(until(date!(2027 - 04 - 01)).as_ref(), Some(TODAY)),
            "until"
        );
        assert_eq!(mode_of(until(TODAY).as_ref(), None), "until");
        // Every mode is one of the options.
        for mode in ["off", "pinned", "today", "until"] {
            assert!(options().iter().any(|(v, _)| v == mode));
        }
    }

    #[test]
    fn the_sentences_say_what_will_happen() {
        assert!(help_text(None, Some(TODAY)).starts_with("Not in focus"));
        assert!(help_text(Some(&Focus::PINNED), Some(TODAY)).contains("until you take it out"));
        assert_eq!(
            help_text(until(TODAY).as_ref(), Some(TODAY)),
            "In focus today only."
        );
        assert!(
            help_text(until(date!(2027 - 04 - 01)).as_ref(), Some(TODAY))
                .contains("through 2027-04-01")
        );
        assert!(
            help_text(until(date!(2027 - 03 - 01)).as_ref(), Some(TODAY))
                .contains("ended 2027-03-01")
        );
    }

    #[test]
    fn the_tooltip_says_what_a_click_does() {
        assert!(star_title(Some(&Focus::PINNED), true).contains("take it out"));
        assert!(star_title(None, false).contains("Put in focus"));
        // An ended focus reads as not in focus.
        assert!(star_title(until(date!(2027 - 03 - 01)).as_ref(), false).contains("Put in focus"));
    }
}
