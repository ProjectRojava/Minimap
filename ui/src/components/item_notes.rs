//! Notes and findings on a task, project or objective: for record keeping. They are ordinary
//! notes (Markdown, searchable, in the exports) that mention the item, so they also show on the
//! Notes screen. The box adds one without leaving the pane; clicking a note opens it to edit.

use leptos::{prelude::*, task::spawn_local};
use minimap_types::{mention_token, CreateNote, NodeRef, NodeType, NoteFilter, Uuid};

use crate::{
    api,
    components::{
        detail_pane::Section,
        form::{BUTTON, BUTTON_PRIMARY, INPUT},
        people_panel::error_line,
    },
    labels::note_kind_tone,
    state::{finish, DataVersion, Selection, Toasts},
};

/// Notes shown before "Show all".
const SHOWN: usize = 6;
/// Longest title made from the first line of what was typed.
const TITLE_CHARS: usize = 80;

/// The title and body of a note typed into the box: the title is the first line (markup
/// stripped, shortened), the body is everything typed and ends by mentioning the item, which is
/// what ties the note to it. `None` when nothing was typed.
fn note_from_text(text: &str, label: &str, id: Uuid) -> Option<(String, String)> {
    let text = text.trim();
    let first = text
        .lines()
        .map(|l| l.trim().trim_start_matches(['#', '-', '*', '>']).trim())
        .find(|l| !l.is_empty())?;
    let title = if first.chars().count() > TITLE_CHARS {
        let cut: String = first.chars().take(TITLE_CHARS).collect();
        format!("{}…", cut.trim_end())
    } else {
        first.to_owned()
    };
    let body = format!("{text}\n\nAbout {}\n", mention_token(label, id));
    Some((title, body))
}

#[component]
pub fn ItemNotes(node: NodeRef) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let selection = expect_context::<Selection>();
    let id = node.id;
    let text = RwSignal::new(String::new());
    let all = RwSignal::new(false);

    let notes = LocalResource::new(move || {
        version.track();
        api::list_notes(NoteFilter {
            mentions_id: Some(id),
            ..Default::default()
        })
    });

    let add = move || {
        let typed = text.get_untracked();
        if typed.trim().is_empty() {
            return;
        }
        spawn_local(async move {
            let label = match api::get_node_summary(node).await {
                Ok(s) => s.label,
                Err(e) => return toasts.error(&e),
            };
            let Some((title, body)) = note_from_text(&typed, &label, id) else {
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
        });
    };
    let add_click = add;

    view! {
        <Section title="Notes and findings">
            <textarea class=INPUT rows="3"
                placeholder="Write a note or finding… (Ctrl+Enter to add)"
                aria-label="New note"
                prop:value=move || text.get()
                on:input=move |ev| text.set(event_target_value(&ev))
                on:keydown=move |ev| if ev.key() == "Enter" && (ev.ctrl_key() || ev.meta_key()) {
                    ev.prevent_default();
                    add();
                }></textarea>
            <div class="mt-1 flex items-center gap-2">
                <button class=BUTTON_PRIMARY
                    disabled=move || text.get().trim().is_empty()
                    on:click=move |_| add_click()>"Add note"</button>
                <span class="text-[11px] text-muted">"Saved as a note that mentions this item."</span>
            </div>
            {move || match notes.get() {
                None => view! { <p class="mt-2 text-muted">"Loading…"</p> }.into_any(),
                Some(Err(e)) => error_line(e),
                Some(Ok(rows)) if rows.is_empty() => view! {
                    <p class="mt-2 text-muted">"No notes yet."</p>
                }.into_any(),
                Some(Ok(rows)) => {
                    let total = rows.len();
                    let shown = if all.get() { total } else { SHOWN };
                    let items = rows.into_iter().take(shown).map(|n| {
                        let note = NodeRef::new(NodeType::Note, n.id);
                        let kind = note_kind_tone(n.kind);
                        let kind_label = n.kind.as_str().replace('_', " ");
                        let kind_chip = (n.kind != minimap_types::NoteKind::General)
                            .then(|| view! { <span class=kind.chip()>{kind_label}</span> });
                        view! {
                            <li>
                                <button class="flex w-full items-center gap-2 rounded px-1.5 py-0.5 text-left hover:bg-hover"
                                        on:click=move |_| selection.open(note)>
                                    <span class="w-20 shrink-0 tabular-nums text-muted">{n.note_date.to_string()}</span>
                                    <span class="min-w-0 flex-1 truncate">{n.title}</span>
                                    {kind_chip}
                                </button>
                            </li>
                        }
                    }).collect_view();
                    view! {
                        <ul class="mt-2 space-y-px">{items}</ul>
                        {(total > SHOWN).then(|| view! {
                            <button class=format!("{BUTTON} mt-2") on:click=move |_| all.update(|a| *a = !*a)>
                                {move || if all.get() { "Show fewer".to_owned() } else { format!("Show all {total}") }}
                            </button>
                        })}
                    }.into_any()
                }
            }}
        </Section>
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
}
