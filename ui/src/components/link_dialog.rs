//! The "link a task" dialog: make a new task joined to this one, or join an existing task to it,
//! saying how they relate. Opened from a Kanban card's menu or a task's panel; a new task opens
//! in the detail pane when it is made.
//!
//! The dialog is centred with `inset-x-0 mx-auto`, not a transform: a transformed element becomes
//! the reference for `position: fixed` inside it, which would put the dropdown lists of its
//! `SelectField`s (placed in viewport pixels) in the wrong place.

use leptos::{ev, html, prelude::*, task::spawn_local};
use minimap_types::{
    EdgeType, LinkRelation, NewEdge, NodeRef, NodeType, TaskFilter, TaskRow, Uuid,
};

use crate::{
    api,
    components::{
        form::{SelectField, BUTTON, BUTTON_ON, BUTTON_PRIMARY, INPUT},
        task_type::{remember_new_type, use_task_types, CHOOSE_TYPE},
    },
    state::{finish, DataVersion, LinkDialog, LinkRequest, Selection, Toasts},
};

/// The ways two tasks can be joined, in the words the dialog uses: what the *other* task is, and
/// what that means. Two families that never mix: the order of the work (`blocks`, `related`) and
/// what is part of what (`subtask_of`), which moves no date and holds nothing up.
pub const RELATIONS: [(LinkRelation, &str, &str); 5] = [
    (
        LinkRelation::Blocks,
        "Blocks this",
        "It must be done first. This task waits for it",
    ),
    (
        LinkRelation::BlockedBy,
        "Blocked by this",
        "It waits until this task is done",
    ),
    (
        LinkRelation::RelatesTo,
        "Related",
        "Connected, with no order between them",
    ),
    (LinkRelation::Parent, "Parent", "This task is part of it"),
    (LinkRelation::Subtask, "Sub-task", "It is part of this task"),
];

/// The two rows of choices: a heading and the relations under it.
pub const RELATION_GROUPS: [(&str, &[LinkRelation]); 2] = [
    (
        "Order of the work",
        &[
            LinkRelation::Blocks,
            LinkRelation::BlockedBy,
            LinkRelation::RelatesTo,
        ],
    ),
    (
        "What is part of what",
        &[LinkRelation::Parent, LinkRelation::Subtask],
    ),
];

/// The words for a relation: its name and what it means.
pub fn relation_words(relation: LinkRelation) -> (&'static str, &'static str) {
    RELATIONS
        .iter()
        .find(|r| r.0 == relation)
        .map(|r| (r.1, r.2))
        .unwrap_or(("", ""))
}

/// A task is part of one task, so a parent is chosen alone.
pub fn picks_one(relation: LinkRelation) -> bool {
    relation == LinkRelation::Parent
}

/// One sentence that says what the choice does, naming this task: shown under the choices so it
/// is never unclear which task waits for which.
pub fn relation_summary(relation: LinkRelation, task: &str) -> String {
    let task = if task.trim().is_empty() {
        "this task"
    } else {
        task.trim()
    };
    match relation {
        LinkRelation::Blocks => format!("“{task}” waits until the other task is done."),
        LinkRelation::BlockedBy => format!("The other task waits until “{task}” is done."),
        LinkRelation::RelatesTo => "Neither task waits for the other.".to_owned(),
        LinkRelation::Parent => {
            format!("“{task}” is part of the other task. Nothing waits and no date moves.")
        }
        LinkRelation::Subtask => {
            format!("The other task is part of “{task}”. Nothing waits and no date moves.")
        }
    }
}

/// The link to add between `source` and an `other` task for `relation`: its type and the ends,
/// in the order they are stored (the task that blocks is `from`; the sub-task is `from`).
pub fn relation_edge(relation: LinkRelation, source: Uuid, other: Uuid) -> NewEdge {
    let (edge_type, from, to) = match relation {
        LinkRelation::Blocks => (EdgeType::Blocks, other, source),
        LinkRelation::BlockedBy => (EdgeType::Blocks, source, other),
        LinkRelation::RelatesTo => (EdgeType::RelatesTo, source, other),
        LinkRelation::Parent => (EdgeType::SubtaskOf, source, other),
        LinkRelation::Subtask => (EdgeType::SubtaskOf, other, source),
    };
    NewEdge {
        edge_type,
        from: NodeRef::new(NodeType::Task, from),
        to: NodeRef::new(NodeType::Task, to),
        attrs: serde_json::json!({}),
    }
}

/// A task the "Existing task" tab can offer: its title and the project it belongs to.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub id: Uuid,
    pub title: String,
    pub project: Option<(Uuid, String)>,
}

/// The candidates for a list of task rows.
pub fn candidates(rows: &[TaskRow]) -> Vec<Candidate> {
    rows.iter()
        .map(|r| Candidate {
            id: r.task.id,
            title: r.task.title.clone(),
            project: r.project.as_ref().map(|p| (p.node.id, p.label.clone())),
        })
        .collect()
}

/// What the "Existing task" tab lists: the tasks of this task's project first, then the rest.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PickList {
    /// The name of this task's project, when it has one.
    pub project: Option<String>,
    /// Tasks of that project, in title order.
    pub same_project: Vec<Candidate>,
    /// Every other task, in title order.
    pub others: Vec<Candidate>,
}

impl PickList {
    pub fn is_empty(&self) -> bool {
        self.same_project.is_empty() && self.others.is_empty()
    }
}

/// The tasks the "Existing task" tab offers: every task but this one and the ones already linked
/// to it, whose title has every word typed in the search box. The ones in the same project as
/// `source` come first, so they are the easiest to reach; each group is in title order.
pub fn pick_list(all: &[Candidate], source: Uuid, linked: &[Uuid], query: &str) -> PickList {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let home = all
        .iter()
        .find(|c| c.id == source)
        .and_then(|c| c.project.clone());
    let mut list = PickList {
        project: home.as_ref().map(|(_, name)| name.clone()),
        ..PickList::default()
    };
    for c in all
        .iter()
        .filter(|c| c.id != source && !linked.contains(&c.id))
        .filter(|c| {
            let title = c.title.to_lowercase();
            words.iter().all(|w| title.contains(w))
        })
    {
        let same = home
            .as_ref()
            .is_some_and(|(id, _)| c.project.as_ref().is_some_and(|(p, _)| p == id));
        if same {
            list.same_project.push(c.clone());
        } else {
            list.others.push(c.clone());
        }
    }
    list.same_project.sort_by_key(|c| c.title.to_lowercase());
    list.others.sort_by_key(|c| c.title.to_lowercase());
    list
}

/// `picked` with `id` added, or removed when it is already there.
pub fn toggled(picked: &[Uuid], id: Uuid) -> Vec<Uuid> {
    if picked.contains(&id) {
        picked.iter().copied().filter(|p| *p != id).collect()
    } else {
        let mut next = picked.to_vec();
        next.push(id);
        next
    }
}

/// `picked` after ticking `id`: like [`toggled`], or, when only one can be chosen, just `id`
/// (ticking it again clears it).
pub fn pick(picked: &[Uuid], id: Uuid, one_only: bool) -> Vec<Uuid> {
    if !one_only {
        return toggled(picked, id);
    }
    if picked == [id] {
        Vec::new()
    } else {
        vec![id]
    }
}

/// The button of the "Existing task" tab.
pub fn link_button_text(chosen: usize) -> String {
    match chosen {
        0 | 1 => "Link".to_owned(),
        n => format!("Link {n} tasks"),
    }
}

/// Mount once, inside the router.
#[component]
pub fn LinkDialogHost() -> impl IntoView {
    let dialog = expect_context::<LinkDialog>();
    view! {
        {move || dialog.0.get().map(|request| view! { <Dialog request=request /> })}
    }
}

#[component]
fn Dialog(request: LinkRequest) -> impl IntoView {
    let dialog = expect_context::<LinkDialog>();
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let selection = expect_context::<Selection>();
    let source = request.task;
    let label = request.label.clone();
    let summary_label = request.label.clone();
    let existing = RwSignal::new(request.existing);
    let relation = RwSignal::new(request.relation);
    let title = RwSignal::new(String::new());
    let picked = RwSignal::new(Vec::<Uuid>::new());
    let query = RwSignal::new(String::new());
    let types = use_task_types();
    // A task has a type: the one used last, else asked.
    let task_type = RwSignal::new(types.starting_type(""));
    let busy = RwSignal::new(false);
    let input = leptos::prelude::NodeRef::<html::Input>::new();

    // The tasks that can be picked (all but this one).
    let tasks = LocalResource::new(move || {
        api::list_tasks(TaskFilter {
            include_closed: true,
            ..TaskFilter::default()
        })
    });
    // Tasks already linked to this one are not offered again.
    let current = LocalResource::new(move || api::list_edges_for(source));
    Effect::new(move |_| {
        if !existing.get() {
            if let Some(el) = input.get() {
                let _ = el.focus();
            }
        }
    });

    let submit = move || {
        if busy.get_untracked() {
            return;
        }
        let relation = relation.get_untracked();
        if existing.get_untracked() {
            let chosen = picked.get_untracked();
            if chosen.is_empty() {
                return;
            }
            let edges: Vec<NewEdge> = chosen
                .iter()
                .map(|other| relation_edge(relation, source, *other))
                .collect();
            let count = edges.len();
            busy.set(true);
            spawn_local(async move {
                // All of them or none, as one step for undo.
                let result = api::add_edges(edges).await;
                busy.set(false);
                if finish(result, toasts, version).is_some() {
                    dialog.close();
                    toasts.info(if count == 1 {
                        "Linked".to_owned()
                    } else {
                        format!("Linked {count} tasks")
                    });
                }
            });
        } else {
            let text = title.get_untracked();
            if text.trim().is_empty() {
                return;
            }
            if task_type.get_untracked().is_empty() {
                toasts.error(&minimap_types::AppError {
                    code: "invalid".into(),
                    message: CHOOSE_TYPE.into(),
                });
                return;
            }
            busy.set(true);
            spawn_local(async move {
                let kind = Some(task_type.get_untracked()).filter(|t| !t.is_empty());
                let result = api::create_linked_task(source, relation, text, kind.clone()).await;
                busy.set(false);
                if let Some(task) = finish(result, toasts, version) {
                    if let Some(kind) = kind {
                        remember_new_type(&kind);
                    }
                    dialog.close();
                    selection.open(NodeRef::new(NodeType::Task, task.id));
                }
            });
        }
    };
    let on_keydown = move |e: ev::KeyboardEvent| match e.key().as_str() {
        "Escape" => {
            e.prevent_default();
            e.stop_propagation();
            dialog.close();
        }
        "Enter" if !existing.get_untracked() => {
            e.prevent_default();
            submit();
        }
        _ => {}
    };

    let relation_rows = RELATION_GROUPS
        .iter()
        .map(|(heading, members)| {
            let buttons = members
                .iter()
                .map(|value| {
                    let value = *value;
                    let (name, hint) = relation_words(value);
                    view! {
                        <button type="button"
                            class=move || format!(
                                "flex flex-col items-start rounded-sm border border-line px-2 py-1 text-left \
                                 hover:border-accent/50 {}",
                                if relation.get() == value { BUTTON_ON } else { "" })
                            aria-pressed=move || (relation.get() == value).to_string()
                            on:click=move |_| {
                                relation.set(value);
                                // A task is part of one task: keep a single choice.
                                if picks_one(value) {
                                    picked.update(|p| p.truncate(1));
                                }
                            }>
                            <span class="text-[12px] font-medium">{name}</span>
                            <span class="text-[11px] text-muted">{hint}</span>
                        </button>
                    }
                })
                .collect_view();
            let columns = if members.len() == 3 { "grid-cols-3" } else { "grid-cols-2" };
            view! {
                <p class="mb-0.5 text-[11px] text-faint">{*heading}</p>
                <div class=format!("mb-1.5 grid gap-2 {columns}")>{buttons}</div>
            }
        })
        .collect_view();
    let tab = move |is_existing: bool, name: &'static str| {
        view! {
            <button type="button"
                class=move || format!("{BUTTON} {}", if existing.get() == is_existing { BUTTON_ON } else { "" })
                on:click=move |_| existing.set(is_existing)>{name}</button>
        }
    };

    view! {
        <div class="fixed inset-0 z-[80] bg-scrim" on:mousedown=move |_| dialog.close()></div>
        <div role="dialog" aria-label="Link a task"
             class="fixed inset-x-0 mx-auto top-[14vh] z-[81] w-[34rem] max-w-[94vw] \
                    rounded-sm border border-line bg-panel p-4 text-[13px]"
             on:keydown=on_keydown>
            <h2 class="mb-3 text-[14px] font-semibold">
                "Link a task to " <span class="text-accent">{label}</span>
            </h2>
            <div class="mb-3 flex gap-2">{tab(false, "New task")}{tab(true, "Existing task")}</div>
            <p class="mb-1 text-[11px] text-muted">"The other task…"</p>
            {relation_rows}
            <p class="mb-3 text-[11px] text-muted">{move || relation_summary(relation.get(), &summary_label)}</p>
            {move || if existing.get() {
                view! { <TaskPicker source=source tasks=tasks current=current picked=picked query=query relation=relation /> }.into_any()
            } else {
                view! {
                    <label class="block">
                        <span class="mb-0.5 block text-[11px] text-muted">"Title of the new task"</span>
                        <input node_ref=input class=INPUT type="text" autocomplete="off"
                            placeholder="What needs doing?"
                            prop:value=move || title.get()
                            on:input=move |ev| title.set(event_target_value(&ev)) />
                    </label>
                    <div class="mt-2">
                        {move || view! {
                            <SelectField label="Type (required)" options=types.new_task_options()
                                current=task_type.get_untracked()
                                on_change=move |v: String| task_type.set(v) />
                        }}
                    </div>
                }.into_any()
            }}
            <div class="mt-4 flex items-center justify-end gap-2">
                <button type="button" class=BUTTON on:click=move |_| dialog.close()>"Cancel"</button>
                <button type="button" class=BUTTON_PRIMARY
                    disabled=move || busy.get() || if existing.get() { picked.get().is_empty() } else { title.get().trim().is_empty() || task_type.get().is_empty() }
                    on:click=move |_| submit()>
                    {move || if existing.get() { link_button_text(picked.get().len()) } else { "Create and open".to_owned() }}
                </button>
            </div>
        </div>
    }
}

/// The "Existing task" tab: a search box over a list of tasks with a tick box each, so several
/// can be chosen and linked at once.
#[component]
fn TaskPicker(
    source: Uuid,
    tasks: LocalResource<Result<Vec<TaskRow>, minimap_types::AppError>>,
    current: LocalResource<Result<Vec<minimap_types::EdgeLink>, minimap_types::AppError>>,
    picked: RwSignal<Vec<Uuid>>,
    query: RwSignal<String>,
    relation: RwSignal<LinkRelation>,
) -> impl IntoView {
    let rows = move || {
        let (Some(Ok(all)), Some(Ok(links))) = (tasks.get(), current.get()) else {
            return None;
        };
        let linked: Vec<Uuid> = links
            .iter()
            .filter(|l| l.other.node.node_type == NodeType::Task)
            .map(|l| l.other.node.id)
            .collect();
        Some(pick_list(&candidates(&all), source, &linked, &query.get()))
    };
    view! {
        <p class="mb-0.5 text-[11px] text-muted">"Tasks to link"</p>
        <input class=INPUT type="search" autocomplete="off" placeholder="Search tasks"
            prop:value=move || query.get()
            on:input=move |ev| query.set(event_target_value(&ev)) />
        <ul class="mt-1 max-h-56 overflow-y-auto rounded-sm border border-line" role="listbox"
            aria-multiselectable="true" aria-label="Tasks to link">
            {move || match rows() {
                None => view! { <li class="px-2 py-1.5 text-muted">"Loading…"</li> }.into_any(),
                Some(list) if list.is_empty() => view! {
                    <li class="px-2 py-1.5 text-muted">"No tasks to link."</li>
                }.into_any(),
                Some(list) => {
                    let heading = |text: String| view! {
                        <li role="presentation"
                            class="sticky top-0 z-10 truncate border-b border-line bg-panel px-2 py-0.5 \
                                   text-[11px] font-medium text-accent">{text}</li>
                    };
                    let row = move |c: Candidate, show_project: bool| view! {
                        <li role="option" aria-selected=move || picked.get().contains(&c.id).to_string()>
                            <label class="flex cursor-pointer items-center gap-2 px-2 py-1 hover:bg-hover">
                                <input type="checkbox"
                                    prop:checked=move || picked.get().contains(&c.id)
                                    on:change=move |_| picked.update(|p| *p = pick(p, c.id, picks_one(relation.get_untracked()))) />
                                <span class="min-w-0 flex-1 truncate">{c.title.clone()}</span>
                                {show_project.then(|| c.project.clone().map(|(_, name)| view! {
                                    <span class="max-w-[10rem] shrink-0 truncate text-[11px] text-faint">{name}</span>
                                }))}
                            </label>
                        </li>
                    };
                    let home = list.project.clone().filter(|_| !list.same_project.is_empty()).map(|name| {
                        view! {
                            {heading(format!("In {name} · {}", list.same_project.len()))}
                            {list.same_project.iter().cloned().map(|c| row(c, false)).collect_view()}
                        }
                    });
                    let others = (!list.others.is_empty()).then(|| {
                        let title = if home.is_some() {
                            format!("Other tasks · {}", list.others.len())
                        } else {
                            format!("Tasks · {}", list.others.len())
                        };
                        view! {
                            {heading(title)}
                            {list.others.iter().cloned().map(|c| row(c, true)).collect_view()}
                        }
                    });
                    view! { {home} {others} }.into_any()
                }
            }}
        </ul>
        <p class="mt-1 text-[11px] text-muted">
            {move || match picked.get().len() {
                0 if picks_one(relation.get()) => "Tick the task this one is part of.".to_owned(),
                0 => "Tick one or more tasks.".to_owned(),
                1 => "1 task chosen.".to_owned(),
                n => format!("{n} tasks chosen."),
            }}
        </p>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(n: u128, title: &str, project: Option<(u128, &str)>) -> Candidate {
        Candidate {
            id: Uuid::from_u128(n),
            title: title.into(),
            project: project.map(|(p, name)| (Uuid::from_u128(p), name.to_owned())),
        }
    }

    fn ids(list: &[Candidate]) -> Vec<u128> {
        list.iter().map(|c| c.id.as_u128()).collect()
    }

    #[test]
    fn the_list_leaves_out_this_task_and_the_ones_already_linked_and_follows_the_search() {
        let all = vec![
            cand(1, "Ship it", None),
            cand(2, "write docs", None),
            cand(3, "Fix the build", None),
            cand(4, "Docs review", None),
        ];
        let found = |q: &str| -> Vec<u128> {
            ids(&pick_list(&all, Uuid::from_u128(1), &[Uuid::from_u128(3)], q).others)
        };
        // Title order, ignoring case; no 1 (this task) and no 3 (already linked).
        assert_eq!(found(""), [4, 2]);
        assert_eq!(found("DOCS"), [4, 2]);
        assert_eq!(found("docs write"), [2]);
        assert!(found("zzz").is_empty());
    }

    #[test]
    fn tasks_of_the_same_project_come_first_and_are_kept_apart() {
        let all = vec![
            cand(1, "This one", Some((10, "Tender API"))),
            cand(2, "Zeta", Some((10, "Tender API"))),
            cand(3, "Alpha", Some((11, "Other"))),
            cand(4, "Beta", Some((10, "Tender API"))),
            cand(5, "Gamma", None),
        ];
        let list = pick_list(&all, Uuid::from_u128(1), &[], "");
        assert_eq!(list.project.as_deref(), Some("Tender API"));
        assert_eq!(ids(&list.same_project), [4, 2]);
        assert_eq!(ids(&list.others), [3, 5]);
        // The search narrows both groups.
        let list = pick_list(&all, Uuid::from_u128(1), &[], "a");
        assert_eq!(ids(&list.same_project), [4, 2]);
        assert_eq!(ids(&list.others), [3, 5]);
        let list = pick_list(&all, Uuid::from_u128(1), &[Uuid::from_u128(4)], "zeta");
        assert_eq!(ids(&list.same_project), [2]);
        assert!(list.others.is_empty());
    }

    #[test]
    fn a_task_without_a_project_has_no_same_project_group() {
        let all = vec![
            cand(1, "Loose", None),
            cand(2, "Also loose", None),
            cand(3, "In a project", Some((10, "P"))),
        ];
        let list = pick_list(&all, Uuid::from_u128(1), &[], "");
        assert_eq!(list.project, None);
        assert!(list.same_project.is_empty());
        assert_eq!(ids(&list.others), [2, 3]);
    }

    #[test]
    fn ticking_twice_unticks_and_the_order_is_kept() {
        let (a, b, c) = (Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3));
        let picked = toggled(&toggled(&toggled(&[], a), b), c);
        assert_eq!(picked, [a, b, c]);
        assert_eq!(toggled(&picked, b), [a, c]);
    }

    #[test]
    fn the_button_counts_what_is_chosen() {
        assert_eq!(link_button_text(0), "Link");
        assert_eq!(link_button_text(1), "Link");
        assert_eq!(link_button_text(3), "Link 3 tasks");
    }

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    #[test]
    fn the_task_that_comes_first_is_the_one_that_blocks() {
        let (source, other) = (id(1), id(2));
        // The other task blocks this one (this waits for it).
        let e = relation_edge(LinkRelation::Blocks, source, other);
        assert_eq!(
            (e.edge_type, e.from.id, e.to.id),
            (EdgeType::Blocks, other, source)
        );
        // The other task is blocked by this one: this task blocks it.
        let e = relation_edge(LinkRelation::BlockedBy, source, other);
        assert_eq!(
            (e.edge_type, e.from.id, e.to.id),
            (EdgeType::Blocks, source, other)
        );
        let e = relation_edge(LinkRelation::RelatesTo, source, other);
        assert_eq!(
            (e.edge_type, e.from.id, e.to.id),
            (EdgeType::RelatesTo, source, other)
        );
    }

    #[test]
    fn the_sub_task_is_the_end_the_link_starts_from() {
        let (source, other) = (id(1), id(2));
        // The other task is this one's parent: this one is the sub-task.
        let e = relation_edge(LinkRelation::Parent, source, other);
        assert_eq!(
            (e.edge_type, e.from.id, e.to.id),
            (EdgeType::SubtaskOf, source, other)
        );
        let e = relation_edge(LinkRelation::Subtask, source, other);
        assert_eq!(
            (e.edge_type, e.from.id, e.to.id),
            (EdgeType::SubtaskOf, other, source)
        );
    }

    #[test]
    fn the_dialog_is_not_moved_by_a_transform_so_its_dropdowns_land_under_their_buttons() {
        // A transformed ancestor re-bases `position: fixed` (which `SelectField` lists use).
        let needle = ["-trans", "late-"].concat();
        let source = include_str!("link_dialog.rs");
        assert!(!source.contains(&needle));
    }

    #[test]
    fn the_summary_names_the_task_that_waits_or_says_that_none_does() {
        assert_eq!(
            relation_summary(LinkRelation::BlockedBy, "Ship it"),
            "The other task waits until “Ship it” is done."
        );
        assert_eq!(
            relation_summary(LinkRelation::Blocks, "Ship it"),
            "“Ship it” waits until the other task is done."
        );
        assert_eq!(
            relation_summary(LinkRelation::RelatesTo, "Ship it"),
            "Neither task waits for the other."
        );
        assert!(relation_summary(LinkRelation::Blocks, " ").starts_with("“this task”"));
        // Part-of is organisation: the sentence says nothing waits.
        for r in [LinkRelation::Parent, LinkRelation::Subtask] {
            let text = relation_summary(r, "Ship it");
            assert!(
                text.contains("“Ship it”") && text.contains("Nothing waits"),
                "{text}"
            );
        }
    }

    #[test]
    fn blocking_and_part_of_are_named_apart() {
        let name = |r: LinkRelation| relation_words(r).0;
        assert_eq!(name(LinkRelation::Blocks), "Blocks this");
        assert_eq!(name(LinkRelation::BlockedBy), "Blocked by this");
        assert_eq!(name(LinkRelation::Parent), "Parent");
        assert_eq!(name(LinkRelation::Subtask), "Sub-task");
        // Parent and child are no longer words for blocking.
        assert!(RELATIONS
            .iter()
            .filter(|r| r.0 == LinkRelation::Blocks || r.0 == LinkRelation::BlockedBy)
            .all(|r| !r.1.contains("arent") && !r.1.contains("hild")));
    }

    #[test]
    fn every_relation_is_offered_once_in_one_row_with_words() {
        let mut seen: Vec<LinkRelation> = RELATIONS.iter().map(|r| r.0).collect();
        seen.dedup();
        assert_eq!(seen.len(), 5);
        assert!(RELATIONS
            .iter()
            .all(|(_, name, hint)| !name.is_empty() && !hint.is_empty()));
        let mut grouped: Vec<LinkRelation> = RELATION_GROUPS
            .iter()
            .flat_map(|(_, members)| members.iter().copied())
            .collect();
        assert_eq!(grouped.len(), 5);
        grouped.dedup();
        assert_eq!(grouped.len(), 5);
        assert!(grouped.iter().all(|r| seen.contains(r)));
    }

    #[test]
    fn a_parent_is_chosen_alone_but_sub_tasks_and_links_can_be_several() {
        let (a, b) = (id(1), id(2));
        assert!(picks_one(LinkRelation::Parent));
        assert!(!picks_one(LinkRelation::Subtask) && !picks_one(LinkRelation::Blocks));
        assert_eq!(pick(&[a], b, true), [b]);
        assert_eq!(pick(&[a], a, true), Vec::<Uuid>::new());
        assert_eq!(pick(&[a], b, false), [a, b]);
    }
}
