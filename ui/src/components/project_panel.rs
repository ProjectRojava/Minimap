//! Detail-pane body for a project: fields, objectives, dependencies, tasks, archive.

use std::str::FromStr;

use leptos::{prelude::*, task::spawn_local};
use minimap_types::{
    EdgeType, NewEdge, NodeRef, NodeSummary, NodeType, Patch, PersonRow, Project, ProjectDetail,
    ProjectStatus, TaskDisposition, UpdateProject, Uuid,
};

use crate::{
    api,
    components::{
        detail_pane::Section,
        form::{date_patch, SelectField, TextField, BUTTON, BUTTON_DANGER},
        health_panel::ProjectHealthSection,
        objective_panel::WeightInput,
        people_panel::{error_line, NodeButtons},
        schedule_panel::SchedulePanel,
        what_if_button::WhatIfButton,
    },
    labels::{humanize, priority_option, project_status_label, project_status_tone},
    state::{finish, DataVersion, Selection, Toasts},
};

#[component]
pub fn ProjectPanel(id: Uuid) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    // Fields (and the people list they need) load once per opening so typing is never overwritten.
    let project = LocalResource::new(move || api::get_project(id));
    let people = LocalResource::new(api::list_people);
    let detail = LocalResource::new(move || {
        version.track();
        api::get_project_detail(id)
    });
    let objectives = LocalResource::new(move || {
        version.track();
        api::list_node_summaries(NodeType::Objective)
    });
    let projects = LocalResource::new(move || {
        version.track();
        api::list_node_summaries(NodeType::Project)
    });

    view! {
        <Section title="Fields">
            {move || match (project.get(), people.get()) {
                (Some(Ok(p)), Some(Ok(ps))) => view! { <ProjectFields project=p people=ps /> }.into_any(),
                (Some(Err(e)), _) | (_, Some(Err(e))) => error_line(e),
                _ => view! { <p class="text-muted">"Loading…"</p> }.into_any(),
            }}
        </Section>
        {move || match (detail.get(), objectives.get(), projects.get()) {
            (Some(Ok(d)), Some(Ok(os)), Some(Ok(ps))) => view! {
                <Objectives detail=d.clone() candidates=os />
                <Dependencies detail=d.clone() candidates=ps />
                <ProjectHealthSection project=id />
                <SchedulePanel project=id />
                <WhatIfButton node=NodeRef::new(NodeType::Project, id) />
                <Tasks detail=d.clone() />
                <ArchiveProject detail=d />
            }.into_any(),
            (Some(Err(e)), _, _) | (_, Some(Err(e)), _) | (_, _, Some(Err(e))) => view! {
                <Section title="Objectives">{error_line(e)}</Section>
            }.into_any(),
            _ => view! { <Section title="Objectives"><p class="text-muted">"Loading…"</p></Section> }.into_any(),
        }}
    }
}

#[component]
fn ProjectFields(project: Project, people: Vec<PersonRow>) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let id = project.id;
    let save = move |patch: UpdateProject| {
        spawn_local(async move {
            finish(api::update_project(id, patch).await, toasts, version);
        });
    };
    let save_start = move |v: String| match date_patch(&v) {
        Ok(p) => save(UpdateProject {
            start_date: p,
            ..Default::default()
        }),
        Err(e) => {
            toasts.error(&e);
            version.bump();
        }
    };
    let save_target = move |v: String| match date_patch(&v) {
        Ok(p) => save(UpdateProject {
            target_date: p,
            ..Default::default()
        }),
        Err(e) => {
            toasts.error(&e);
            version.bump();
        }
    };
    let save_status = move |v: String| {
        if let Ok(s) = ProjectStatus::from_str(&v) {
            save(UpdateProject {
                status: Some(s),
                ..Default::default()
            });
        }
    };
    let save_priority = move |v: String| {
        if let Ok(p) = v.parse::<u8>() {
            save(UpdateProject {
                priority: Some(p),
                ..Default::default()
            });
        }
    };
    let save_owner = move |v: String| {
        let owner = match Uuid::parse_str(&v) {
            Ok(o) => Patch::Set(o),
            Err(_) => Patch::Clear,
        };
        save(UpdateProject {
            owner_person_id: owner,
            ..Default::default()
        });
    };

    let owner_options: Vec<(String, String)> =
        std::iter::once((String::new(), "— none —".to_owned()))
            .chain(
                people
                    .iter()
                    .map(|p| (p.person.id.to_string(), p.person.name.clone())),
            )
            .collect();
    let status_options: Vec<(String, String)> = ProjectStatus::ALL
        .iter()
        .map(|s| (s.as_str().to_owned(), project_status_label(*s).to_owned()))
        .collect();
    let priority_options: Vec<(String, String)> = (1..=5u8)
        .map(|p| (p.to_string(), priority_option(p)))
        .collect();

    view! {
        <TextField label="Title" value=project.title.clone()
            on_commit=move |v: String| save(UpdateProject { title: Some(v), ..Default::default() }) />
        <TextField label="Handle (for quick-add: #handle)" value=project.slug.clone()
            on_commit=move |v: String| save(UpdateProject { slug: Some(v), ..Default::default() }) />
        <TextField label="Description" multiline=true value=project.description.clone()
            on_commit=move |v: String| save(UpdateProject { description: Some(v), ..Default::default() }) />
        <div class="grid grid-cols-2 gap-3">
            <TextField label="Start date" kind="date"
                value=project.start_date.map(|d| d.to_string()).unwrap_or_default() on_commit=save_start />
            <TextField label="Target date" kind="date"
                value=project.target_date.map(|d| d.to_string()).unwrap_or_default() on_commit=save_target />
        </div>
        <div class="grid grid-cols-2 gap-3">
            <SelectField label="Status" options=status_options
                current=project.status.as_str().to_owned() on_change=save_status
                tone=project_status_tone(project.status).text() />
            <SelectField label="Priority" options=priority_options
                current=project.priority.to_string() on_change=save_priority />
        </div>
        <div class="mt-2">
            <SelectField label="Owner" options=owner_options
                current=project.owner_person_id.map(|o| o.to_string()).unwrap_or_default()
                on_change=save_owner />
        </div>
    }
}

/// A picker that reports the chosen node; used to add links.
#[component]
fn NodePicker(
    placeholder: &'static str,
    options: Vec<NodeSummary>,
    #[prop(into)] on_pick: Callback<NodeRef>,
) -> impl IntoView {
    let by_value: Vec<(String, NodeRef)> = options
        .iter()
        .map(|n| (n.node.id.to_string(), n.node))
        .collect();
    let choices: Vec<(String, String)> = std::iter::once((String::new(), placeholder.to_owned()))
        .chain(
            options
                .iter()
                .map(|n| (n.node.id.to_string(), n.label.clone())),
        )
        .collect();
    let pick = move |v: String| {
        if let Some((_, node)) = by_value.iter().find(|(id, _)| *id == v) {
            on_pick.run(*node);
        }
    };
    view! { <SelectField compact=true options=choices current=String::new() on_change=pick /> }
}

#[component]
fn Objectives(detail: ProjectDetail, candidates: Vec<NodeSummary>) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let selection = expect_context::<Selection>();
    let project = NodeRef::new(NodeType::Project, detail.project.id);

    let linked: std::collections::HashSet<Uuid> =
        detail.objectives.iter().map(|c| c.node.node.id).collect();
    let options: Vec<NodeSummary> = candidates
        .into_iter()
        .filter(|c| !linked.contains(&c.node.id))
        .collect();
    let none_exist = linked.is_empty() && options.is_empty();

    let add = move |objective: NodeRef| {
        let new = NewEdge {
            edge_type: EdgeType::ContributesTo,
            from: project,
            to: objective,
            attrs: serde_json::json!({ "weight": 1.0 }),
        };
        spawn_local(async move {
            finish(api::add_edge(new).await, toasts, version);
        });
    };
    let rows = detail.objectives.into_iter().map(|c| {
        let (edge_id, node) = (c.edge_id, c.node.node);
        let remove = move |_| {
            spawn_local(async move {
                finish(api::remove_edge(edge_id).await, toasts, version);
            });
        };
        view! {
            <li class="flex items-center gap-2">
                <button class="flex-1 truncate text-left hover:underline" on:click=move |_| selection.open(node)>
                    {c.node.label}
                </button>
                <span class="text-[11px] text-muted">{humanize(&c.status)}</span>
                <WeightInput edge_id=edge_id weight=c.weight />
                <button class="px-1 text-faint hover:text-danger" aria-label="Remove from objective"
                        on:click=remove>"✕"</button>
            </li>
        }
    }).collect_view();

    view! {
        <Section title="Objectives">
            <ul class="mb-2 space-y-1">{rows}</ul>
            <NodePicker placeholder="Add to an objective…" options=options on_pick=add />
            {none_exist.then(|| view! {
                <p class="mt-2 text-[11px] text-muted">"Create an objective first (Objectives screen), then link it here."</p>
            })}
        </Section>
    }
}

#[component]
fn Dependencies(detail: ProjectDetail, candidates: Vec<NodeSummary>) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let selection = expect_context::<Selection>();
    let id = detail.project.id;
    let project = NodeRef::new(NodeType::Project, id);

    let linked: std::collections::HashSet<Uuid> =
        detail.depends_on.iter().map(|d| d.node.node.id).collect();
    let options: Vec<NodeSummary> = candidates
        .into_iter()
        .filter(|c| c.node.id != id && !linked.contains(&c.node.id))
        .collect();

    // Loops are rejected by the backend and surface as a toast naming the projects.
    let add = move |target: NodeRef| {
        let new = NewEdge {
            edge_type: EdgeType::DependsOn,
            from: project,
            to: target,
            attrs: serde_json::json!({}),
        };
        spawn_local(async move {
            finish(api::add_edge(new).await, toasts, version);
        });
    };
    let rows = detail.depends_on.into_iter().map(|d| {
        let (edge_id, node) = (d.edge_id, d.node.node);
        let remove = move |_| {
            spawn_local(async move {
                finish(api::remove_edge(edge_id).await, toasts, version);
            });
        };
        view! {
            <li class="flex items-center gap-2">
                <button class="flex-1 truncate text-left hover:underline" on:click=move |_| selection.open(node)>
                    {d.node.label}
                </button>
                <button class="px-1 text-faint hover:text-danger" aria-label="Remove dependency"
                        on:click=remove>"✕"</button>
            </li>
        }
    }).collect_view();

    view! {
        <Section title="Dependencies">
            <p class="mb-1 text-[11px] text-muted">"Depends on"</p>
            <ul class="mb-2 space-y-1">{rows}</ul>
            <NodePicker placeholder="Add a dependency…" options=options on_pick=add />
            {(!detail.needed_by.is_empty()).then(|| view! {
                <p class="mt-3 mb-1 text-[11px] text-muted">"Needed by"</p>
                <NodeButtons nodes=detail.needed_by.clone() />
            })}
        </Section>
    }
}

#[component]
fn Tasks(detail: ProjectDetail) -> impl IntoView {
    let selection = expect_context::<Selection>();
    view! {
        <Section title="Tasks">
            {if detail.tasks.is_empty() {
                view! { <p class="text-muted">"No tasks yet. Tasks are created from the Tasks screen."</p> }.into_any()
            } else {
                view! {
                    <ul class="space-y-px">
                        {detail.tasks.into_iter().map(|t| {
                            let node = t.node.node;
                            view! {
                                <li class="flex items-center gap-2">
                                    <button class="flex-1 truncate rounded px-1.5 py-0.5 text-left hover:bg-hover"
                                            on:click=move |_| selection.open(node)>{t.node.label}</button>
                                    <span class="text-[11px] text-muted">{humanize(t.status.as_str())}</span>
                                    <span class="w-20 text-right text-[11px] tabular-nums text-muted">
                                        {t.due_date.map(|d| d.to_string()).unwrap_or_default()}
                                    </span>
                                </li>
                            }
                        }).collect_view()}
                    </ul>
                }.into_any()
            }}
        </Section>
    }
}

#[component]
fn ArchiveProject(detail: ProjectDetail) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let selection = expect_context::<Selection>();
    let id = detail.project.id;
    let title = detail.project.title.clone();
    // `Some(tasks)` = confirmation showing; the tasks are what the choice is about.
    let confirming = RwSignal::new(None::<Vec<NodeSummary>>);

    let start = move |_| {
        spawn_local(async move {
            match api::preview_archive_project(id).await {
                Ok(p) => confirming.set(Some(p.tasks)),
                Err(e) => toasts.error(&e),
            }
        });
    };
    let archive = move |tasks: TaskDisposition| {
        spawn_local(async move {
            if finish(api::archive_project(id, tasks).await, toasts, version).is_some() {
                confirming.set(None);
                selection.close();
            }
        });
    };

    view! {
        <Section title="Archive">
            {move || match confirming.get() {
                None => view! { <button class=BUTTON on:click=start>"Archive project…"</button> }.into_any(),
                Some(tasks) if tasks.is_empty() => view! {
                    <div class="space-y-2">
                        <p>"Archive " <strong>{title.clone()}</strong> "? It has no tasks."</p>
                        <div class="flex gap-2">
                            <button class=BUTTON_DANGER on:click=move |_| archive(TaskDisposition::Inbox)>"Archive"</button>
                            <button class=BUTTON on:click=move |_| confirming.set(None)>"Cancel"</button>
                        </div>
                    </div>
                }.into_any(),
                Some(tasks) => {
                    let n = tasks.len();
                    view! {
                        <div class="space-y-2">
                            <p>
                                "Archive " <strong>{title.clone()}</strong> ". What should happen to its "
                                {n} {if n == 1 { " task" } else { " tasks" }} "?"
                            </p>
                            <NodeButtons nodes=tasks />
                            <div class="flex flex-wrap gap-2">
                                <button class=BUTTON_DANGER on:click=move |_| archive(TaskDisposition::Archive)>"Archive tasks too"</button>
                                <button class=BUTTON on:click=move |_| archive(TaskDisposition::Inbox)>"Move tasks to inbox"</button>
                                <button class=BUTTON on:click=move |_| confirming.set(None)>"Cancel"</button>
                            </div>
                        </div>
                    }.into_any()
                }
            }}
        </Section>
    }
}
