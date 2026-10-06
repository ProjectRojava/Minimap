//! Detail-pane body for a note: title, date and kind, a Markdown editor that autosaves and
//! offers an `@` picker for people, projects and tasks, a preview, and the note's checklist.

use std::{str::FromStr, time::Duration};

use leptos::{html, prelude::*, task::spawn_local, web_sys};
use minimap_types::{
    mention_token, timefmt::parse_date, AppError, ChecklistItem, NodeRef, NodeSummary, NodeType,
    Note, NoteDetail, NoteKind, UpdateNote, Uuid,
};
use wasm_bindgen::JsCast;

use crate::{
    api,
    components::{
        attachments::{attach_files, files_of},
        detail_pane::Section,
        form::{SelectField, TextField, BUTTON, BUTTON_DANGER, BUTTON_ON, INPUT},
        people_panel::{error_line, NodeButtons},
        repeat_field::RepeatField,
    },
    mentions::{byte_to_utf16, insert_mention, mention_query, utf16_to_byte, MentionQuery},
    nav::type_label,
    state::{finish, DataVersion, Selection, Toasts},
};

/// Pause after the last keystroke before the note is saved.
const AUTOSAVE: Duration = Duration::from_millis(800);
const MAX_SUGGESTIONS: usize = 8;

pub fn kind_label(k: NoteKind) -> &'static str {
    match k {
        NoteKind::OneOnOne => "1:1",
        NoteKind::Meeting => "Meeting",
        NoteKind::General => "General",
    }
}

#[component]
pub fn NotePanel(id: Uuid) -> impl IntoView {
    // Loaded once per opening: the editor owns the text while it is open.
    let note = LocalResource::new(move || api::get_note(id));
    view! {
        {move || match note.get() {
            None => view! { <Section title="Note"><p class="text-muted">"Loading…"</p></Section> }.into_any(),
            Some(Err(e)) => view! { <Section title="Note">{error_line(e)}</Section> }.into_any(),
            Some(Ok(n)) => view! { <NoteEditor note=n /> }.into_any(),
        }}
        <ArchiveNote id=id />
    }
}

#[component]
fn NoteEditor(note: Note) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let selection = expect_context::<Selection>();
    let id = note.id;
    let recurrence = note.recurrence.clone();

    let detail = LocalResource::new(move || {
        version.track();
        api::get_note_detail(id)
    });
    // Everything an `@` can point at.
    let candidates = LocalResource::new(move || async move {
        let mut all: Vec<NodeSummary> = Vec::new();
        for t in [NodeType::Person, NodeType::Project, NodeType::Task] {
            all.extend(api::list_node_summaries(t).await?);
        }
        Ok::<_, AppError>(all)
    });

    // (Leptos' own `NodeRef`, not the app's node reference.)
    let area: leptos::prelude::NodeRef<html::Textarea> = leptos::prelude::NodeRef::new();
    let text = RwSignal::new(note.body.clone());
    let saved = RwSignal::new(note.body.clone());
    let status = RwSignal::new("");
    let preview = RwSignal::new(false);
    let rendered = RwSignal::new(String::new());
    let typing = RwSignal::new(None::<MentionQuery>);
    let choice = RwSignal::new(0usize);
    let tick = RwSignal::new(0u64);

    // Saves the body if it changed. Several saves in one editing session become one activity row.
    let save_now = move || {
        let body = text.get_untracked();
        if body == saved.get_untracked() {
            return;
        }
        status.set("Saving…");
        spawn_local(async move {
            let patch = UpdateNote {
                body: Some(body.clone()),
                ..Default::default()
            };
            match api::update_note(id, patch).await {
                Ok(_) => {
                    saved.set(body);
                    status.set("Saved");
                    version.bump();
                }
                Err(e) => {
                    status.set("Not saved");
                    toasts.error(&e);
                }
            }
        });
    };
    let schedule_save = move || {
        let n = tick.get_untracked() + 1;
        tick.set(n);
        set_timeout(
            move || {
                if tick.get_untracked() == n {
                    save_now();
                }
            },
            AUTOSAVE,
        );
    };

    // The `@word` being typed at the caret, if any.
    let refresh_picker = move || {
        let Some(el) = area.get() else {
            return;
        };
        let value = el.value();
        let utf16 = el.selection_start().ok().flatten().unwrap_or(0) as usize;
        let query = mention_query(&value, utf16_to_byte(&value, utf16));
        if query.is_some() && typing.get_untracked().is_none() {
            choice.set(0);
        }
        typing.set(query);
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

    let insert = move |picked: NodeSummary| {
        let Some(q) = typing.get_untracked() else {
            return;
        };
        let token = mention_token(&picked.label, picked.node.id);
        let current = area
            .get()
            .map(|el| el.value())
            .unwrap_or_else(|| text.get_untracked());
        let (new_text, caret) = insert_mention(&current, q.start, q.caret, &token);
        if let Some(el) = area.get() {
            el.set_value(&new_text);
            let pos = byte_to_utf16(&new_text, caret) as u32;
            let _ = el.set_selection_range(pos, pos);
            let _ = el.focus();
        }
        text.set(new_text);
        typing.set(None);
        schedule_save();
    };

    let on_keydown = move |ev: leptos::ev::KeyboardEvent| {
        let list = suggestions.get_untracked();
        if typing.get_untracked().is_none() || list.is_empty() {
            return;
        }
        let n = list.len();
        match ev.key().as_str() {
            "ArrowDown" => {
                ev.prevent_default();
                choice.update(|c| *c = (*c + 1) % n);
            }
            "ArrowUp" => {
                ev.prevent_default();
                choice.update(|c| *c = (*c + n - 1) % n);
            }
            "Enter" | "Tab" => {
                ev.prevent_default();
                let i = choice.get_untracked().min(n - 1);
                insert(list[i].clone());
            }
            "Escape" => {
                // Close the picker only; don't also leave the field.
                ev.prevent_default();
                ev.stop_propagation();
                typing.set(None);
            }
            _ => {}
        }
    };

    let show_preview = move |_| {
        save_now();
        let body = text.get_untracked();
        preview.set(true);
        spawn_local(async move {
            match api::render_markdown(body).await {
                Ok(html) => rendered.set(html),
                Err(e) => toasts.error(&e),
            }
        });
    };
    // Attached pictures in the preview get their address from the app's own protocol.
    let preview_el: leptos::prelude::NodeRef<html::Div> = leptos::prelude::NodeRef::new();
    Effect::new(move |_| {
        rendered.track();
        preview.track();
        request_animation_frame(move || {
            let Some(el) = preview_el.get() else { return };
            let Ok(images) = el.query_selector_all("img[data-attachment]") else {
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
    // Files pasted or dropped into the text become attachments of this note, and a link to each
    // is put in at the caret.
    let attach_here = move |files: Vec<web_sys::File>| {
        if files.is_empty() {
            return;
        }
        spawn_local(async move {
            let node = NodeRef::new(NodeType::Note, id);
            let added = attach_files(node, files, toasts).await;
            if added.is_empty() {
                return;
            }
            let Some(el) = area.get() else { return };
            let current = el.value();
            let caret = utf16_to_byte(
                &current,
                el.selection_start().ok().flatten().unwrap_or(0) as usize,
            );
            let links: String = added
                .iter()
                .map(|a| a.markdown.clone())
                .collect::<Vec<_>>()
                .join("\n");
            let before = &current[..caret];
            let lead = if before.is_empty() || before.ends_with('\n') {
                ""
            } else {
                "\n"
            };
            let new_text = format!("{before}{lead}{links}\n{}", &current[caret..]);
            el.set_value(&new_text);
            let pos = byte_to_utf16(&new_text, caret + lead.len() + links.len() + 1) as u32;
            let _ = el.set_selection_range(pos, pos);
            text.set(new_text);
            schedule_save();
            version.bump();
        });
    };
    let on_paste = move |ev: leptos::ev::ClipboardEvent| {
        let files = files_of(ev.clipboard_data().and_then(|d| d.files()));
        if !files.is_empty() {
            // A pasted picture (a screenshot) is a file, not text.
            ev.prevent_default();
            attach_here(files);
        }
    };
    let on_drop = move |ev: leptos::ev::DragEvent| {
        let files = files_of(ev.data_transfer().and_then(|d| d.files()));
        if !files.is_empty() {
            ev.prevent_default();
            attach_here(files);
        }
    };
    // Mentions in the preview open the node they point at; attachment links open the file.
    let open_mention = move |ev: leptos::ev::MouseEvent| {
        let target = ev
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok());
        if let Some(a) = target
            .as_ref()
            .and_then(|el| el.closest("a.attachment-link").ok().flatten())
        {
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
            return;
        }
        let anchor = target.and_then(|el| el.closest("a.mention").ok().flatten());
        if let Some(a) = anchor {
            let kind = a
                .get_attribute("data-node-type")
                .and_then(|t| NodeType::from_str(&t).ok());
            let node_id = a
                .get_attribute("data-node-id")
                .and_then(|i| Uuid::parse_str(&i).ok());
            if let (Some(t), Some(i)) = (kind, node_id) {
                selection.open(NodeRef::new(t, i));
            }
        }
    };

    let save_field = move |patch: UpdateNote| {
        spawn_local(async move {
            finish(api::update_note(id, patch).await, toasts, version);
        });
    };
    let save_date = move |v: String| match parse_date(v.trim()) {
        Ok(d) => save_field(UpdateNote {
            note_date: Some(d),
            ..Default::default()
        }),
        Err(_) => {
            toasts.error(&AppError {
                code: "invalid".into(),
                message: "Date must look like 2027-03-31".into(),
            });
            version.bump();
        }
    };
    let save_kind = move |v: String| {
        if let Ok(k) = NoteKind::from_str(&v) {
            save_field(UpdateNote {
                kind: Some(k),
                ..Default::default()
            });
        }
    };
    let kind_options: Vec<(String, String)> = NoteKind::ALL
        .iter()
        .map(|k| (k.as_str().to_owned(), kind_label(*k).to_owned()))
        .collect();

    // Converting reads the stored body, so flush edits first; the body then changes on the server.
    let convert = move |item: ChecklistItem| {
        spawn_local(async move {
            let body = text.get_untracked();
            if body != saved.get_untracked() {
                let patch = UpdateNote {
                    body: Some(body.clone()),
                    ..Default::default()
                };
                if let Err(e) = api::update_note(id, patch).await {
                    toasts.error(&e);
                    return;
                }
                saved.set(body);
            }
            match api::convert_checklist_item(id, item.line, item.text).await {
                Ok(task) => {
                    if let Ok(n) = api::get_note(id).await {
                        if let Some(el) = area.get() {
                            el.set_value(&n.body);
                        }
                        text.set(n.body.clone());
                        saved.set(n.body);
                    }
                    toasts.info(format!("Task created: {}", task.title));
                }
                Err(e) => toasts.error(&e),
            }
            version.bump();
        });
    };

    let initial = note.body.clone();
    view! {
        <Section title="Note">
            <TextField label="Title" value=note.title.clone()
                on_commit=move |v: String| save_field(UpdateNote { title: Some(v), ..Default::default() }) />
            <div class="grid grid-cols-2 gap-3">
                <TextField label="Date" kind="date" value=note.note_date.to_string() on_commit=save_date />
                <SelectField label="Kind" options=kind_options current=note.kind.as_str().to_owned() on_change=save_kind />
            </div>
            <RepeatField node=NodeRef::new(NodeType::Note, id) current=recurrence />
            <div class="mt-3 mb-1 flex items-center gap-2">
                <button class=move || format!("{BUTTON} {}", if preview.get() { "" } else { BUTTON_ON })
                        on:click=move |_| preview.set(false)>"Write"</button>
                <button class=move || format!("{BUTTON} {}", if preview.get() { BUTTON_ON } else { "" })
                        on:click=show_preview>"Preview"</button>
                <span class="ml-auto text-[11px] text-muted">{move || status.get()}</span>
            </div>
            <div class=move || if preview.get() { "hidden" } else { "block" }>
                <textarea node_ref=area class=format!("{INPUT} font-mono") rows="16" spellcheck="true"
                    placeholder="Markdown. Type @ to mention a person, project or task; start a line with [ ] for a checklist item."
                    prop:value=initial
                    on:input=move |ev| {
                        text.set(event_target_value(&ev));
                        status.set("");
                        schedule_save();
                        refresh_picker();
                    }
                    on:click=move |_| refresh_picker()
                    on:paste=on_paste
                    on:drop=on_drop
                    on:blur=move |_| { typing.set(None); save_now(); }
                    on:keydown=on_keydown></textarea>
                <Show when=move || !suggestions.get().is_empty()>
                    <ul class="mt-1 rounded-sm border border-line bg-panel" role="listbox">
                        {move || suggestions.get().into_iter().enumerate().map(|(i, s)| {
                            let picked = s.clone();
                            view! {
                                <li role="option">
                                    <button class=move || format!(
                                                "flex w-full gap-2 px-2 py-0.5 text-left {}",
                                                if choice.get() == i { "bg-active" } else { "hover:bg-hover" })
                                            // mousedown (not click) so the textarea keeps focus
                                            on:mousedown=move |ev| { ev.prevent_default(); insert(picked.clone()); }>
                                        <span class="w-14 shrink-0 text-faint">{type_label(s.node.node_type)}</span>
                                        <span class="truncate">{s.label}</span>
                                    </button>
                                </li>
                            }
                        }).collect_view()}
                    </ul>
                </Show>
            </div>
            <Show when=move || preview.get()>
                <div node_ref=preview_el class="md min-h-24 rounded-sm border border-line p-2" on:click=open_mention
                     inner_html=move || rendered.get()></div>
            </Show>
        </Section>
        {move || match detail.get() {
            Some(Ok(d)) => view! { <NoteDetails detail=d on_convert=convert /> }.into_any(),
            Some(Err(e)) => view! { <Section title="Checklist">{error_line(e)}</Section> }.into_any(),
            None => ().into_any(),
        }}
    }
}

#[component]
fn NoteDetails(
    detail: NoteDetail,
    #[prop(into)] on_convert: Callback<ChecklistItem>,
) -> impl IntoView {
    let mentions = detail.mentions.clone();
    let items = detail.checklist.clone();
    view! {
        {(!items.is_empty()).then(|| view! {
            <Section title="Checklist">
                <ul class="space-y-1">
                    {items.into_iter().map(|item| {
                        let label = item.text.clone();
                        view! {
                            <li class="flex items-center gap-2">
                                <span class="min-w-0 flex-1 truncate">{label}</span>
                                <button class=BUTTON on:click=move |_| on_convert.run(item.clone())>"Convert to task"</button>
                            </li>
                        }
                    }).collect_view()}
                </ul>
            </Section>
        })}
        {(!mentions.is_empty()).then(|| view! {
            <Section title="Mentions"><NodeButtons nodes=mentions.clone() /></Section>
        })}
    }
}

#[component]
fn ArchiveNote(id: Uuid) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let selection = expect_context::<Selection>();
    let confirming = RwSignal::new(false);
    let confirm = move |_| {
        spawn_local(async move {
            if finish(api::archive_note(id).await, toasts, version).is_some() {
                confirming.set(false);
                selection.close();
            }
        });
    };
    view! {
        <Section title="Archive">
            {move || if confirming.get() {
                view! {
                    <div class="space-y-2">
                        <p>"Archive this note? Its links are archived with it."</p>
                        <div class="flex gap-2">
                            <button class=BUTTON_DANGER on:click=confirm>"Archive"</button>
                            <button class=BUTTON on:click=move |_| confirming.set(false)>"Cancel"</button>
                        </div>
                    </div>
                }.into_any()
            } else {
                view! { <button class=BUTTON on:click=move |_| confirming.set(true)>"Archive note…"</button> }.into_any()
            }}
        </Section>
    }
}
