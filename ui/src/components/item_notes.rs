//! Notes and findings on a task, project or objective: a thread of comments, like the ones under a
//! GitHub issue. They are ordinary notes (Markdown, searchable, in the exports) that mention the
//! item, so they also show on the Notes screen. Each one reads as formatted text with a pencil to
//! edit it in place and a ⋯ menu (open in the pane, archive); the box to write the next one is at
//! the bottom.

use leptos::{prelude::*, task::spawn_local};
use minimap_types::{
    mention_token, CreateNote, NodeRef, NodeType, Note, NoteFilter, NoteKind, UpdateNote, Uuid,
};

use crate::{
    api,
    components::{
        detail_pane::Section,
        form::{BUTTON, BUTTON_PRIMARY},
        markdown_box::{MarkdownBox, MarkdownView},
        page::Icon,
        people_panel::error_line,
    },
    labels::note_kind_tone,
    state::{finish, DataVersion, Selection, Toasts},
};

/// Notes shown before "Show earlier notes": the newest ones, so the box is never far away.
const SHOWN: usize = 10;
/// Longest title made from the first line of what was typed.
const TITLE_CHARS: usize = 80;

/// The title of a note: its first line with markup stripped and shortened. `None` when there is
/// nothing to make one from.
fn title_of(text: &str) -> Option<String> {
    let first = text
        .lines()
        .map(|l| l.trim().trim_start_matches(['#', '-', '*', '>']).trim())
        .find(|l| !l.is_empty())?;
    Some(if first.chars().count() > TITLE_CHARS {
        let cut: String = first.chars().take(TITLE_CHARS).collect();
        format!("{}…", cut.trim_end())
    } else {
        first.to_owned()
    })
}

/// The line that ties a note to its item (it makes the `mentions` link).
fn footer_line(label: &str, id: Uuid) -> String {
    format!("About {}", mention_token(label, id))
}

/// A note body split into what was typed and the closing "About @[item]" line that ties it to
/// `item` (kept out of the card and of the editor, and put back on save).
fn split_footer(body: &str, item: Uuid) -> (String, Option<String>) {
    let end = body.trim_end();
    if let Some(at) = end.rfind("\n\nAbout @[") {
        let line = &end[at + 2..];
        if !line.contains('\n') && line.ends_with(&format!("](node:{item})")) {
            return (end[..at].to_owned(), Some(line.to_owned()));
        }
    }
    (body.to_owned(), None)
}

/// The body to store for `text` and the closing line (if the note had one).
fn join_footer(text: &str, footer: Option<&str>) -> String {
    match footer {
        Some(line) => format!("{}\n\n{line}\n", text.trim()),
        None => text.to_owned(),
    }
}

/// The title and body of a note typed into the box: the title is the first line, the body is
/// everything typed and ends by mentioning the item, which is what ties the note to it. `None`
/// when nothing was typed.
fn note_from_text(text: &str, label: &str, id: Uuid) -> Option<(String, String)> {
    let text = text.trim();
    let title = title_of(text)?;
    Some((title, join_footer(text, Some(&footer_line(label, id)))))
}

#[component]
pub fn ItemNotes(node: NodeRef) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let id = node.id;
    let text = RwSignal::new(String::new());
    let all = RwSignal::new(false);
    let busy = RwSignal::new(false);

    let notes = LocalResource::new(move || {
        version.track();
        api::list_notes(NoteFilter {
            mentions_id: Some(id),
            ..Default::default()
        })
    });
    // Newest last, like a thread; the ids only, each card loads its own text.
    let ids = Memo::new(move |_| -> Option<Vec<Uuid>> {
        match notes.get() {
            Some(Ok(rows)) => {
                let total = rows.len();
                let hidden = if all.get() {
                    0
                } else {
                    total.saturating_sub(SHOWN)
                };
                Some(rows.into_iter().rev().skip(hidden).map(|n| n.id).collect())
            }
            _ => None,
        }
    });

    let add = move || {
        let typed = text.get_untracked();
        if typed.trim().is_empty() || busy.get_untracked() {
            return;
        }
        busy.set(true);
        spawn_local(async move {
            let label = match api::get_node_summary(node).await {
                Ok(s) => s.label,
                Err(e) => {
                    busy.set(false);
                    return toasts.error(&e);
                }
            };
            let Some((title, body)) = note_from_text(&typed, &label, id) else {
                busy.set(false);
                return;
            };
            let input = CreateNote {
                title,
                body,
                note_date: None,
                kind: None,
                recurrence: None,
            };
            if finish(api::create_note(input).await, toasts, version).is_some() {
                text.set(String::new());
            }
            busy.set(false);
        });
    };

    view! {
        <Section title="Notes and findings"
            meta=move || view! {
                {move || notes.get().and_then(|n| n.ok()).filter(|n| !n.is_empty())
                    .map(|n| format!("· {}", n.len()))}
            }>
            {move || match notes.get() {
                None => view! { <p class="mb-2 text-muted">"Loading…"</p> }.into_any(),
                Some(Err(e)) => error_line(e),
                Some(Ok(rows)) if rows.is_empty() => view! {
                    <p class="mb-2 text-muted">"No notes yet."</p>
                }.into_any(),
                Some(Ok(rows)) => {
                    let total = rows.len();
                    view! {
                        {(total > SHOWN && !all.get()).then(|| view! {
                            <button class=format!("{BUTTON} mb-2") on:click=move |_| all.set(true)>
                                {format!("Show {} earlier {}", total - SHOWN, if total - SHOWN == 1 { "note" } else { "notes" })}
                            </button>
                        })}
                        <ul class="mb-3 space-y-2">
                            <For each=move || ids.get().unwrap_or_default()
                                 key=|note| *note
                                 children=move |note| view! { <NoteCard id=note item=node /> } />
                        </ul>
                    }.into_any()
                }
            }}
            <MarkdownBox text=text attach_to=node rows=4
                placeholder="Write a note or finding. Markdown works; @ mentions a person, project or task."
                on_submit=Callback::new(move |_| add()) />
            <div class="mt-1.5 flex items-center gap-2">
                <button class=BUTTON_PRIMARY
                    disabled=move || busy.get() || text.with(|t| t.trim().is_empty())
                    on:click=move |_| add()>"Add note"</button>
                <span class="text-[11px] text-muted">"Ctrl/Cmd+Enter adds. Saved as a note that mentions this item."</span>
            </div>
        </Section>
    }
}

/// One note of the thread. It loads its own text (the list has none), so a card that is being
/// edited is not disturbed when the list changes around it.
#[component]
fn NoteCard(id: Uuid, item: NodeRef) -> impl IntoView {
    let note = LocalResource::new(move || api::get_note(id));
    view! {
        {move || match note.get() {
            None => view! { <li class="rounded-sm border border-line px-2 py-1.5 text-muted">"Loading…"</li> }.into_any(),
            Some(Err(e)) => view! { <li class="px-2 py-1.5">{error_line(e)}</li> }.into_any(),
            Some(Ok(n)) => view! { <CardBody note=n item=item /> }.into_any(),
        }}
    }
}

#[component]
fn CardBody(note: Note, item: NodeRef) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let selection = expect_context::<Selection>();
    let id = note.id;
    let (text, footer) = split_footer(&note.body, item.id);
    // What the card shows, and the title it was made with, as they stand after each save.
    let shown = RwSignal::new(text);
    let title = RwSignal::new(note.title.clone());
    let editing = RwSignal::new(false);
    let draft = RwSignal::new(String::new());
    let menu = RwSignal::new(false);
    let busy = RwSignal::new(false);

    let start = move || {
        menu.set(false);
        draft.set(shown.get_untracked());
        editing.set(true);
    };
    let cancel = move || editing.set(false);
    let footer = StoredValue::new(footer);
    let save = move || {
        let new_text = draft.get_untracked();
        if busy.get_untracked() || new_text.trim().is_empty() {
            return;
        }
        if new_text == shown.get_untracked() {
            return cancel();
        }
        // The title follows the first line unless it was changed by hand.
        let derived_before = title_of(&shown.get_untracked());
        let retitle = derived_before.as_deref() == Some(title.get_untracked().as_str());
        let new_title = if retitle { title_of(&new_text) } else { None };
        let patch = UpdateNote {
            body: Some(join_footer(&new_text, footer.get_value().as_deref())),
            title: new_title.clone(),
            ..Default::default()
        };
        busy.set(true);
        spawn_local(async move {
            let result = api::update_note(id, patch).await;
            busy.set(false);
            if finish(result, toasts, version).is_some() {
                shown.set(new_text.trim().to_owned());
                if let Some(t) = new_title {
                    title.set(t);
                }
                editing.set(false);
            }
        });
    };
    let escape = move || {
        if draft.get_untracked() == shown.get_untracked() {
            cancel();
        }
    };
    let archive = move || {
        menu.set(false);
        spawn_local(async move {
            if finish(api::archive_note(id).await, toasts, version).is_some() {
                toasts.info("Archived the note. Press Ctrl/Cmd+Z to undo.".to_owned());
            }
        });
    };
    let kind_chip = (note.kind != NoteKind::General).then(|| {
        view! { <span class=note_kind_tone(note.kind).chip()>{note.kind.as_str().replace('_', " ")}</span> }
    });

    view! {
        <li class="rounded-sm border border-line bg-canvas">
            <div class="flex items-center gap-2 border-b border-line bg-panel px-2 py-0.5 text-[11px] text-muted">
                <span class="tabular-nums">{note.note_date.to_string()}</span>
                {kind_chip}
                <span class="ml-auto flex items-center gap-0.5">
                    <Show when=move || !editing.get()>
                        <button type="button" class="flex h-5 w-5 items-center justify-center rounded-sm hover:bg-hover hover:text-fg"
                            title="Edit this note" aria-label="Edit this note" on:click=move |_| start()>
                            <Icon name="edit" size="h-3 w-3" />
                        </button>
                    </Show>
                    <button type="button" class="flex h-5 w-5 items-center justify-center rounded-sm text-[14px] leading-none hover:bg-hover hover:text-fg"
                        title="More" aria-label="More" aria-expanded=move || menu.get().to_string()
                        on:click=move |_| menu.update(|m| *m = !*m)>"⋯"</button>
                </span>
            </div>
            <Show when=move || menu.get()>
                <div class="flex items-center gap-2 border-b border-line px-2 py-1">
                    <button class=BUTTON on:click=move |_| { menu.set(false); selection.open(NodeRef::new(NodeType::Note, id)); }>
                        "Open in the pane"
                    </button>
                    <button class=format!("{BUTTON} text-danger") on:click=move |_| archive()>"Archive"</button>
                </div>
            </Show>
            <div class="px-2 py-1.5">
                {move || if editing.get() {
                    view! {
                        <MarkdownBox text=draft attach_to=NodeRef::new(NodeType::Note, id) rows=6 autofocus=true
                            on_submit=Callback::new(move |_| save())
                            on_cancel=Callback::new(move |_| escape()) />
                        <div class="mt-1.5 flex items-center gap-2">
                            <button class=BUTTON_PRIMARY
                                disabled=move || busy.get() || draft.with(|d| d.trim().is_empty())
                                on:click=move |_| save()>"Save"</button>
                            <button class=BUTTON on:click=move |_| cancel()>"Cancel"</button>
                            <span class="text-[11px] text-muted">"Ctrl/Cmd+Enter saves"</span>
                        </div>
                    }.into_any()
                } else {
                    view! {
                        <div on:dblclick=move |_| start()>
                            <MarkdownView text=shown empty="Nothing written." />
                        </div>
                    }.into_any()
                }}
            </div>
        </li>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id() -> Uuid {
        Uuid::nil()
    }

    #[test]
    fn the_first_line_is_the_title_and_the_note_mentions_the_item() {
        let (title, body) = note_from_text(
            "  Vendor said 2 weeks\nand wants a PO",
            "Gateway [v2]",
            id(),
        )
        .unwrap();
        assert_eq!(title, "Vendor said 2 weeks");
        assert!(
            body.starts_with("Vendor said 2 weeks\nand wants a PO\n\nAbout @[Gateway v2](node:")
        );
        assert!(body.contains(&id().to_string()));
    }

    #[test]
    fn markup_is_stripped_from_the_title_and_long_ones_are_cut() {
        assert_eq!(
            note_from_text("## Finding: slow query", "x", id())
                .unwrap()
                .0,
            "Finding: slow query"
        );
        assert_eq!(
            note_from_text("\n\n- first\n- second", "x", id())
                .unwrap()
                .0,
            "first"
        );
        let long = "w ".repeat(100);
        let title = note_from_text(&long, "x", id()).unwrap().0;
        assert!(title.ends_with('…') && title.chars().count() <= TITLE_CHARS + 1);
    }

    #[test]
    fn nothing_typed_makes_no_note() {
        assert!(note_from_text("  \n ", "x", id()).is_none());
        assert!(note_from_text("###", "x", id()).is_none());
    }

    #[test]
    fn the_closing_line_is_kept_out_of_the_card_and_put_back_on_save() {
        let item = Uuid::from_u128(7);
        let (_, body) = note_from_text("Vendor said 2 weeks", "Gateway", item).unwrap();
        let (text, footer) = split_footer(&body, item);
        assert_eq!(text, "Vendor said 2 weeks");
        let footer = footer.unwrap();
        assert!(footer.starts_with("About @[Gateway](node:") && footer.ends_with(')'));
        // Editing the text and saving gives the same shape again.
        assert_eq!(
            join_footer("Vendor said 3 weeks\n", Some(&footer)),
            format!("Vendor said 3 weeks\n\n{footer}\n")
        );
        assert_eq!(join_footer(&text, Some(&footer)), body);
    }

    #[test]
    fn a_note_without_the_closing_line_for_this_item_is_shown_whole() {
        let item = Uuid::from_u128(7);
        let other = Uuid::from_u128(8);
        // Written on the Notes screen, or about another item: nothing is hidden.
        let body = "A 1:1 note\n\nAbout @[Other](node:00000000-0000-0000-0000-000000000008)\n";
        assert_eq!(split_footer(body, item), (body.to_owned(), None));
        let (text, footer) = split_footer(body, other);
        assert_eq!(text, "A 1:1 note");
        assert!(footer.is_some());
        // The words "About @[" in the middle of a note are not a closing line.
        let tricky = format!("x\n\nAbout @[A](node:{item}) and more\nmore\n");
        assert_eq!(split_footer(&tricky, item).1, None);
        assert_eq!(join_footer("plain", None), "plain");
    }

    #[test]
    fn the_title_follows_the_first_line() {
        assert_eq!(
            title_of("  \n## Slow query\nmore").as_deref(),
            Some("Slow query")
        );
        assert_eq!(title_of("- a\n- b").as_deref(), Some("a"));
        assert_eq!(title_of(" \n"), None);
    }
}
