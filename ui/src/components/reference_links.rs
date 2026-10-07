//! Reference links on a task (spec 28): web pages, Google Drive files, anything with an address.
//! A list in the detail pane (click one to open it in the system's browser), an address box with
//! an optional name, and remove. Which addresses are accepted is decided by the backend, which
//! answers with a message that becomes a toast.

use leptos::{prelude::*, task::spawn_local};
use minimap_types::{LinkKind, RefLink, UpdateTask, Uuid};

use crate::{
    api,
    components::{
        detail_pane::Section,
        form::{BUTTON_PRIMARY, INPUT},
    },
    state::{finish, DataVersion, Toasts},
};

/// The list with `link` added at the end.
fn with_added(list: &[RefLink], title: &str, url: &str) -> Vec<RefLink> {
    let mut out = list.to_vec();
    out.push(RefLink {
        title: title.trim().to_owned(),
        url: url.trim().to_owned(),
    });
    out
}

/// The list without the link at `index`.
fn without(list: &[RefLink], index: usize) -> Vec<RefLink> {
    let mut out = list.to_vec();
    if index < out.len() {
        out.remove(index);
    }
    out
}

fn kind_tag(kind: LinkKind) -> &'static str {
    match kind {
        LinkKind::Drive => "text-accent",
        LinkKind::Web | LinkKind::Email => "text-muted",
    }
}

#[component]
pub fn ReferenceLinks(task: Uuid) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let loaded = LocalResource::new(move || {
        version.track();
        api::get_task(task)
    });
    let url = RwSignal::new(String::new());
    let title = RwSignal::new(String::new());

    let save = move |links: Vec<RefLink>, after: Option<Box<dyn FnOnce()>>| {
        spawn_local(async move {
            let patch = UpdateTask {
                links: Some(links),
                ..Default::default()
            };
            if finish(api::update_task(task, patch).await, toasts, version).is_some() {
                if let Some(after) = after {
                    after();
                }
            }
        });
    };
    let open = move |address: String| {
        spawn_local(async move {
            if let Err(e) = api::open_link(address).await {
                toasts.error(&e);
            }
        });
    };

    view! {
        <Section title="Reference links">
            {move || match loaded.get() {
                None => view! { <p class="text-muted">"Loading…"</p> }.into_any(),
                Some(Err(_)) => view! { <p class="text-muted">"Couldn't load the links."</p> }.into_any(),
                Some(Ok(t)) => {
                    let list = t.links;
                    let add_to = list.clone();
                    let add = move || {
                        if url.get_untracked().trim().is_empty() {
                            return;
                        }
                        let links = with_added(&add_to, &title.get_untracked(), &url.get_untracked());
                        save(links, Some(Box::new(move || {
                            url.set(String::new());
                            title.set(String::new());
                        })));
                    };
                    let rows = list.iter().enumerate().map(|(i, link)| {
                        let address = link.url.clone();
                        let shown = link.label();
                        let host = link.host().filter(|_| !link.title.is_empty()).unwrap_or_default();
                        let kind = link.kind();
                        let remaining = without(&list, i);
                        view! {
                            <li class="flex items-center gap-2">
                                <span class=format!("w-9 shrink-0 text-[11px] {}", kind_tag(kind))>{kind.label()}</span>
                                <button class="min-w-0 flex-1 truncate text-left hover:underline"
                                        title=address.clone() on:click={
                                            let address = address.clone();
                                            move |_| open(address.clone())
                                        }>
                                    {shown}
                                </button>
                                <span class="shrink-0 truncate text-[11px] text-muted max-w-[7rem]">{host}</span>
                                <button class="px-1 text-faint hover:text-danger" aria-label="Remove link"
                                        on:click=move |_| save(remaining.clone(), None)>"✕"</button>
                            </li>
                        }
                    }).collect_view();
                    let empty = list.is_empty();
                    let add_enter = add.clone();
                    view! {
                        {if empty {
                            view! { <p class="mb-2 text-muted">"No links yet. Paste a web page or Google Drive address below."</p> }.into_any()
                        } else {
                            view! { <ul class="mb-2 space-y-1">{rows}</ul> }.into_any()
                        }}
                        <div class="space-y-1">
                            <input class=INPUT type="text" placeholder="Paste an address (https://…)"
                                aria-label="Link address"
                                prop:value=move || url.get()
                                on:input=move |ev| url.set(event_target_value(&ev))
                                on:keydown=move |ev| if ev.key() == "Enter" { add_enter() } />
                            <div class="flex gap-2">
                                <input class=INPUT type="text" placeholder="Name (optional)"
                                    aria-label="Link name"
                                    prop:value=move || title.get()
                                    on:input=move |ev| title.set(event_target_value(&ev))
                                    on:keydown={
                                        let add = add.clone();
                                        move |ev| if ev.key() == "Enter" { add() }
                                    } />
                                <button class=BUTTON_PRIMARY on:click=move |_| add()>"Add link"</button>
                            </div>
                        </div>
                    }.into_any()
                }
            }}
        </Section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(title: &str, url: &str) -> RefLink {
        RefLink {
            title: title.into(),
            url: url.into(),
        }
    }

    #[test]
    fn adding_appends_and_trims() {
        let list = vec![link("A", "https://a.co")];
        let out = with_added(&list, "  Plan ", " https://b.co ");
        assert_eq!(
            out,
            vec![link("A", "https://a.co"), link("Plan", "https://b.co")]
        );
    }

    #[test]
    fn removing_drops_only_that_one() {
        let list = vec![link("A", "https://a.co"), link("B", "https://b.co")];
        assert_eq!(without(&list, 0), vec![link("B", "https://b.co")]);
        assert_eq!(without(&list, 5), list);
    }
}
