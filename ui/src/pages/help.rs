//! Help: the wiki. Pages from `crate::help` (Markdown in `docs/help/`), a list grouped by
//! subject, a search, "On this page" jumps and previous/next. The page is in the address
//! (`/help?page=quick-add`), so other screens can link to one.

use leptos::{ev, prelude::*};
use leptos_router::{
    hooks::{query_signal_with_options, use_navigate},
    NavigateOptions,
};

use crate::{
    components::{
        form::INPUT,
        page::{Hints, PageHeader},
    },
    help::{
        self,
        markdown::{self, Block, Inline},
        Page,
    },
};

const CODE: &str = "rounded-sm border border-line bg-canvas px-1 font-mono text-[12px]";

/// Where a help link points: another page (`help:id`) or a screen (`app:/path`).
enum Target {
    Page(String),
    Screen(String),
}

fn target_of(raw: &str) -> Option<Target> {
    if let Some(id) = raw.strip_prefix("help:") {
        Some(Target::Page(id.to_owned()))
    } else {
        raw.strip_prefix("app:")
            .map(|path| Target::Screen(path.to_owned()))
    }
}

/// The address a link goes to (so hovering shows it and middle-click behaves).
fn href_of(target: &Target) -> String {
    match target {
        Target::Page(id) => format!("/help?page={id}"),
        Target::Screen(path) => path.clone(),
    }
}

fn inlines(parts: Vec<Inline>, go: Callback<String>) -> impl IntoView {
    parts
        .into_iter()
        .map(|part| match part {
            Inline::Text(t) => t.into_any(),
            Inline::Bold(t) => {
                view! { <strong class="font-semibold text-fg">{t}</strong> }.into_any()
            }
            Inline::Italic(t) => view! { <em>{t}</em> }.into_any(),
            Inline::Code(t) => view! { <code class=CODE>{t}</code> }.into_any(),
            Inline::Link { text, target } => match target_of(&target) {
                Some(t) => {
                    let href = href_of(&t);
                    view! {
                        <a href=href class="text-accent hover:underline"
                           on:click=move |ev: ev::MouseEvent| {
                               ev.prevent_default();
                               go.run(target.clone());
                           }>{text}</a>
                    }
                    .into_any()
                }
                None => text.into_any(),
            },
        })
        .collect_view()
}

fn blocks(list: Vec<Block>, go: Callback<String>) -> impl IntoView {
    list.into_iter()
        .map(|block| match block {
            Block::Heading { level: 2, text, id } => view! {
                <h2 id=id class="mt-8 mb-2 border-b border-line pb-1 text-[15px] font-semibold">{text}</h2>
            }
            .into_any(),
            Block::Heading { text, id, .. } => view! {
                <h3 id=id class="mt-5 mb-1 text-[13px] font-semibold">{text}</h3>
            }
            .into_any(),
            Block::Paragraph(p) => view! {
                <p class="my-2 text-[13px] leading-6">{inlines(p, go)}</p>
            }
            .into_any(),
            Block::Bullets(items) => view! {
                <ul class="my-2 list-disc space-y-1 pl-5 text-[13px] leading-6">
                    {items.into_iter().map(|i| view! { <li>{inlines(i, go)}</li> }).collect_view()}
                </ul>
            }
            .into_any(),
            Block::Numbered(items) => view! {
                <ol class="my-2 list-decimal space-y-1 pl-5 text-[13px] leading-6">
                    {items.into_iter().map(|i| view! { <li>{inlines(i, go)}</li> }).collect_view()}
                </ol>
            }
            .into_any(),
            Block::Table { head, rows } => view! {
                <div class="my-3 overflow-x-auto">
                    <table class="w-full border-collapse text-[12px]">
                        <thead>
                            <tr>
                                {head.into_iter().map(|c| view! {
                                    <th class="border border-line bg-panel px-2 py-1 text-left font-medium">{inlines(c, go)}</th>
                                }).collect_view()}
                            </tr>
                        </thead>
                        <tbody>
                            {rows.into_iter().map(|row| view! {
                                <tr>
                                    {row.into_iter().map(|c| view! {
                                        <td class="border border-line px-2 py-1 align-top leading-5">{inlines(c, go)}</td>
                                    }).collect_view()}
                                </tr>
                            }).collect_view()}
                        </tbody>
                    </table>
                </div>
            }
            .into_any(),
            Block::Code(code) => view! {
                <pre class="my-3 overflow-x-auto rounded-sm border border-line bg-canvas p-3 font-mono text-[12px] leading-5">{code}</pre>
            }
            .into_any(),
            Block::Note(p) => view! {
                <div class="my-3 rounded-sm border border-line border-l-2 border-l-accent bg-panel px-3 py-2 text-[13px] leading-6">
                    {inlines(p, go)}
                </div>
            }
            .into_any(),
            Block::Rule => view! { <hr class="my-6 border-line" /> }.into_any(),
        })
        .collect_view()
}

/// Scrolls to a heading of the page.
fn scroll_to(id: &str) {
    if let Some(el) = document().get_element_by_id(id) {
        el.scroll_into_view();
    }
}

#[component]
pub fn Help() -> impl IntoView {
    let navigate = use_navigate();
    let (page_param, set_page_param) =
        query_signal_with_options::<String>("page", NavigateOptions::default());
    let current: Signal<&'static Page> =
        Signal::derive(move || help::page_or_first(page_param.get().as_deref()));
    let query = RwSignal::new(String::new());

    let go = Callback::new(move |raw: String| match target_of(&raw) {
        Some(Target::Page(id)) => set_page_param.set(Some(id)),
        Some(Target::Screen(path)) => navigate(&path, Default::default()),
        None => {}
    });
    let open_page = move |id: &'static str| {
        set_page_param.set(Some(id.to_owned()));
    };

    // A new page starts at the top.
    let scroller = NodeRef::<leptos::html::Div>::new();
    Effect::new(move |_| {
        current.track();
        if let Some(el) = scroller.get() {
            el.set_scroll_top(0);
        }
    });

    let list = move || {
        let q = query.get();
        if q.trim().is_empty() {
            help::groups()
                .into_iter()
                .map(|(heading, pages)| {
                    view! {
                        <div>
                            <h2 class="px-2 pt-3 pb-1 text-[10px] font-semibold uppercase tracking-wider text-muted">{heading}</h2>
                            <ul class="space-y-px">
                                {pages.into_iter().map(|p| view! { <PageLink page=p current=current on_open=open_page /> }).collect_view()}
                            </ul>
                        </div>
                    }
                })
                .collect_view()
                .into_any()
        } else {
            let hits = help::search(&q);
            if hits.is_empty() {
                view! { <p class="px-2 pt-3 text-muted">"No page matches. Try fewer or different words."</p> }.into_any()
            } else {
                view! {
                    <p class="px-2 pt-3 pb-1 text-[10px] uppercase tracking-wider text-muted">
                        {format!("{} page{}", hits.len(), if hits.len() == 1 { "" } else { "s" })}
                    </p>
                    <ul class="space-y-px">
                        {hits.into_iter().map(|h| {
                            let id = h.page.id;
                            view! {
                                <li>
                                    <button class=move || format!(
                                            "block w-full rounded-sm px-2 py-1.5 text-left hover:bg-hover {}",
                                            if current.get().id == id { "bg-active" } else { "" })
                                        on:click=move |_| open_page(id)>
                                        <span class="block font-medium">{h.page.title}</span>
                                        <span class="block text-[11px] leading-4 text-muted">{h.snippet}</span>
                                    </button>
                                </li>
                            }
                        }).collect_view()}
                    </ul>
                }
                .into_any()
            }
        }
    };

    let article = move || {
        let page = current.get();
        let parsed = markdown::parse(page.body);
        let jumps: Vec<(String, String)> = parsed
            .blocks
            .iter()
            .filter_map(|b| match b {
                Block::Heading { level: 2, text, id } => Some((id.clone(), text.clone())),
                _ => None,
            })
            .collect();
        let (before, after) = help::neighbours(page.id);
        view! {
            <h1 class="text-[20px] font-semibold leading-7">{page.title}</h1>
            <p class="mt-1 text-[13px] text-muted">{page.summary}</p>
            {(jumps.len() >= 3).then(|| view! {
                <nav class="mt-4 rounded-sm border border-line bg-panel px-3 py-2" aria-label="On this page">
                    <span class="text-[10px] font-semibold uppercase tracking-wider text-muted">"On this page"</span>
                    <ul class="mt-1 flex flex-wrap gap-x-4 gap-y-0.5 text-[12px]">
                        {jumps.into_iter().map(|(id, text)| view! {
                            <li><a href=format!("#{id}") class="text-accent hover:underline"
                                   on:click=move |ev: ev::MouseEvent| { ev.prevent_default(); scroll_to(&id); }>{text}</a></li>
                        }).collect_view()}
                    </ul>
                </nav>
            })}
            {blocks(parsed.blocks, go)}
            <div class="mt-10 flex items-center justify-between gap-3 border-t border-line pt-4 text-[12px]">
                {match before {
                    Some(p) => view! {
                        <button class="text-left text-accent hover:underline" on:click=move |_| open_page(p.id)>
                            "← " {p.title}
                        </button>
                    }.into_any(),
                    None => view! { <span></span> }.into_any(),
                }}
                {match after {
                    Some(p) => view! {
                        <button class="text-right text-accent hover:underline" on:click=move |_| open_page(p.id)>
                            {p.title} " →"
                        </button>
                    }.into_any(),
                    None => view! { <span></span> }.into_any(),
                }}
            </div>
        }
    };

    view! {
        <div class="flex h-full flex-col">
            <PageHeader icon="help" title="Help" subtitle="How to use Minimap, in detail">
                <Hints keys=&[("?", "open help"), ("g h", "go to help")] />
            </PageHeader>
            <div class="flex min-h-0 flex-1">
                <aside class="flex w-64 shrink-0 flex-col border-r border-line bg-panel" aria-label="Help pages">
                    <div class="border-b border-line p-2">
                        <input class=INPUT type="search" placeholder="Search the help" aria-label="Search the help"
                            prop:value=move || query.get()
                            on:input=move |ev| query.set(event_target_value(&ev)) />
                    </div>
                    <div class="flex-1 overflow-y-auto px-2 pb-3">{list}</div>
                </aside>
                <div class="min-w-0 flex-1 overflow-y-auto" node_ref=scroller>
                    <article class="mx-auto max-w-3xl px-8 py-6">{article}</article>
                </div>
            </div>
        </div>
    }
}

/// One entry of the page list.
#[component]
fn PageLink(
    page: &'static Page,
    current: Signal<&'static Page>,
    on_open: impl Fn(&'static str) + Copy + 'static,
) -> impl IntoView {
    let selected = move || current.get().id == page.id;
    view! {
        <li>
            <button
                class=move || format!(
                    "relative block w-full truncate rounded-sm py-1 pl-3 pr-2 text-left hover:bg-hover {}",
                    if selected() { "bg-active font-medium text-fg" } else { "text-muted hover:text-fg" })
                aria-current=move || if selected() { "page" } else { "false" }
                title=page.summary
                on:click=move |_| on_open(page.id)>
                {selected().then(|| view! { <span class="absolute inset-y-0 left-0 w-0.5 bg-fg"></span> })}
                {page.title}
            </button>
        </li>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_are_to_pages_or_screens() {
        assert!(matches!(target_of("help:tasks"), Some(Target::Page(id)) if id == "tasks"));
        assert!(
            matches!(target_of("app:/settings?tab=data"), Some(Target::Screen(p)) if p == "/settings?tab=data")
        );
        assert!(target_of("https://example.com").is_none());
        assert_eq!(href_of(&Target::Page("tasks".into())), "/help?page=tasks");
        assert_eq!(href_of(&Target::Screen("/graph".into())), "/graph");
    }
}
