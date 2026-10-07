//! Settings -> Developer (spec 24, debug builds only): add the demo dataset to an empty database.

use leptos::{prelude::*, task::spawn_local};

use crate::{
    api,
    components::{form::BUTTON_PRIMARY, page::Card},
    state::{DataVersion, Toasts},
};

/// What the demo data holds, for the card.
pub const CONTENTS: [&str; 9] = [
    "2 objectives and 3 projects; \"EU Region\" is at risk (projected two working days late, one task overdue, one blocked)",
    "40 tasks across the projects and the inbox, with blocks links between projects",
    "8 people (you and 7 others) in 2 nested teams, with reporting lines; one person is overloaded",
    "3 notes (a 1:1 with mentions and a checklist), 5 decisions (one replaced by another)",
    "3 waiting-ons: one stale, one resolved this week",
    "This week's events: a task finished, two due dates moved later, one task newly blocked",
    "Repeating work: a monthly task, a task every 12 weeks, and a weekly 1:1 note",
    "Subtasks: two clean-up tasks are steps of \"Right-size the compute fleet\"",
    "Reference links on three tasks: a Google Doc and Sheet, a Drive folder, and web pages",
];

#[component]
pub fn DeveloperSettings() -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    let version = expect_context::<DataVersion>();
    let busy = RwSignal::new(false);
    let seed = move |_| {
        busy.set(true);
        spawn_local(async move {
            match api::seed_demo_data().await {
                Ok(summary) => {
                    toasts.info(format!("Added demo data: {}", summary.describe()));
                    version.bump();
                }
                Err(e) => toasts.error(&e),
            }
            busy.set(false);
        });
    };
    let items = CONTENTS
        .iter()
        .map(|c| view! { <li>{*c}</li> })
        .collect_view();
    view! {
        <Card title="Demo data"
              description="A realistic company's worth of work for trying the app out. Only in debug builds.">
            <ul class="list-disc space-y-0.5 pl-5 text-[12px] text-muted">{items}</ul>
            <div class="flex items-center gap-2 pt-1">
                <button class=BUTTON_PRIMARY disabled=move || busy.get() on:click=seed>
                    {move || if busy.get() { "Adding…" } else { "Add demo data" }}
                </button>
            </div>
            <p class="text-[11px] text-muted">
                "Works on an empty database only (nothing but your own \"me\"), and it is all or nothing. "
                "Dates are set around today. It is refused while Google Drive is connected, so it can't "
                "end up in your real Drive. To remove it again, use Settings → Data & backup → Demo data, which appears while the database holds demo data (in every build)."
            </p>
        </Card>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_card_describes_the_documented_dataset() {
        let all = CONTENTS.join(" ");
        for needle in [
            "2 objectives",
            "3 projects",
            "40 tasks",
            "8 people",
            "2 nested teams",
        ] {
            assert!(all.contains(needle), "{needle}");
        }
    }
}
