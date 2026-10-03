use leptos::{prelude::*, task::spawn_local};
use minimap_types::{CreateTeam, NodeRef, NodeType, Uuid};

use crate::{
    api,
    components::{
        form::{SelectField, BUTTON, BUTTON_PRIMARY, INPUT},
        node_row::NodeRow,
        people_panel::team_options,
    },
    state::{finish, DataVersion, ListNav, Selection, Toasts},
};

#[component]
pub fn Teams() -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let list = expect_context::<ListNav>();
    let selection = expect_context::<Selection>();

    let rows = LocalResource::new(move || {
        version.track();
        api::list_teams()
    });
    Effect::new(move |_| match rows.get() {
        Some(Ok(r)) => list.set_items(
            r.iter()
                .map(|t| NodeRef::new(NodeType::Team, t.team.id))
                .collect(),
        ),
        Some(Err(e)) => toasts.error(&e),
        None => {}
    });

    let adding = RwSignal::new(false);
    let name = RwSignal::new(String::new());
    let parent = RwSignal::new(String::new());
    let submit = move || {
        let n = name.get_untracked();
        if n.trim().is_empty() {
            return;
        }
        let parent_team_id = Uuid::parse_str(&parent.get_untracked()).ok();
        spawn_local(async move {
            let input = CreateTeam {
                name: n,
                description: String::new(),
                parent_team_id,
            };
            if let Some(t) = finish(api::create_team(input).await, toasts, version) {
                name.set(String::new());
                selection.open(NodeRef::new(NodeType::Team, t.id));
            }
        });
    };

    view! {
        <div class="flex flex-col h-full">
            <header class="flex items-center gap-3 px-4 h-10 shrink-0 border-b border-line">
                <h1 class="text-[13px] font-semibold">"Teams"</h1>
                <button class=BUTTON on:click=move |_| adding.update(|a| *a = !*a)>
                    {move || if adding.get() { "Cancel" } else { "New team" }}
                </button>
            </header>
            <Show when=move || adding.get()>
                <form class="flex items-end gap-2 px-4 py-2 border-b border-line bg-panel"
                      on:submit=move |ev| { ev.prevent_default(); submit(); }>
                    <input class=INPUT placeholder="Team name" autofocus prop:value=move || name.get()
                           on:input=move |ev| name.set(event_target_value(&ev)) />
                    {move || {
                        let options: Vec<(String, String)> = std::iter::once((String::new(), "— top level —".to_owned()))
                            .chain(match rows.get() {
                                Some(Ok(r)) => team_options(&r, |_| false),
                                _ => Vec::new(),
                            })
                            .collect();
                        view! { <SelectField options=options current=parent.get_untracked() on_change=move |v: String| parent.set(v) /> }
                    }}
                    <button class=BUTTON_PRIMARY type="submit">"Add"</button>
                </form>
            </Show>
            <div class="flex-1 overflow-y-auto" role="tree">
                {move || match rows.get() {
                    None => view! { <p class="p-4 text-muted">"Loading…"</p> }.into_any(),
                    Some(Err(_)) => view! { <p class="p-4 text-muted">"Couldn't load teams."</p> }.into_any(),
                    Some(Ok(r)) if r.is_empty() => view! {
                        <p class="p-4 text-muted">"No teams yet. Create one, then add people to it from their page."</p>
                    }.into_any(),
                    Some(Ok(r)) => r.into_iter().enumerate().map(|(i, t)| {
                        let node = NodeRef::new(NodeType::Team, t.team.id);
                        let indent = format!("padding-left: {}rem", f64::from(t.depth) * 1.25);
                        view! {
                            <NodeRow node=node index=i>
                                <div class="flex w-full items-center gap-3" style=indent>
                                    <span class="flex-1 truncate font-medium">{t.team.name}</span>
                                    <span class="text-[11px] text-muted">
                                        {t.member_count} {if t.member_count == 1 { " member" } else { " members" }}
                                    </span>
                                </div>
                            </NodeRow>
                        }
                    }).collect_view().into_any(),
                }}
            </div>
        </div>
    }
}
