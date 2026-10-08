//! Detail-pane body for a note: title, date and kind, a Markdown editor that autosaves and
//! offers an `@` picker for people, projects and tasks, a preview, and the note's checklist. A
//! note with text opens as formatted text with a pencil to edit it; an empty one opens for typing.

use std::{str::FromStr, time::Duration};

use leptos::{prelude::*, task::spawn_local};
use minimap_types::{
    timefmt::parse_date, AppError, ChecklistItem, NodeRef, NodeType, Note, NoteDetail, NoteKind,
    UpdateNote, Uuid,
};

use crate::{
    api,
    components::{
        detail_pane::Section,
        form::{SelectField, TextField, BUTTON, BUTTON_DANGER, BUTTON_SOFT},
        markdown_box::{MarkdownBox, MarkdownView},
        page::Icon,
        people_panel::{error_line, NodeButtons},
        repeat_field::RepeatField,
    },
    state::{finish, DataVersion, Selection, Toasts},
};

/// Pause after the last keystroke before the note is saved.
const AUTOSAVE: Duration = Duration::from_millis(800);

pub fn kind_label(k: NoteKind) -> &'static str {
    match k {
        NoteKind::OneOnOne => "1:1",
        NoteKind::Meeting => "Meeting",
        NoteKind::General => "General",
    }
}

/// A note opens as text to read; only one with nothing written yet opens ready to type in.
pub fn starts_editing(body: &str) -> bool {
    body.trim().is_empty()
}

#[component]
pub fn NotePanel(id: Uuid) -> impl IntoView {
    // Loaded once per opening: the editor owns the text while it is open.
    let note = LocalResource::new(move || api::get_note(id));
    view! {
        {move || match note.get() {
            None => view! { <Section title="Note" always_open=true><p class="text-muted">"Loading…"</p></Section> }.into_any(),
            Some(Err(e)) => view! { <Section title="Note" always_open=true>{error_line(e)}</Section> }.into_any(),
            Some(Ok(n)) => view! { <NoteEditor note=n /> }.into_any(),
        }}
        <ArchiveNote id=id />
    }
}

#[component]
fn NoteEditor(note: Note) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let id = note.id;
    let recurrence = note.recurrence.clone();

    let detail = LocalResource::new(move || {
        version.track();
        api::get_note_detail(id)
    });

    let text = RwSignal::new(note.body.clone());
    let saved = RwSignal::new(note.body.clone());
    let status = RwSignal::new("");
    // Read first: the note shows as formatted text with a pencil to edit it.
    let editing = RwSignal::new(starts_editing(&note.body));
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
    // Done: save what was typed and go back to reading it.
    let done = move || {
        save_now();
        editing.set(false);
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

    view! {
        <Section title="Note" always_open=true>
            <TextField label="Title" value=note.title.clone()
                on_commit=move |v: String| save_field(UpdateNote { title: Some(v), ..Default::default() }) />
            <div class="grid grid-cols-2 gap-3">
                <TextField label="Date" kind="date" value=note.note_date.to_string() on_commit=save_date />
                <SelectField label="Kind" options=kind_options current=note.kind.as_str().to_owned() on_change=save_kind />
            </div>
            <RepeatField node=NodeRef::new(NodeType::Note, id) current=recurrence />
            <div class="mt-3 mb-1 flex items-center gap-2">
                <span class="text-[11px] text-muted">{move || status.get()}</span>
                <span class="ml-auto">
                    {move || if editing.get() {
                        view! {
                            <button class=BUTTON_SOFT title="Done editing" aria-label="Done editing"
                                    on:click=move |_| done()>
                                <span class="flex items-center gap-1">
                                    <Icon name="check" size="h-3.5 w-3.5" />"Done"
                                </span>
                            </button>
                        }.into_any()
                    } else {
                        view! {
                            <button class=format!("{BUTTON} px-1.5") title="Edit this note" aria-label="Edit this note"
                                    on:click=move |_| editing.set(true)>
                                <Icon name="edit" size="h-3.5 w-3.5" />
                            </button>
                        }.into_any()
                    }}
                </span>
            </div>
            {move || if editing.get() {
                view! {
                    <MarkdownBox text=text attach_to=NodeRef::new(NodeType::Note, id) rows=16 autofocus=true
                        placeholder="Markdown. Type @ to mention a person, project or task; start a line with [ ] for a checklist item."
                        on_change=Callback::new(move |_| { status.set(""); schedule_save(); })
                        on_blur=Callback::new(move |_| save_now())
                        on_submit=Callback::new(move |_| done()) />
                }.into_any()
            } else {
                view! {
                    <div class="rounded-sm border border-line p-2" on:dblclick=move |_| editing.set(true)
                         title="Double-click to edit">
                        <MarkdownView text=text empty="Nothing written yet." class="min-h-20" />
                    </div>
                }.into_any()
            }}
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
        <Section title="Archive" collapsed=true tone=crate::components::page::Tone::Danger>
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_with_text_opens_to_read_and_an_empty_one_opens_to_write() {
        assert!(!starts_editing("Use PAN as identifier"));
        assert!(!starts_editing("\n  x"));
        assert!(starts_editing(""));
        assert!(starts_editing(" \n\t "));
    }
}
