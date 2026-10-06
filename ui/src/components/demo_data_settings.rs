//! Settings -> Data & backup -> Demo data (spec 24): remove the sample company again. Shown in
//! every build, but only while the database holds demo data (a development build may have added
//! it to the data folder a released build opens).

use leptos::{prelude::*, task::spawn_local};
use minimap_types::{DemoImpact, DemoRemoval, DemoSource};

use crate::{
    api,
    components::{
        form::{BUTTON, BUTTON_DANGER},
        page::Card,
    },
    state::{DataVersion, Toasts},
};

/// How the demo data was recognised, for the user.
fn source_text(source: Option<DemoSource>) -> &'static str {
    match source {
        Some(DemoSource::Titles) => {
            "This was added before Minimap kept a record of it, so it is recognised by the exact \
             titles of the sample items. Anything you renamed counts as yours and stays."
        }
        _ => "Minimap kept a record of exactly what it added, so only those items are removed.",
    }
}

/// What removal does to the user's own items, one sentence each; empty when nothing of theirs is
/// involved.
pub fn impact_lines(impact: &DemoImpact) -> Vec<String> {
    let n = |count: u32, one: &str, many: &str| {
        if count == 1 {
            one.to_owned()
        } else {
            many.replace("{n}", &count.to_string())
        }
    };
    let mut lines = Vec::new();
    if impact.tasks_to_inbox > 0 {
        lines.push(n(
            impact.tasks_to_inbox,
            "1 of your tasks is in a demo project: it moves to the Inbox.",
            "{n} of your tasks are in demo projects: they move to the Inbox.",
        ));
    }
    if impact.owners_cleared > 0 {
        lines.push(n(
            impact.owners_cleared,
            "1 of your projects is owned by a demo person: it loses its owner.",
            "{n} of your projects are owned by demo people: they lose their owner.",
        ));
    }
    if impact.teams_unparented > 0 {
        lines.push(n(
            impact.teams_unparented,
            "1 of your teams is inside a demo team: it becomes a top-level team.",
            "{n} of your teams are inside demo teams: they become top-level teams.",
        ));
    }
    if impact.your_links > 0 {
        lines.push(n(
            impact.your_links,
            "1 link between your items and the demo items is removed with them.",
            "{n} links between your items and the demo items are removed with them.",
        ));
    }
    if impact.people_kept > 0 {
        lines.push(n(
            impact.people_kept,
            "1 demo person stays, because a waiting-on of yours is about them.",
            "{n} demo people stay, because waiting-ons of yours are about them.",
        ));
    }
    lines
}

/// The toast after a removal: what went and where the backup is.
fn removed_text(removal: &DemoRemoval) -> String {
    let backup = removal
        .backup
        .as_deref()
        .map(|p| {
            let name = p.rsplit(['/', '\\']).next().unwrap_or(p);
            format!(" A backup was saved first: {name}.")
        })
        .unwrap_or_default();
    format!("Removed demo data: {}.{backup}", removal.removed.describe())
}

#[component]
pub fn DemoDataSettings() -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let status = LocalResource::new(move || {
        version.track();
        api::get_demo_status()
    });
    let confirming = RwSignal::new(false);
    let busy = RwSignal::new(false);
    let remove = move |_| {
        busy.set(true);
        spawn_local(async move {
            match api::remove_demo_data().await {
                Ok(removal) => {
                    toasts.info(removed_text(&removal));
                    confirming.set(false);
                }
                Err(e) => toasts.error(&e),
            }
            // Always: the card disappears once the data is gone, or shows what is left.
            version.bump();
            busy.set(false);
        });
    };

    view! {
        {move || match status.get() {
            Some(Ok(s)) if s.found => {
                let impact = impact_lines(&s.impact);
                view! {
                    <Card title="Demo data"
                          description="This database holds the sample company from the demo data.">
                        <p>{format!("Found: {}.", s.items.describe())}</p>
                        <p class="text-[12px] text-muted">{source_text(s.source)}</p>
                        {if impact.is_empty() {
                            view! { <p class="text-[12px] text-muted">"Nothing of yours is linked to it."</p> }.into_any()
                        } else {
                            view! {
                                <ul class="list-disc space-y-0.5 pl-5 text-[12px]">
                                    {impact.into_iter().map(|l| view! { <li>{l}</li> }).collect_view()}
                                </ul>
                            }.into_any()
                        }}
                        <p class="text-[12px] text-muted">
                            "Your own items are never deleted. Removing the demo data takes the sample items and their links away for good."
                        </p>
                        <Show when=move || !confirming.get()>
                            <button class=BUTTON_DANGER on:click=move |_| confirming.set(true)>
                                "Remove demo data…"
                            </button>
                        </Show>
                        <Show when=move || confirming.get()>
                            <div class="space-y-2 rounded-sm border border-danger/40 bg-danger/10 p-3">
                                <p class="font-medium">"Remove the demo data?"</p>
                                <p class="text-[12px]">
                                    "A backup of everything is saved first, so you can get it all back with "
                                    "Restore in Backups below. This can't be undone with Ctrl/Cmd+Z. "
                                    "If Google Drive is connected, the removal reaches your other computers too."
                                </p>
                                <div class="flex gap-2">
                                    <button class=BUTTON_DANGER disabled=move || busy.get() on:click=remove>
                                        {move || if busy.get() { "Removing…" } else { "Remove demo data" }}
                                    </button>
                                    <button class=BUTTON disabled=move || busy.get()
                                            on:click=move |_| confirming.set(false)>"Keep it"</button>
                                </div>
                            </div>
                        </Show>
                    </Card>
                }.into_any()
            }
            _ => ().into_any(),
        }}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::DemoSummary;

    #[test]
    fn nothing_of_yours_involved_says_nothing() {
        assert!(impact_lines(&DemoImpact::default()).is_empty());
    }

    #[test]
    fn each_effect_on_your_items_is_one_plain_sentence() {
        let lines = impact_lines(&DemoImpact {
            your_links: 1,
            tasks_to_inbox: 3,
            owners_cleared: 1,
            teams_unparented: 2,
            people_kept: 1,
        });
        assert_eq!(lines.len(), 5);
        assert!(lines[0].starts_with("3 of your tasks") && lines[0].contains("Inbox"));
        assert!(lines[1].starts_with("1 of your projects") && lines[1].contains("owner"));
        assert!(lines[2].starts_with("2 of your teams") && lines[2].contains("top-level"));
        assert!(lines[3].starts_with("1 link "));
        assert!(lines[4].contains("waiting-on"));
    }

    #[test]
    fn the_toast_names_what_went_and_the_backup_file() {
        let removal = DemoRemoval {
            removed: DemoSummary {
                tasks: 40,
                ..Default::default()
            },
            impact: DemoImpact::default(),
            backup: Some("/home/me/.local/share/app/backups/minimap-20270303-101500.db".into()),
        };
        let text = removed_text(&removal);
        assert!(text.starts_with("Removed demo data: 0 objectives"));
        assert!(text.contains("40 tasks"));
        assert!(text.ends_with("A backup was saved first: minimap-20270303-101500.db."));
        let windows = DemoRemoval {
            backup: Some(r"C:\Users\me\backups\minimap-1.db".into()),
            ..removal
        };
        assert!(removed_text(&windows).contains("minimap-1.db."));
    }

    #[test]
    fn the_source_is_explained_in_words() {
        assert!(source_text(Some(DemoSource::Recorded)).contains("record"));
        assert!(source_text(Some(DemoSource::Titles)).contains("titles"));
    }
}
