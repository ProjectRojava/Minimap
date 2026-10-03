use std::str::FromStr;

use leptos::{ev, prelude::*, task::spawn_local};
use minimap_types::{
    CreateProject, EdgeType, NewEdge, NodeRef, NodeType, ProjectFilter, ProjectGroup,
    ProjectLayout, ProjectRow, ProjectStatus, Uuid,
};

use crate::{
    api,
    components::{
        form::{SelectField, BUTTON, BUTTON_PRIMARY, INPUT},
        node_row::NodeRow,
    },
    labels::{priority_short, project_status_label},
    state::{finish, DataVersion, ListNav, Selection, Toasts},
};

const COLS: &str =
    "grid w-full items-center gap-3 grid-cols-[2rem_minmax(0,1fr)_8rem_5.5rem_6.5rem_3.5rem]";

/// "3/5", or empty when the project has no tasks.
fn progress(row: &ProjectRow) -> String {
    if row.task_count == 0 {
        String::new()
    } else {
        format!("{}/{}", row.done_task_count, row.task_count)
    }
}

fn node_of(row: &ProjectRow) -> NodeRef {
    NodeRef::new(NodeType::Project, row.project.id)
}

#[component]
pub fn Projects() -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let list = expect_context::<ListNav>();
    let selection = expect_context::<Selection>();

    let board = RwSignal::new(false);
    let status = RwSignal::new(String::new());
    let owner = RwSignal::new(String::new());
    let objective = RwSignal::new(String::new());

    let groups = LocalResource::new(move || {
        version.track();
        let filter = ProjectFilter {
            status: ProjectStatus::from_str(&status.get()).ok(),
            owner_person_id: Uuid::parse_str(&owner.get()).ok(),
            objective_id: Uuid::parse_str(&objective.get()).ok(),
        };
        let layout = if board.get() {
            ProjectLayout::Board
        } else {
            ProjectLayout::List
        };
        api::list_projects(filter, layout)
    });
    let people = LocalResource::new(move || {
        version.track();
        api::list_people()
    });
    let objectives = LocalResource::new(move || {
        version.track();
        api::list_node_summaries(NodeType::Objective)
    });

    // Keyboard rows follow the screen: top to bottom (list) or column by column (board).
    Effect::new(move |_| match groups.get() {
        Some(Ok(g)) => list.set_items(g.iter().flat_map(|g| g.rows.iter()).map(node_of).collect()),
        Some(Err(e)) => toasts.error(&e),
        None => {}
    });

    let adding = RwSignal::new(false);
    list.on_new(move || adding.set(true));
    let title = RwSignal::new(String::new());
    let new_owner = RwSignal::new(String::new());
    let new_objective = RwSignal::new(String::new());
    let submit = move || {
        let t = title.get_untracked();
        if t.trim().is_empty() {
            return;
        }
        let owner_id = Uuid::parse_str(&new_owner.get_untracked()).ok();
        let objective_id = Uuid::parse_str(&new_objective.get_untracked()).ok();
        spawn_local(async move {
            let input = CreateProject {
                title: t,
                slug: None,
                description: String::new(),
                owner_person_id: owner_id,
                start_date: None,
                target_date: None,
                status: None,
                priority: None,
            };
            let Some(p) = finish(api::create_project(input).await, toasts, version) else {
                return;
            };
            if let Some(o) = objective_id {
                let link = NewEdge {
                    edge_type: EdgeType::ContributesTo,
                    from: NodeRef::new(NodeType::Project, p.id),
                    to: NodeRef::new(NodeType::Objective, o),
                    attrs: serde_json::json!({ "weight": 1.0 }),
                };
                finish(api::add_edge(link).await, toasts, version);
            }
            title.set(String::new());
            selection.open(NodeRef::new(NodeType::Project, p.id));
        });
    };

    let toggle_class = move |on: bool| {
        format!(
            "{BUTTON} !rounded-none {}",
            if on { "bg-active" } else { "" }
        )
    };

    view! {
        <div class="flex flex-col h-full">
            <header class="flex items-center gap-3 px-4 h-10 shrink-0 border-b border-line">
                <h1 class="text-[13px] font-semibold">"Projects"</h1>
                <button class=BUTTON on:click=move |_| adding.update(|a| *a = !*a)>
                    {move || if adding.get() { "Cancel" } else { "New project" }}
                </button>
                <div class="ml-auto flex" role="group" aria-label="Layout">
                    <button class=move || toggle_class(!board.get()) aria-pressed=move || (!board.get()).to_string()
                            on:click=move |_| board.set(false)>"List"</button>
                    <button class=move || toggle_class(board.get()) aria-pressed=move || board.get().to_string()
                            on:click=move |_| board.set(true)>"Board"</button>
                </div>
            </header>
            <div class="flex items-center gap-2 px-4 py-1.5 shrink-0 border-b border-line">
                <span class="text-[11px] uppercase tracking-wide text-muted">"Filter"</span>
                {move || view! {
                    <SelectField compact=true current=status.get_untracked()
                        options=status_options() on_change=move |v: String| status.set(v) />
                }}
                {move || {
                    let options: Vec<(String, String)> = std::iter::once((String::new(), "Any owner".to_owned()))
                        .chain(match people.get() {
                            Some(Ok(p)) => p.into_iter().map(|p| (p.person.id.to_string(), p.person.name)).collect(),
                            _ => Vec::new(),
                        })
                        .collect();
                    view! { <SelectField compact=true current=owner.get_untracked() options=options
                                         on_change=move |v: String| owner.set(v) /> }
                }}
                {move || {
                    let options: Vec<(String, String)> = std::iter::once((String::new(), "Any objective".to_owned()))
                        .chain(match objectives.get() {
                            Some(Ok(o)) => o.into_iter().map(|o| (o.node.id.to_string(), o.label)).collect(),
                            _ => Vec::new(),
                        })
                        .collect();
                    view! { <SelectField compact=true current=objective.get_untracked() options=options
                                         on_change=move |v: String| objective.set(v) /> }
                }}
            </div>
            <Show when=move || adding.get()>
                <form class="flex items-end gap-2 px-4 py-2 border-b border-line bg-panel"
                      on:submit=move |ev| { ev.prevent_default(); submit(); }>
                    <input class=INPUT placeholder="Project title" autofocus prop:value=move || title.get()
                           on:input=move |ev| title.set(event_target_value(&ev)) />
                    {move || {
                        let options: Vec<(String, String)> = std::iter::once((String::new(), "No owner".to_owned()))
                            .chain(match people.get() {
                                Some(Ok(p)) => p.into_iter().map(|p| (p.person.id.to_string(), p.person.name)).collect(),
                                _ => Vec::new(),
                            })
                            .collect();
                        view! { <SelectField options=options current=new_owner.get_untracked()
                                             on_change=move |v: String| new_owner.set(v) /> }
                    }}
                    {move || {
                        let options: Vec<(String, String)> = std::iter::once((String::new(), "No objective".to_owned()))
                            .chain(match objectives.get() {
                                Some(Ok(o)) => o.into_iter().map(|o| (o.node.id.to_string(), o.label)).collect(),
                                _ => Vec::new(),
                            })
                            .collect();
                        view! { <SelectField options=options current=new_objective.get_untracked()
                                             on_change=move |v: String| new_objective.set(v) /> }
                    }}
                    <button class=BUTTON_PRIMARY type="submit">"Add"</button>
                </form>
            </Show>
            {move || match groups.get() {
                None => view! { <p class="p-4 text-muted">"Loading…"</p> }.into_any(),
                Some(Err(_)) => view! { <p class="p-4 text-muted">"Couldn't load projects."</p> }.into_any(),
                Some(Ok(g)) if !board.get() && g.is_empty() => view! {
                    <p class="p-4 text-muted">"No projects match. Add one, or clear the filters."</p>
                }.into_any(),
                Some(Ok(g)) if board.get() => view! { <Board groups=g /> }.into_any(),
                Some(Ok(g)) => view! { <ListView groups=g /> }.into_any(),
            }}
        </div>
    }
}

fn status_options() -> Vec<(String, String)> {
    std::iter::once((String::new(), "Any status".to_owned()))
        .chain(
            ProjectStatus::ALL
                .iter()
                .map(|s| (s.as_str().to_owned(), project_status_label(*s).to_owned())),
        )
        .collect()
}

#[component]
fn ListView(groups: Vec<ProjectGroup>) -> impl IntoView {
    let mut index = 0;
    let mut out = Vec::new();
    for group in groups {
        out.push(view! {
            <div class="px-3 pt-3 pb-1 text-[11px] font-semibold uppercase tracking-wide text-muted">
                {group.label} <span class="ml-1 font-normal text-faint">{group.rows.len()}</span>
            </div>
        }.into_any());
        for row in group.rows {
            out.push(project_row(row, index).into_any());
            index += 1;
        }
    }
    view! {
        <div class=format!("{COLS} px-3 py-1 text-[11px] uppercase tracking-wide text-muted border-b border-line")>
            <span>"Pri"</span><span>"Project"</span><span>"Owner"</span><span>"Status"</span>
            <span>"Target"</span><span class="text-right">"Tasks"</span>
        </div>
        <div class="flex-1 overflow-y-auto" role="table">{out}</div>
    }
}

fn project_row(row: ProjectRow, index: usize) -> impl IntoView {
    let node = node_of(&row);
    let tasks = progress(&row);
    let p = row.project;
    view! {
        <NodeRow node=node index=index>
            <div class=COLS>
                <span class="text-muted tabular-nums">{priority_short(p.priority)}</span>
                <span class="truncate">
                    <span class="font-medium">{p.title}</span>
                    <span class="ml-2 font-mono text-[11px] text-faint">{format!("#{}", p.slug)}</span>
                </span>
                <span class="truncate text-muted">{row.owner.map(|o| o.label).unwrap_or_default()}</span>
                <span class="text-muted">{project_status_label(p.status)}</span>
                <span class="text-muted tabular-nums">{p.target_date.map(|d| d.to_string()).unwrap_or_default()}</span>
                <span class="text-right tabular-nums text-muted">{tasks}</span>
            </div>
        </NodeRow>
    }
}

/// One column per status; drag a card onto another column to change its status.
#[component]
fn Board(groups: Vec<ProjectGroup>) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    // The card being dragged (id, current status) and the column it is over.
    let dragging = RwSignal::new(None::<(Uuid, ProjectStatus)>);
    let over = RwSignal::new(None::<ProjectStatus>);

    let mut index = 0;
    let columns = groups
        .into_iter()
        .map(|group| {
            let status = group.status;
            let count = group.rows.len();
            let cards = group
                .rows
                .into_iter()
                .map(|row| {
                    let card = view! { <BoardCard row=row index=index dragging=dragging over=over /> };
                    index += 1;
                    card
                })
                .collect_view();
            let drop = move |ev: ev::DragEvent| {
                ev.prevent_default();
                over.set(None);
                let (Some((id, from)), Some(to)) = (dragging.get_untracked(), status) else {
                    return;
                };
                dragging.set(None);
                if from == to {
                    return;
                }
                spawn_local(async move {
                    let patch = minimap_types::UpdateProject {
                        status: Some(to),
                        ..Default::default()
                    };
                    finish(api::update_project(id, patch).await, toasts, version);
                });
            };
            view! {
                <section
                    class=move || format!(
                        "flex w-64 shrink-0 flex-col rounded-sm border border-line {}",
                        if over.get() == status && dragging.get().is_some() { "bg-hover" } else { "bg-panel" })
                    on:dragover=move |ev: ev::DragEvent| { ev.prevent_default(); over.set(status); }
                    on:drop=drop
                >
                    <h2 class="flex items-center gap-2 px-2 py-1.5 text-[11px] font-semibold uppercase tracking-wide text-muted">
                        {group.label} <span class="font-normal text-faint">{count}</span>
                    </h2>
                    <div class="flex-1 space-y-1 overflow-y-auto p-1">{cards}</div>
                </section>
            }
        })
        .collect_view();

    view! { <div class="flex flex-1 gap-2 overflow-x-auto p-3">{columns}</div> }
}

#[component]
fn BoardCard(
    row: ProjectRow,
    index: usize,
    dragging: RwSignal<Option<(Uuid, ProjectStatus)>>,
    over: RwSignal<Option<ProjectStatus>>,
) -> impl IntoView {
    let selection = expect_context::<Selection>();
    let list = expect_context::<ListNav>();
    let node = node_of(&row);
    let (id, status) = (row.project.id, row.project.status);
    let tasks = progress(&row);
    let p = row.project;
    let is_open = move || selection.0.get() == Some(node);
    let on_cursor = move || list.cursor.get() == Some(index);
    let being_dragged = move || dragging.get().map(|d| d.0) == Some(id);

    view! {
        <div
            draggable="true"
            class=move || format!(
                "cursor-default select-none rounded-sm border border-line p-2 {} {}",
                if is_open() { "bg-active" }
                else if on_cursor() { "bg-hover shadow-[inset_2px_0_0_var(--color-muted)]" }
                else { "bg-canvas hover:bg-hover" },
                if being_dragged() { "opacity-40" } else { "" })
            on:click=move |_| {
                list.cursor.set(Some(index));
                selection.open(node);
            }
            on:dragstart=move |ev: ev::DragEvent| {
                // WebKit only starts a drag when it carries data.
                if let Some(dt) = ev.data_transfer() {
                    let _ = dt.set_data("text/plain", &id.to_string());
                }
                dragging.set(Some((id, status)));
            }
            on:dragend=move |_| {
                dragging.set(None);
                over.set(None);
            }
        >
            <div class="flex items-start gap-2">
                <span class="flex-1 break-words font-medium">{p.title}</span>
                <span class="text-[11px] tabular-nums text-muted">{priority_short(p.priority)}</span>
            </div>
            <div class="font-mono text-[11px] text-faint">{format!("#{}", p.slug)}</div>
            <div class="mt-1 flex flex-wrap gap-x-2 text-[11px] text-muted">
                {row.owner.map(|o| view! { <span>{o.label}</span> })}
                {p.target_date.map(|d| view! { <span class="tabular-nums">{d.to_string()}</span> })}
                {(!tasks.is_empty()).then(|| view! { <span class="tabular-nums">{tasks} " tasks"</span> })}
            </div>
        </div>
    }
}
