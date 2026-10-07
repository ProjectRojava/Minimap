//! Detail-pane body for a team: fields, parent, sub-teams, members, archive.

use leptos::{prelude::*, task::spawn_local};
use minimap_types::{NodeSummary, Patch, Team, TeamDetail, TeamRow, UpdateTeam, Uuid};

use crate::{
    api,
    components::{
        detail_pane::Section,
        form::{SelectField, TextField, BUTTON, BUTTON_DANGER},
        people_panel::{error_line, team_options, NodeButtons},
    },
    state::{finish, DataVersion, Selection, Toasts},
};

#[component]
pub fn TeamPanel(id: Uuid) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let team = LocalResource::new(move || api::get_team(id));
    let detail = LocalResource::new(move || {
        version.track();
        api::get_team_detail(id)
    });
    let teams = LocalResource::new(move || {
        version.track();
        api::list_teams()
    });

    view! {
        <Section title="Fields">
            {move || match team.get() {
                None => view! { <p class="text-muted">"Loading…"</p> }.into_any(),
                Some(Err(e)) => error_line(e),
                Some(Ok(t)) => view! { <TeamFields team=t /> }.into_any(),
            }}
        </Section>
        {move || match (detail.get(), teams.get()) {
            (Some(Ok(d)), Some(Ok(ts))) => view! {
                <Structure detail=d.clone() teams=ts />
                <Members detail=d.clone() />
                <ArchiveTeam detail=d />
            }.into_any(),
            (Some(Err(e)), _) | (_, Some(Err(e))) => view! {
                <Section title="Structure">{error_line(e)}</Section>
            }.into_any(),
            _ => view! { <Section title="Structure"><p class="text-muted">"Loading…"</p></Section> }.into_any(),
        }}
    }
}

#[component]
fn TeamFields(team: Team) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let id = team.id;
    let save = move |patch: UpdateTeam| {
        spawn_local(async move {
            finish(api::update_team(id, patch).await, toasts, version);
        });
    };
    view! {
        <TextField label="Name" value=team.name.clone()
            on_commit=move |v: String| save(UpdateTeam { name: Some(v), ..Default::default() }) />
        <TextField label="Description" multiline=true value=team.description.clone()
            on_commit=move |v: String| save(UpdateTeam { description: Some(v), ..Default::default() }) />
    }
}

#[component]
fn Structure(detail: TeamDetail, teams: Vec<TeamRow>) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let id = detail.team.id;

    let options: Vec<(String, String)> =
        std::iter::once((String::new(), "— top level —".to_owned()))
            .chain(team_options(&teams, |t| t == id))
            .collect();
    let current = detail
        .parent
        .as_ref()
        .map(|p| p.node.id.to_string())
        .unwrap_or_default();
    let set_parent = move |v: String| {
        let patch = UpdateTeam {
            parent_team_id: match Uuid::parse_str(&v) {
                Ok(p) => Patch::Set(p),
                Err(_) => Patch::Clear,
            },
            ..Default::default()
        };
        spawn_local(async move {
            finish(api::update_team(id, patch).await, toasts, version);
        });
    };

    view! {
        <Section title="Structure">
            <SelectField label="Parent team" options=options current=current on_change=set_parent />
            <p class="mt-3 mb-1 text-[11px] text-muted">"Sub-teams"</p>
            {if detail.children.is_empty() {
                view! { <p class="text-muted">"None."</p> }.into_any()
            } else {
                view! { <NodeButtons nodes=detail.children.clone() /> }.into_any()
            }}
        </Section>
    }
}

#[component]
fn Members(detail: TeamDetail) -> impl IntoView {
    let selection = expect_context::<Selection>();
    view! {
        <Section title="Members">
            {if detail.members.is_empty() {
                view! { <p class="text-muted">"No members yet. Add people from their own page."</p> }.into_any()
            } else {
                view! {
                    <ul class="space-y-px">
                        {detail.members.into_iter().map(|m| {
                            let node = m.node.node;
                            view! {
                                <li class="flex items-center gap-2">
                                    <button class="flex-1 truncate rounded px-1.5 py-0.5 text-left hover:bg-hover"
                                            on:click=move |_| selection.open(node)>{m.node.label}</button>
                                    <span class="text-[11px] text-muted">{m.role}</span>
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
fn ArchiveTeam(detail: TeamDetail) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let selection = expect_context::<Selection>();
    let id = detail.team.id;
    let name = detail.team.name.clone();
    let members: Vec<NodeSummary> = detail.members.iter().map(|m| m.node.clone()).collect();
    let confirming = RwSignal::new(false);

    let confirm = move |_| {
        spawn_local(async move {
            // Refused (with a toast) while the team still has active sub-teams.
            if finish(api::archive_team(id).await, toasts, version).is_some() {
                confirming.set(false);
                selection.close();
            }
        });
    };

    view! {
        <Section title="Archive" tone=crate::components::page::Tone::Danger>
            {move || if confirming.get() {
                view! {
                    <div class="space-y-2">
                        <p>
                            "Archive " <strong>{name.clone()}</strong> "? "
                            {if members.is_empty() { "It has no members.".to_owned() } else {
                                format!("Its {} membership{} will be archived with it.", members.len(), if members.len() == 1 { "" } else { "s" })
                            }}
                        </p>
                        <div class="flex gap-2">
                            <button class=BUTTON_DANGER on:click=confirm>"Archive"</button>
                            <button class=BUTTON on:click=move |_| confirming.set(false)>"Cancel"</button>
                        </div>
                    </div>
                }.into_any()
            } else {
                view! { <button class=BUTTON on:click=move |_| confirming.set(true)>"Archive team…"</button> }.into_any()
            }}
        </Section>
    }
}
