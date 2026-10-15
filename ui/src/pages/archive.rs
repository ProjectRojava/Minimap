//! The Archive: everything that has been archived, whatever it is, in one list. Search it, narrow
//! it to one kind of item, open an item to read it, and restore it. Finished tasks that came of
//! age are archived here by themselves (Settings, *Finished tasks*), so this is where to look for
//! one that has gone from the board.

use std::str::FromStr;

use leptos::{prelude::*, task::spawn_local};
use minimap_types::{ArchivedItem, Date, NodeType};

use crate::{
    api,
    components::{
        form::{BUTTON, COMPACT_INPUT},
        page::{column_head, EmptyState, PageHeader, FILTER_BAR},
        select::SelectField,
    },
    nav::type_label,
    state::{finish, DataVersion, Selection, Toasts},
};

const COLS: &str = "grid w-full items-center gap-3 grid-cols-[6rem_minmax(0,2fr)_minmax(0,1.4fr)_9rem_5.5rem]";

/// The items that match the text (in the name or detail) and the kind ("" = every kind).
pub fn matching(items: &[ArchivedItem], text: &str, kind: &str) -> Vec<ArchivedItem> {
    let text = text.trim().to_lowercase();
    let kind = NodeType::from_str(kind).ok();
    items
        .iter()
        .filter(|i| kind.is_none_or(|k| i.node.node.node_type == k))
        .filter(|i| {
            text.is_empty()
                || i.node.label.to_lowercase().contains(&text)
                || i.detail.to_lowercase().contains(&text)
        })
        .cloned()
        .collect()
}

/// "2027-03-02 · 4 days ago", "· today", "· yesterday".
pub fn archived_text(on: Date, today: Option<Date>) -> String {
    let ago = match today.map(|t| (t - on).whole_days()) {
        Some(0) => " · today".to_owned(),
        Some(1) => " · yesterday".to_owned(),
        Some(n) if n > 1 => format!(" · {n} days ago"),
        _ => String::new(),
    };
    format!("{on}{ago}")
}

/// The kinds to narrow to: (value, label).
fn kind_options() -> Vec<(String, String)> {
    std::iter::once((String::new(), "Everything".to_owned()))
        .chain(
            NodeType::ALL
                .iter()
                .map(|t| (t.as_str().to_owned(), type_label(*t).to_owned())),
        )
        .collect()
}

#[component]
pub fn Archive() -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let selection = expect_context::<Selection>();
    let items = LocalResource::new(move || {
        version.track();
        api::list_archived()
    });
    let text = RwSignal::new(String::new());
    let kind = RwSignal::new(String::new());
    let today = crate::components::task_board::today();

    view! {
        <div class="flex h-full flex-col">
            <PageHeader icon="archive" title="Archive"
                subtitle="Everything that was archived, by you or by the app. Restore anything." >
                {move || items.get().and_then(|r| r.ok()).map(|all| {
                    let shown = matching(&all, &text.get(), &kind.get()).len();
                    view! {
                        <span class="ml-auto text-[11px] text-muted tabular-nums">
                            {if shown == all.len() { format!("{shown} items") } else { format!("{shown} of {} items", all.len()) }}
                        </span>
                    }
                })}
            </PageHeader>
            <div class=FILTER_BAR>
                <input class=format!("{COMPACT_INPUT} w-56") type="search" placeholder="Search the archive"
                       prop:value=move || text.get()
                       on:input=move |ev| text.set(event_target_value(&ev)) />
                <SelectField compact=true options=kind_options() current=String::new()
                             on_change=move |v: String| kind.set(v) />
            </div>
            <div class=column_head(COLS)>
                <span>"Kind"</span><span>"Name"</span><span>"Detail"</span>
                <span>"Archived"</span><span></span>
            </div>
            <div class="flex-1 overflow-y-auto" role="table">
                {move || match items.get() {
                    None => view! { <p class="p-4 text-muted">"Loading…"</p> }.into_any(),
                    Some(Err(_)) => view! { <p class="p-4 text-muted">"Couldn't load the archive."</p> }.into_any(),
                    Some(Ok(all)) if all.is_empty() => view! {
                        <EmptyState icon="archive" title="Nothing archived"
                            hint="Archived tasks, projects, people and the rest are kept here. Finished tasks arrive on their own after a while." />
                    }.into_any(),
                    Some(Ok(all)) => {
                        let rows = matching(&all, &text.get(), &kind.get());
                        if rows.is_empty() {
                            return view! { <p class="p-4 text-muted">"Nothing matches."</p> }.into_any();
                        }
                        rows.into_iter().map(|item| {
                            let node = item.node.node;
                            let on = archived_text(item.archived_on, today);
                            view! {
                                <div class=format!("{COLS} group border-b border-line px-4 py-1.5 hover:bg-hover") role="row">
                                    <span class="truncate text-[11px] uppercase tracking-wide text-muted">{type_label(node.node_type)}</span>
                                    <button class="min-w-0 truncate text-left font-medium hover:text-accent hover:underline"
                                            title="Open it to read it" on:click=move |_| selection.open(node)>
                                        {item.node.label}
                                    </button>
                                    <span class="truncate text-muted">{item.detail}</span>
                                    <span class="tabular-nums text-muted">{on}</span>
                                    <button class=BUTTON title="Put it back, with the links archived along with it"
                                            on:click=move |_| {
                                                spawn_local(async move {
                                                    if finish(api::restore_archived(node).await, toasts, version).is_some() {
                                                        toasts.info("Restored.");
                                                    }
                                                });
                                            }>
                                        "Restore"
                                    </button>
                                </div>
                            }
                        }).collect_view().into_any()
                    }
                }}
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{NodeRef, NodeSummary, Uuid};
    use time::macros::date;

    fn item(kind: NodeType, label: &str, detail: &str) -> ArchivedItem {
        ArchivedItem {
            node: NodeSummary {
                node: NodeRef::new(kind, Uuid::nil()),
                label: label.into(),
                archived: true,
            },
            archived_on: date!(2027 - 03 - 02),
            archived_at: "2027-03-02T10:00:00.000Z".into(),
            detail: detail.into(),
        }
    }

    #[test]
    fn the_list_narrows_by_text_and_by_kind() {
        let all = [
            item(NodeType::Task, "Write the plan", "done · Launch"),
            item(NodeType::Person, "Priya", ""),
            item(NodeType::Task, "Book room", "done · Offsite"),
        ];
        assert_eq!(matching(&all, "", "").len(), 3);
        assert_eq!(matching(&all, "  PLAN ", "").len(), 1);
        // The project in the detail is searched too.
        assert_eq!(matching(&all, "offsite", "")[0].node.label, "Book room");
        assert_eq!(matching(&all, "", "task").len(), 2);
        assert_eq!(matching(&all, "priya", "task").len(), 0);
        assert_eq!(matching(&all, "", "person")[0].node.label, "Priya");
        // A kind it doesn't know narrows nothing.
        assert_eq!(matching(&all, "", "nonsense").len(), 3);
    }

    #[test]
    fn the_day_is_said_in_words_when_the_clock_is_known() {
        let on = date!(2027 - 03 - 02);
        assert_eq!(archived_text(on, Some(on)), "2027-03-02 · today");
        assert_eq!(archived_text(on, Some(date!(2027 - 03 - 03))), "2027-03-02 · yesterday");
        assert_eq!(archived_text(on, Some(date!(2027 - 03 - 06))), "2027-03-02 · 4 days ago");
        assert_eq!(archived_text(on, None), "2027-03-02");
    }
}
