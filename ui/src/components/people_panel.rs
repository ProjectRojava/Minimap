//! Detail-pane body for a person: editable fields, teams, manager, waiting-ons, archive.

use leptos::{prelude::*, task::spawn_local};
use minimap_types::{
    mention_token, AppError, CreateNote, EdgeType, NewEdge, NodeRef, NodeSummary, NodeType,
    NoteFilter, NoteKind, Patch, Person, PersonDetail, PersonRow, TeamRow, UpdatePerson, Uuid,
};

use crate::{
    api,
    components::{
        detail_pane::Section,
        form::{SelectField, TextField, BUTTON, BUTTON_DANGER},
    },
    state::{finish, DataVersion, Selection, Toasts},
};

pub const ROLES: [&str; 2] = ["member", "lead"];

fn role_options() -> Vec<(String, String)> {
    ROLES
        .iter()
        .map(|r| (r.to_string(), r.to_string()))
        .collect()
}

pub fn error_line(e: AppError) -> AnyView {
    view! { <p class="text-danger">{e.message}</p> }.into_any()
}

/// "· · Name" so nested teams read as a tree inside a flat dropdown.
pub fn indented(depth: u32, name: &str) -> String {
    format!("{}{name}", "· ".repeat(depth as usize))
}

pub fn team_options(teams: &[TeamRow], skip: impl Fn(Uuid) -> bool) -> Vec<(String, String)> {
    teams
        .iter()
        .filter(|t| !skip(t.team.id))
        .map(|t| (t.team.id.to_string(), indented(t.depth, &t.team.name)))
        .collect()
}

#[component]
pub fn PersonPanel(id: Uuid) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    // Fields load once per opening so typing is never overwritten by a reload.
    let person = LocalResource::new(move || api::get_person(id));
    let detail = LocalResource::new(move || {
        version.track();
        api::get_person_detail(id)
    });
    let people = LocalResource::new(move || {
        version.track();
        api::list_people()
    });
    let teams = LocalResource::new(move || {
        version.track();
        api::list_teams()
    });

    view! {
        <Section title="Fields">
            {move || match person.get() {
                None => view! { <p class="text-muted">"Loading…"</p> }.into_any(),
                Some(Err(e)) => error_line(e),
                Some(Ok(p)) => view! { <PersonFields person=p /> }.into_any(),
            }}
        </Section>
        {move || match (detail.get(), people.get(), teams.get()) {
            (Some(Ok(d)), Some(Ok(ps)), Some(Ok(ts))) => view! {
                <Organization detail=d.clone() people=ps teams=ts />
                <WaitingOnList detail=d.clone() />
                <PersonNotes detail=d.clone() />
                <ArchivePerson detail=d />
            }.into_any(),
            (Some(Err(e)), _, _) | (_, Some(Err(e)), _) | (_, _, Some(Err(e))) => view! {
                <Section title="Organization">{error_line(e)}</Section>
            }.into_any(),
            _ => view! { <Section title="Organization"><p class="text-muted">"Loading…"</p></Section> }.into_any(),
        }}
    }
}

#[component]
fn PersonFields(person: Person) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let id = person.id;
    let save = move |patch: UpdatePerson| {
        spawn_local(async move {
            finish(api::update_person(id, patch).await, toasts, version);
        });
    };

    let save_capacity = move |v: String| match v.trim().parse::<f64>() {
        Ok(h) => save(UpdatePerson {
            weekly_capacity_hours: Some(h),
            ..Default::default()
        }),
        Err(_) => {
            toasts.error(&AppError {
                code: "invalid".into(),
                message: "Weekly capacity must be a number of hours".into(),
            });
            version.bump();
        }
    };

    view! {
        <TextField label="Name" value=person.name.clone()
            on_commit=move |v: String| save(UpdatePerson { name: Some(v), ..Default::default() }) />
        <TextField label="Role" value=person.role_title.clone()
            on_commit=move |v: String| save(UpdatePerson { role_title: Some(v), ..Default::default() }) />
        <TextField label="Email" kind="email" value=person.email.clone().unwrap_or_default()
            on_commit=move |v: String| {
                let v = v.trim().to_owned();
                save(UpdatePerson {
                    email: if v.is_empty() { Patch::Clear } else { Patch::Set(v) },
                    ..Default::default()
                })
            } />
        <TextField label="Weekly capacity (hours)" kind="number"
            value=person.weekly_capacity_hours.to_string() on_commit=save_capacity />
        <TextField label="Notes" multiline=true value=person.notes.clone()
            on_commit=move |v: String| save(UpdatePerson { notes: Some(v), ..Default::default() }) />
    }
}

#[component]
fn Organization(
    detail: PersonDetail,
    people: Vec<PersonRow>,
    teams: Vec<TeamRow>,
) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let selection = expect_context::<Selection>();
    let id = detail.person.id;

    let manager_options: Vec<(String, String)> =
        std::iter::once((String::new(), "— none —".to_owned()))
            .chain(
                people
                    .iter()
                    .filter(|p| p.person.id != id)
                    .map(|p| (p.person.id.to_string(), p.person.name.clone())),
            )
            .collect();
    let manager_now = detail
        .manager
        .as_ref()
        .map(|m| m.node.node.id.to_string())
        .unwrap_or_default();
    let set_manager = move |v: String| {
        let manager = Uuid::parse_str(&v).ok();
        spawn_local(async move {
            finish(api::set_manager(id, manager).await, toasts, version);
        });
    };

    let member_edge = move |team: Uuid, role: &str| NewEdge {
        edge_type: EdgeType::MemberOf,
        from: NodeRef::new(NodeType::Person, id),
        to: NodeRef::new(NodeType::Team, team),
        attrs: serde_json::json!({ "role": role }),
    };
    let add_team = move |v: String| {
        if let Ok(team) = Uuid::parse_str(&v) {
            spawn_local(async move {
                finish(
                    api::add_edge(member_edge(team, "member")).await,
                    toasts,
                    version,
                );
            });
        }
    };

    let member_of: std::collections::HashSet<Uuid> =
        detail.memberships.iter().map(|m| m.node.node.id).collect();
    let add_options: Vec<(String, String)> =
        std::iter::once((String::new(), "Add to team…".to_owned()))
            .chain(team_options(&teams, |t| member_of.contains(&t)))
            .collect();

    let rows = detail.memberships.into_iter().map(|m| {
        let (edge_id, node) = (m.edge_id, m.node.node);
        let change_role = move |role: String| {
            spawn_local(async move {
                let attrs = serde_json::json!({ "role": role });
                finish(api::update_edge_attrs(edge_id, attrs).await, toasts, version);
            });
        };
        let remove = move |_| {
            spawn_local(async move {
                finish(api::remove_edge(edge_id).await, toasts, version);
            });
        };
        view! {
            <li class="flex items-center gap-2">
                <button class="flex-1 truncate text-left hover:underline" on:click=move |_| selection.open(node)>
                    {m.node.label}
                </button>
                <SelectField compact=true current=m.role options=role_options() on_change=change_role />
                <button class="px-1 text-faint hover:text-danger" aria-label="Remove from team"
                        on:click=remove>"✕"</button>
            </li>
        }
    }).collect_view();

    view! {
        <Section title="Organization">
            <SelectField label="Manager" options=manager_options current=manager_now on_change=set_manager />
            <p class="mt-3 mb-1 text-[11px] text-muted">"Teams"</p>
            <ul class="mb-2 space-y-1">{rows}</ul>
            <SelectField compact=true options=add_options current=String::new() on_change=add_team />
            {(!detail.reports.is_empty()).then(|| {
                let reports = detail.reports.clone();
                view! {
                    <p class="mt-3 mb-1 text-[11px] text-muted">"Direct reports"</p>
                    <NodeButtons nodes=reports />
                }
            })}
        </Section>
    }
}

/// A list of nodes, each a button that opens it in the pane.
#[component]
pub fn NodeButtons(nodes: Vec<NodeSummary>) -> impl IntoView {
    let selection = expect_context::<Selection>();
    view! {
        <ul class="space-y-px">
            {nodes.into_iter().map(|n| {
                let node = n.node;
                view! {
                    <li>
                        <button class="w-full rounded px-1.5 py-0.5 text-left hover:bg-hover"
                                on:click=move |_| selection.open(node)>{n.label}</button>
                    </li>
                }
            }).collect_view()}
        </ul>
    }
}

/// Notes that mention this person (their 1:1s first among equals: newest first), and a
/// shortcut to start a new 1:1 that already mentions them.
#[component]
fn PersonNotes(detail: PersonDetail) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let selection = expect_context::<Selection>();
    let id = detail.person.id;
    let name = detail.person.name.clone();
    let is_self = detail.person.is_self;

    let notes = LocalResource::new(move || {
        version.track();
        api::list_notes(NoteFilter {
            mentions_id: Some(id),
            ..Default::default()
        })
    });
    let new_one_on_one = move |_| {
        let input = CreateNote {
            title: format!("1:1 with {name}"),
            body: format!("{}\n\n", mention_token(&name, id)),
            note_date: None,
            kind: Some(NoteKind::OneOnOne),
            recurrence: None,
        };
        spawn_local(async move {
            if let Some(n) = finish(api::create_note(input).await, toasts, version) {
                selection.open(NodeRef::new(NodeType::Note, n.id));
            }
        });
    };

    view! {
        <Section title="Notes">
            {(!is_self).then(|| view! {
                <button class=BUTTON on:click=new_one_on_one>"New 1:1"</button>
            })}
            {move || match notes.get() {
                None => view! { <p class="mt-2 text-muted">"Loading…"</p> }.into_any(),
                Some(Err(e)) => error_line(e),
                Some(Ok(rows)) if rows.is_empty() => view! {
                    <p class="mt-2 text-muted">"No notes mention them yet."</p>
                }.into_any(),
                Some(Ok(rows)) => view! {
                    <ul class="mt-2 space-y-px">
                        {rows.into_iter().take(10).map(|n| {
                            let node = NodeRef::new(NodeType::Note, n.id);
                            view! {
                                <li>
                                    <button class="flex w-full items-center gap-2 rounded px-1.5 py-0.5 text-left hover:bg-hover"
                                            on:click=move |_| selection.open(node)>
                                        <span class="w-20 shrink-0 tabular-nums text-muted">{n.note_date.to_string()}</span>
                                        <span class="truncate">{n.title}</span>
                                    </button>
                                </li>
                            }
                        }).collect_view()}
                    </ul>
                }.into_any(),
            }}
        </Section>
    }
}

#[component]
fn WaitingOnList(detail: PersonDetail) -> impl IntoView {
    let selection = expect_context::<Selection>();
    view! {
        <Section title="Waiting on them">
            {if detail.waiting_ons.is_empty() {
                view! { <p class="text-muted">"Nothing outstanding."</p> }.into_any()
            } else {
                view! {
                    <ul class="space-y-px">
                        {detail.waiting_ons.into_iter().map(|w| {
                            let node = NodeRef::new(NodeType::WaitingOn, w.id);
                            view! {
                                <li>
                                    <button class="w-full rounded px-1.5 py-0.5 text-left hover:bg-hover"
                                            on:click=move |_| selection.open(node)>
                                        {w.description}
                                        <span class="ml-2 text-[11px] text-muted">"since " {w.asked_on.to_string()}</span>
                                    </button>
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
fn ArchivePerson(detail: PersonDetail) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let selection = expect_context::<Selection>();
    let id = detail.person.id;
    let name = detail.person.name.clone();
    // `Some(tasks)` = confirmation showing, listing the tasks that lose their assignee.
    let confirming = RwSignal::new(None::<Vec<NodeSummary>>);

    let start = move |_| {
        spawn_local(async move {
            match api::preview_archive_person(id).await {
                Ok(p) => confirming.set(Some(p.assigned_tasks)),
                Err(e) => toasts.error(&e),
            }
        });
    };
    let confirm = move |_| {
        spawn_local(async move {
            if finish(api::archive_person(id).await, toasts, version).is_some() {
                confirming.set(None);
                selection.close();
            }
        });
    };

    if detail.person.is_self {
        return view! {
            <Section title="Archive"><p class="text-muted">"This is you, so it can't be archived."</p></Section>
        }
        .into_any();
    }

    view! {
        <Section title="Archive">
            {move || match confirming.get() {
                None => view! { <button class=BUTTON on:click=start>"Archive person…"</button> }.into_any(),
                Some(tasks) => view! {
                    <div class="space-y-2">
                        <p>
                            "Archive " <strong>{name.clone()}</strong> "? Their links are archived with them"
                            {if tasks.is_empty() { ".".to_owned() } else {
                                format!(", so {} active task{} will become unassigned:", tasks.len(), if tasks.len() == 1 { "" } else { "s" })
                            }}
                        </p>
                        <NodeButtons nodes=tasks />
                        <div class="flex gap-2">
                            <button class=BUTTON_DANGER on:click=confirm>"Archive"</button>
                            <button class=BUTTON on:click=move |_| confirming.set(None)>"Cancel"</button>
                        </div>
                    </div>
                }.into_any(),
            }}
        </Section>
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_labels_are_indented() {
        assert_eq!(indented(0, "Eng"), "Eng");
        assert_eq!(indented(2, "Infra"), "· · Infra");
    }
}
