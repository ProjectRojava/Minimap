//! The Markdown box, used for every long piece of text a person writes (a note, a description, a
//! note on a task): a `Write | Preview` pair of tabs, a toolbar like the one on a GitHub comment
//! (heading, bold, italic, quote, code, link, lists, task list, `@` mention, attach a file), an `@`
//! picker for people, projects and tasks, and files pasted or dropped into it.
//!
//! - [`MarkdownBox`]: the editor. The caller owns the text (a signal) and decides when it is saved.
//! - [`MarkdownView`]: formatted text, with mentions, attachments and links that open.
//! - [`MarkdownField`]: a field that reads as formatted text, with a pencil to edit it and
//!   *Save* / *Cancel* (the description of a task, project, objective ...).

use std::str::FromStr;

use leptos::{ev, html, prelude::*, task::spawn_local, web_sys};
use minimap_types::{mention_token, AppError, NodeRef as Node, NodeSummary, NodeType, Uuid};
use wasm_bindgen::JsCast;

use crate::{
    api,
    components::{
        attachments::{attach_files, files_of},
        form::{BUTTON, BUTTON_ON, BUTTON_PRIMARY},
        page::Icon,
    },
    md_edit::{self, Block, Sel, Splice},
    mentions::{byte_to_utf16, mention_query, utf16_to_byte, MentionQuery},
    nav::type_label,
    state::{DataVersion, Selection, Toasts},
};

const MAX_SUGGESTIONS: usize = 8;
const TOOL: &str = "flex h-6 min-w-6 items-center justify-center rounded-sm px-1 text-[12px] \
    text-muted hover:bg-hover hover:text-fg";

/// Puts `s` in place of the textarea's selection through the browser's own edit command, so the
/// box's native undo (Ctrl/Cmd+Z) still steps back through toolbar edits. `false` when the
/// browser would not.
fn exec_insert(s: &str) -> bool {
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
        return false;
    };
    let Ok(html) = doc.dyn_into::<web_sys::HtmlDocument>() else {
        return false;
    };
    if s.is_empty() {
        html.exec_command("delete").unwrap_or(false)
    } else {
        html.exec_command_with_show_ui_and_value("insertText", false, s)
            .unwrap_or(false)
    }
}

/// Opens what was clicked in formatted text: a mention opens its item in the pane, an attached
/// file opens, a web or mail link opens in the default program.
fn follow(ev: &ev::MouseEvent, selection: Selection, toasts: Toasts) {
    let Some(target) = ev
        .target()
        .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
    else {
        return;
    };
    let closest = |sel: &str| target.closest(sel).ok().flatten();
    if let Some(a) = closest("a.attachment-link") {
        if let Some(file) = a
            .get_attribute("data-attachment")
            .and_then(|v| Uuid::parse_str(&v).ok())
        {
            spawn_local(async move {
                if let Err(e) = api::open_attachment(file).await {
                    toasts.error(&e);
                }
            });
        }
    } else if let Some(a) = closest("a.mention") {
        let kind = a
            .get_attribute("data-node-type")
            .and_then(|t| NodeType::from_str(&t).ok());
        let id = a
            .get_attribute("data-node-id")
            .and_then(|i| Uuid::parse_str(&i).ok());
        if let (Some(t), Some(i)) = (kind, id) {
            selection.open(Node::new(t, i));
        }
    } else if let Some(link) = closest("span.link") {
        if let Some(url) = link.get_attribute("title") {
            spawn_local(async move {
                if let Err(e) = api::open_link(url).await {
                    toasts.error(&e);
                }
            });
        }
    }
}

/// Formatted text. Mentions show the item's current name and open it; attached pictures show;
/// links open in the default program. Nothing is rendered for empty text (`empty` is shown).
#[component]
pub fn MarkdownView(
    #[prop(into)] text: Signal<String>,
    #[prop(default = "")] empty: &'static str,
    #[prop(default = "")] class: &'static str,
) -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    let selection = expect_context::<Selection>();
    let rendered = RwSignal::new(String::new());
    let turn = StoredValue::new(0u64);
    let el: leptos::prelude::NodeRef<html::Div> = leptos::prelude::NodeRef::new();

    Effect::new(move |_| {
        let body = text.get();
        let n = turn.get_value() + 1;
        turn.set_value(n);
        if body.trim().is_empty() {
            rendered.set(String::new());
            return;
        }
        spawn_local(async move {
            match api::render_markdown(body).await {
                // A slower, older answer must not replace a newer one.
                Ok(html) if turn.get_value() == n => rendered.set(html),
                Ok(_) => {}
                Err(e) => toasts.error(&e),
            }
        });
    });
    // Attached pictures get their address from the app's own protocol.
    Effect::new(move |_| {
        rendered.track();
        request_animation_frame(move || {
            let Some(root) = el.get() else { return };
            let Ok(images) = root.query_selector_all("img[data-attachment]") else {
                return;
            };
            for i in 0..images.length() {
                let Some(img) = images
                    .item(i)
                    .and_then(|n| n.dyn_into::<web_sys::Element>().ok())
                else {
                    continue;
                };
                if let Some(id) = img
                    .get_attribute("data-attachment")
                    .and_then(|v: String| Uuid::parse_str(&v).ok())
                {
                    let _ = img.set_attribute("src", &api::attachment_url(id));
                }
            }
        });
    });

    view! {
        {move || text.with(|t| t.trim().is_empty()).then(|| view! {
            <p class="text-muted">{empty}</p>
        })}
        <div node_ref=el class=format!("md {class}")
             class:hidden=move || text.with(|t| t.trim().is_empty())
             on:click=move |ev| follow(&ev, selection, toasts)
             inner_html=move || rendered.get()></div>
    }
}

/// The tools of the box's toolbar, in order. `None` is a divider.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Heading,
    Bold,
    Italic,
    Quote,
    Code,
    Link,
    Bullets,
    Numbers,
    Tasks,
    Mention,
    Attach,
}

/// The editor: tabs, toolbar, text box, `@` picker, preview. The caller owns `text` and decides
/// when it is saved (`on_change` after each edit, `on_blur` when the box loses focus or the
/// preview tab opens, `on_submit` on Ctrl/Cmd+Enter, `on_cancel` on Escape).
#[component]
pub fn MarkdownBox(
    text: RwSignal<String>,
    /// The item a pasted, dropped or attached file belongs to; without it the file tools are off.
    #[prop(optional)]
    attach_to: Option<Node>,
    #[prop(default = "")] placeholder: &'static str,
    #[prop(default = 8)] rows: u32,
    #[prop(optional)] on_change: Option<Callback<()>>,
    #[prop(optional)] on_blur: Option<Callback<()>>,
    #[prop(optional)] on_submit: Option<Callback<()>>,
    #[prop(optional)] on_cancel: Option<Callback<()>>,
    #[prop(optional)] autofocus: bool,
) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let area: leptos::prelude::NodeRef<html::Textarea> = leptos::prelude::NodeRef::new();
    let preview = RwSignal::new(false);
    let typing = RwSignal::new(None::<MentionQuery>);
    let choice = RwSignal::new(0usize);
    let wanted = RwSignal::new(false);

    // Everything an `@` can point at, fetched the first time one is typed.
    let candidates = LocalResource::new(move || async move {
        let mut all: Vec<NodeSummary> = Vec::new();
        if wanted.get() {
            for t in [NodeType::Person, NodeType::Project, NodeType::Task] {
                all.extend(api::list_node_summaries(t).await?);
            }
        }
        Ok::<_, AppError>(all)
    });

    let changed = move || {
        if let Some(f) = on_change {
            f.run(());
        }
    };
    // The text and the selection (in bytes), from the box.
    let state = move || -> Option<(String, Sel)> {
        let el = area.get()?;
        let value = el.value();
        let a = el.selection_start().ok().flatten().unwrap_or(0) as usize;
        let b = el.selection_end().ok().flatten().unwrap_or(0) as usize;
        let sel = (utf16_to_byte(&value, a), utf16_to_byte(&value, b));
        Some((value, sel))
    };
    let refresh_picker = move || {
        let Some((value, sel)) = state() else { return };
        let query = (sel.0 == sel.1)
            .then(|| mention_query(&value, sel.0))
            .flatten();
        if query.is_some() {
            wanted.set(true);
            if typing.get_untracked().is_none() {
                choice.set(0);
            }
        }
        typing.set(query);
    };

    // Applies an edit through the browser, so native undo works, then puts the selection where
    // the edit says. If the browser's result is not exactly the edit (it should be), the text is
    // set directly.
    let apply = move |s: Splice| {
        let Some(el) = area.get() else { return };
        let current = el.value();
        let expected = s.apply(&current);
        let _ = el.focus();
        let from = byte_to_utf16(&current, s.from) as u32;
        let to = byte_to_utf16(&current, s.to) as u32;
        let _ = el.set_selection_range(from, to);
        if !(s.from == s.to && s.insert.is_empty()) {
            exec_insert(&s.insert);
        }
        if el.value() != expected {
            el.set_value(&expected);
        }
        let now = el.value();
        let _ = el.set_selection_range(
            byte_to_utf16(&now, s.select.0) as u32,
            byte_to_utf16(&now, s.select.1) as u32,
        );
        text.set(now);
        changed();
        refresh_picker();
    };
    let edit = move |make: &dyn Fn(&str, Sel) -> Splice| {
        if let Some((value, sel)) = state() {
            apply(make(&value, sel));
        }
    };

    let suggestions = Memo::new(move |_| -> Vec<NodeSummary> {
        let Some(q) = typing.get() else {
            return Vec::new();
        };
        let needle = q.query.to_lowercase();
        match candidates.get() {
            Some(Ok(all)) => all
                .into_iter()
                .filter(|c| c.label.to_lowercase().contains(&needle))
                .take(MAX_SUGGESTIONS)
                .collect(),
            _ => Vec::new(),
        }
    });
    let insert_mention = move |picked: NodeSummary| {
        let Some(q) = typing.get_untracked() else {
            return;
        };
        let token = mention_token(&picked.label, picked.node.id);
        let current = area.get().map(|el| el.value()).unwrap_or_default();
        typing.set(None);
        apply(md_edit::pick_mention(&current, q.start, q.caret, &token));
    };

    // Files pasted, dropped or picked become attachments of `attach_to`, linked at the caret.
    let put_links = move |added: Vec<minimap_types::Attachment>| {
        if added.is_empty() {
            return;
        }
        let links = added
            .iter()
            .map(|a| a.markdown.clone())
            .collect::<Vec<_>>()
            .join("\n");
        edit(&|t: &str, sel: Sel| md_edit::insert_on_own_line(t, sel, &links));
        version.bump();
    };
    let attach_here = move |files: Vec<web_sys::File>| {
        let Some(node) = attach_to else { return };
        if files.is_empty() {
            return;
        }
        spawn_local(async move {
            put_links(attach_files(node, files, toasts).await);
        });
    };
    let pick_file = move || {
        let Some(node) = attach_to else { return };
        spawn_local(async move {
            match api::pick_attachment().await {
                Ok(Some(path)) => match api::add_attachment(node, path).await {
                    Ok(a) => put_links(vec![a]),
                    Err(e) => toasts.error(&e),
                },
                Ok(None) => {}
                Err(e) => toasts.error(&e),
            }
        });
    };
    let on_paste = move |ev: ev::ClipboardEvent| {
        let files = files_of(ev.clipboard_data().and_then(|d| d.files()));
        if attach_to.is_some() && !files.is_empty() {
            // A pasted picture (a screenshot) is a file, not text.
            ev.prevent_default();
            attach_here(files);
        }
    };
    let on_drop = move |ev: ev::DragEvent| {
        let files = files_of(ev.data_transfer().and_then(|d| d.files()));
        if attach_to.is_some() && !files.is_empty() {
            ev.prevent_default();
            attach_here(files);
        }
    };

    let run_tool = move |tool: Tool| match tool {
        Tool::Heading => edit(&(|t, s| md_edit::block(t, s, Block::Heading))),
        Tool::Bold => edit(&(|t, s| md_edit::wrap(t, s, "**", "**", "bold text"))),
        Tool::Italic => edit(&(|t, s| md_edit::wrap(t, s, "_", "_", "italic text"))),
        Tool::Quote => edit(&(|t, s| md_edit::block(t, s, Block::Quote))),
        Tool::Code => edit(&md_edit::code),
        Tool::Link => edit(&md_edit::link),
        Tool::Bullets => edit(&(|t, s| md_edit::block(t, s, Block::Bullet))),
        Tool::Numbers => edit(&(|t, s| md_edit::block(t, s, Block::Numbered))),
        Tool::Tasks => edit(&(|t, s| md_edit::block(t, s, Block::Task))),
        Tool::Mention => edit(&md_edit::mention),
        Tool::Attach => pick_file(),
    };

    let on_keydown = move |ev: ev::KeyboardEvent| {
        let key = ev.key();
        // The picker owns the arrows, Enter, Tab and Escape while it is open.
        let list = suggestions.get_untracked();
        if typing.get_untracked().is_some() && !list.is_empty() {
            let n = list.len();
            match key.as_str() {
                "ArrowDown" => {
                    ev.prevent_default();
                    choice.update(|c| *c = (*c + 1) % n);
                    return;
                }
                "ArrowUp" => {
                    ev.prevent_default();
                    choice.update(|c| *c = (*c + n - 1) % n);
                    return;
                }
                "Enter" | "Tab" => {
                    ev.prevent_default();
                    insert_mention(list[choice.get_untracked().min(n - 1)].clone());
                    return;
                }
                "Escape" => {
                    ev.prevent_default();
                    ev.stop_propagation();
                    typing.set(None);
                    return;
                }
                _ => {}
            }
        }
        let command = ev.ctrl_key() || ev.meta_key();
        match key.as_str() {
            "Enter" if command => {
                if let Some(f) = on_submit {
                    ev.prevent_default();
                    f.run(());
                }
            }
            "Escape" => {
                if let Some(f) = on_cancel {
                    ev.prevent_default();
                    ev.stop_propagation();
                    f.run(());
                }
            }
            "b" | "B" if command && !ev.shift_key() => {
                ev.prevent_default();
                run_tool(Tool::Bold);
            }
            "i" | "I" if command && !ev.shift_key() => {
                ev.prevent_default();
                run_tool(Tool::Italic);
            }
            // Enter in a list or quote carries the list on.
            "Enter" if !command && !ev.shift_key() && !ev.alt_key() => {
                if let Some((value, sel)) = state() {
                    if sel.0 == sel.1 {
                        if let Some(s) = md_edit::continue_list(&value, sel.0) {
                            ev.prevent_default();
                            apply(s);
                        }
                    }
                }
            }
            _ => {}
        }
    };

    let show_preview = move |on: bool| {
        if on {
            if let Some(f) = on_blur {
                f.run(());
            }
        }
        preview.set(on);
        if !on {
            request_animation_frame(move || {
                if let Some(el) = area.get() {
                    let _ = el.focus();
                }
            });
        }
    };
    if autofocus {
        request_animation_frame(move || {
            if let Some(el) = area.get() {
                let _ = el.focus();
            }
        });
    }

    let tab = move |name: &'static str, is_preview: bool| {
        view! {
            <button type="button"
                class=move || format!("{BUTTON} {}", if preview.get() == is_preview { BUTTON_ON } else { "" })
                on:click=move |_| show_preview(is_preview)>{name}</button>
        }
    };
    let tool = move |which: Tool, title: &'static str, label: AnyView| {
        view! {
            <button type="button" class=TOOL title=title aria-label=title
                // mousedown would take the focus (and the selection) away from the text box
                on:mousedown=|ev| ev.prevent_default()
                on:click=move |_| run_tool(which)>{label}</button>
        }
    };
    let divider = || view! { <span class="mx-0.5 h-4 w-px bg-line" aria-hidden="true"></span> };
    let bold = || view! { <span class="font-bold">"B"</span> }.into_any();
    let italic = || view! { <span class="italic">"I"</span> }.into_any();
    let plain = |s: &'static str| view! { <span>{s}</span> }.into_any();

    view! {
        <div class="rounded-sm border border-line bg-canvas focus-within:border-accent">
            <div class="flex flex-wrap items-center gap-1 border-b border-line bg-panel px-1.5 py-1">
                {tab("Write", false)}
                {tab("Preview", true)}
                <div class="ml-auto flex flex-wrap items-center gap-0.5" class:hidden=move || preview.get()>
                    {tool(Tool::Heading, "Heading", plain("H"))}
                    {tool(Tool::Bold, "Bold (Ctrl/Cmd+B)", bold())}
                    {tool(Tool::Italic, "Italic (Ctrl/Cmd+I)", italic())}
                    {divider()}
                    {tool(Tool::Quote, "Quote", plain("❝"))}
                    {tool(Tool::Code, "Code", plain("<>"))}
                    {tool(Tool::Link, "Link", view! { <Icon name="chain" size="h-3.5 w-3.5" /> }.into_any())}
                    {divider()}
                    {tool(Tool::Bullets, "Bulleted list", plain("•"))}
                    {tool(Tool::Numbers, "Numbered list", plain("1."))}
                    {tool(Tool::Tasks, "Task list", plain("☑"))}
                    {divider()}
                    {tool(Tool::Mention, "Mention a person, project or task", plain("@"))}
                    {attach_to.is_some().then(|| tool(Tool::Attach, "Attach a file", view! { <Icon name="attach" size="h-3.5 w-3.5" /> }.into_any()))}
                </div>
            </div>
            <textarea node_ref=area
                class=move || format!(
                    "block w-full resize-y bg-transparent px-2 py-1.5 font-mono text-[13px] text-fg focus:outline-none {}",
                    if preview.get() { "hidden" } else { "" })
                rows=rows.to_string() spellcheck="true" placeholder=placeholder
                aria-label="Text"
                prop:value=move || text.get()
                on:input=move |ev| {
                    text.set(event_target_value(&ev));
                    changed();
                    refresh_picker();
                }
                on:click=move |_| refresh_picker()
                on:paste=on_paste
                on:drop=on_drop
                on:blur=move |_| {
                    typing.set(None);
                    if let Some(f) = on_blur { f.run(()); }
                }
                on:keydown=on_keydown></textarea>
            <Show when=move || preview.get()>
                <div class="min-h-24 px-2 py-1.5">
                    <MarkdownView text=text empty="Nothing to preview." />
                </div>
            </Show>
            <Show when=move || !suggestions.get().is_empty()>
                <ul class="border-t border-line bg-panel" role="listbox">
                    {move || suggestions.get().into_iter().enumerate().map(|(i, s)| {
                        let picked = s.clone();
                        view! {
                            <li role="option">
                                <button class=move || format!(
                                            "flex w-full gap-2 px-2 py-0.5 text-left {}",
                                            if choice.get() == i { "bg-active" } else { "hover:bg-hover" })
                                        // mousedown (not click) so the text box keeps focus
                                        on:mousedown=move |ev| { ev.prevent_default(); insert_mention(picked.clone()); }>
                                    <span class="w-14 shrink-0 text-faint">{type_label(s.node.node_type)}</span>
                                    <span class="truncate">{s.label}</span>
                                </button>
                            </li>
                        }
                    }).collect_view()}
                </ul>
            </Show>
        </div>
    }
}

/// A `save` callback for [`MarkdownField`] from an async function that says whether it saved.
pub fn saver<F, Fut>(save: F) -> Callback<(String, Callback<bool>)>
where
    F: Fn(String) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = bool> + 'static,
{
    Callback::new(move |(text, done): (String, Callback<bool>)| {
        let run = save(text);
        spawn_local(async move { done.run(run.await) });
    })
}

/// A field that reads as formatted text (the description of a task, project, objective ...).
/// The pencil (or a double-click) opens the box; *Save* hands the text to `save` together with a
/// callback to say whether it was saved; *Cancel* (or Escape, when nothing was typed) puts it
/// back as it was. Ctrl/Cmd+Enter saves.
#[component]
pub fn MarkdownField(
    label: &'static str,
    value: String,
    /// The item the text belongs to (files attach to it).
    node: Node,
    #[prop(into)] save: Callback<(String, Callback<bool>)>,
    #[prop(default = "Nothing written yet.")] empty: &'static str,
    #[prop(default = "")] placeholder: &'static str,
    #[prop(default = 6)] rows: u32,
) -> impl IntoView {
    let saved = RwSignal::new(value);
    let editing = RwSignal::new(false);
    let draft = RwSignal::new(String::new());
    let busy = RwSignal::new(false);

    let start = move || {
        draft.set(saved.get_untracked());
        editing.set(true);
    };
    let cancel = move || editing.set(false);
    let submit = move || {
        if busy.get_untracked() {
            return;
        }
        let text = draft.get_untracked();
        if text == saved.get_untracked() {
            return cancel();
        }
        busy.set(true);
        save.run((
            text.clone(),
            Callback::new(move |ok: bool| {
                busy.set(false);
                if ok {
                    saved.set(text.clone());
                    editing.set(false);
                }
            }),
        ));
    };
    // Escape only gives up an edit that has nothing in it to lose.
    let escape = move || {
        if draft.get_untracked() == saved.get_untracked() {
            cancel();
        }
    };

    view! {
        <div class="mb-2">
            <div class="mb-0.5 flex items-center">
                <span class="text-[11px] text-muted">{label}</span>
                <Show when=move || !editing.get()>
                    <button type="button" class=format!("{TOOL} ml-auto")
                        title=format!("Edit {}", label.to_lowercase())
                        aria-label=format!("Edit {}", label.to_lowercase())
                        on:click=move |_| start()>
                        <Icon name="edit" size="h-3.5 w-3.5" />
                    </button>
                </Show>
            </div>
            {move || if editing.get() {
                view! {
                    <MarkdownBox text=draft attach_to=node placeholder=placeholder rows=rows
                        autofocus=true
                        on_submit=Callback::new(move |_| submit())
                        on_cancel=Callback::new(move |_| escape()) />
                    <div class="mt-1.5 flex items-center gap-2">
                        <button class=BUTTON_PRIMARY disabled=move || busy.get() on:click=move |_| submit()>"Save"</button>
                        <button class=BUTTON on:click=move |_| cancel()>"Cancel"</button>
                        <span class="text-[11px] text-muted">"Ctrl/Cmd+Enter saves"</span>
                    </div>
                }.into_any()
            } else {
                view! {
                    <div class="min-h-6 rounded-sm px-1 py-0.5 hover:bg-hover/40" on:dblclick=move |_| start()>
                        <MarkdownView text=saved empty=empty />
                    </div>
                }.into_any()
            }}
        </div>
    }
}
