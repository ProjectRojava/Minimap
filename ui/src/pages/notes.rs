use std::str::FromStr;

use leptos::{prelude::*, task::spawn_local};
use minimap_types::{
    timefmt::parse_date, CreateNote, NodeRef, NodeType, NoteFilter, NoteKind, NoteRow, Uuid,
};

use crate::{
    api,
    components::{
        form::{DateField, SelectField, BUTTON, COMPACT_INPUT},
        node_row::NodeRow,
        note_panel::kind_label,
    },
    state::{finish, DataVersion, ListNav, Selection, Toasts},
};

const COLS: &str = "grid w-full items-center gap-3 grid-cols-[6.5rem_4.5rem_minmax(0,1fr)_10rem]";

/// Meeting notes and 1:1s, newest first. Mentions (`@Name`) link a note to people, projects and tasks.
#[component]
pub fn Notes() -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let list = expect_context::<ListNav>();
    let selection = expect_context::<Selection>();

    let text = RwSignal::new(String::new());
    let kind = RwSignal::new(String::new());
    let person = RwSignal::new(String::new());
    let from = RwSignal::new(String::new());
    let to = RwSignal::new(String::new());

    let rows = LocalResource::new(move || {
        version.track();
        let t = text.get();
        api::list_notes(NoteFilter {
            kind: NoteKind::from_str(&kind.get()).ok(),
            mentions_id: Uuid::parse_str(&person.get()).ok(),
            date_from: parse_date(&from.get()).ok(),
            date_to: parse_date(&to.get()).ok(),
            text: (!t.trim().is_empty()).then_some(t),
        })
    });
    let people = LocalResource::new(move || {
        version.track();
        api::list_people()
    });
    Effect::new(move |_| match rows.get() {
        Some(Ok(r)) => list.set_items(
            r.iter()
                .map(|n| NodeRef::new(NodeType::Note, n.id))
                .collect(),
        ),
        Some(Err(e)) => toasts.error(&e),
        None => {}
    });

    // A new note is created at once (so mentions and autosave have something to attach to) and
    // opened for writing.
    let create = move || {
        spawn_local(async move {
            let input = CreateNote {
                title: "Untitled note".into(),
                body: String::new(),
                note_date: None,
                kind: NoteKind::from_str(&kind.get_untracked()).ok(),
            };
            if let Some(n) = finish(api::create_note(input).await, toasts, version) {
                selection.open(NodeRef::new(NodeType::Note, n.id));
            }
        });
    };
    list.on_new(create);

    let kind_options: Vec<(String, String)> =
        std::iter::once((String::new(), "Any kind".to_owned()))
            .chain(
                NoteKind::ALL
                    .iter()
                    .map(|k| (k.as_str().to_owned(), kind_label(*k).to_owned())),
            )
            .collect();

    view! {
        <div class="flex flex-col h-full">
            <header class="flex items-center gap-3 px-4 h-10 shrink-0 border-b border-line">
                <h1 class="text-[13px] font-semibold">"Notes"</h1>
                <button class=BUTTON on:click=move |_| create()>"New note"</button>
                <span class="ml-auto text-[11px] text-muted">"n new · j/k move · Enter open"</span>
            </header>
            <div class="flex flex-wrap items-center gap-2 px-4 py-1.5 shrink-0 border-b border-line">
                <input class=format!("{COMPACT_INPUT} w-44") type="search" placeholder="Search notes"
                       prop:value=move || text.get() on:input=move |ev| text.set(event_target_value(&ev)) />
                <SelectField compact=true options=kind_options current=String::new() on_change=move |v: String| kind.set(v) />
                {move || {
                    let options: Vec<(String, String)> = std::iter::once((String::new(), "Mentioning anyone".to_owned()))
                        .chain(match people.get() {
                            Some(Ok(p)) => p.into_iter().map(|p| (p.person.id.to_string(), p.person.name)).collect(),
                            _ => Vec::new(),
                        })
                        .collect();
                    view! { <SelectField compact=true current=person.get_untracked() options=options
                                         on_change=move |v: String| person.set(v) /> }
                }}
                <label class="flex items-center gap-1 text-[11px] text-muted">"From"
                    <DateField compact=true current=from.get_untracked() on_commit=move |v: String| from.set(v) />
                    "to"
                    <DateField compact=true current=to.get_untracked() on_commit=move |v: String| to.set(v) />
                </label>
            </div>
            <div class="flex-1 overflow-y-auto" role="table">
                {move || match rows.get() {
                    None => view! { <p class="p-4 text-muted">"Loading…"</p> }.into_any(),
                    Some(Err(_)) => view! { <p class="p-4 text-muted">"Couldn't load notes."</p> }.into_any(),
                    Some(Ok(r)) if r.is_empty() => view! {
                        <p class="p-4 text-muted">"No notes match. Press n to write one."</p>
                    }.into_any(),
                    Some(Ok(r)) => r.into_iter().enumerate()
                        .map(|(i, row)| view! { <NoteRowView row=row index=i /> })
                        .collect_view().into_any(),
                }}
            </div>
        </div>
    }
}

#[component]
fn NoteRowView(row: NoteRow, index: usize) -> impl IntoView {
    let node = NodeRef::new(NodeType::Note, row.id);
    let mentions = row
        .mentions
        .iter()
        .map(|m| m.label.clone())
        .collect::<Vec<_>>()
        .join(", ");
    view! {
        <NodeRow node=node index=index>
            <div class=COLS>
                <span class="tabular-nums text-muted">{row.note_date.to_string()}</span>
                <span class="text-[11px] uppercase tracking-wide text-muted">{kind_label(row.kind)}</span>
                <span class="truncate">
                    <span class="font-medium">{row.title}</span>
                    <span class="ml-2 text-muted">{row.excerpt}</span>
                </span>
                <span class="truncate text-muted">{mentions}</span>
            </div>
        </NodeRow>
    }
}
