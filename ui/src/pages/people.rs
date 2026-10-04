use leptos::{prelude::*, task::spawn_local};
use minimap_types::{CreatePerson, NodeRef, NodeType};

use crate::{
    api,
    components::{
        form::{BUTTON_PRIMARY, BUTTON_SOFT, INPUT},
        node_row::NodeRow,
        page::{column_head, EmptyState, PageHeader, FORM_BAR},
    },
    state::{finish, DataVersion, ListNav, Selection, Toasts},
};

const COLS: &str = "grid w-full items-center gap-3 grid-cols-[minmax(0,1.6fr)_minmax(0,1.2fr)_minmax(0,1.6fr)_4.5rem_4.5rem]";

#[component]
pub fn People() -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let list = expect_context::<ListNav>();
    let selection = expect_context::<Selection>();

    let rows = LocalResource::new(move || {
        version.track();
        api::list_people()
    });
    Effect::new(move |_| match rows.get() {
        Some(Ok(r)) => list.set_items(
            r.iter()
                .map(|p| NodeRef::new(NodeType::Person, p.person.id))
                .collect(),
        ),
        Some(Err(e)) => toasts.error(&e),
        None => {}
    });

    let adding = RwSignal::new(false);
    list.on_new(move || adding.set(true));
    let name = RwSignal::new(String::new());
    let role = RwSignal::new(String::new());
    let submit = move || {
        let (n, r) = (name.get_untracked(), role.get_untracked());
        if n.trim().is_empty() {
            return;
        }
        spawn_local(async move {
            let input = CreatePerson {
                name: n,
                role_title: r,
                email: None,
                weekly_capacity_hours: None,
                is_self: false,
                notes: String::new(),
            };
            if let Some(p) = finish(api::create_person(input).await, toasts, version) {
                name.set(String::new());
                role.set(String::new());
                selection.open(NodeRef::new(NodeType::Person, p.id));
            }
        });
    };

    view! {
        <div class="flex flex-col h-full">
            <PageHeader icon="people" title="People" subtitle="The people you work with, their teams and load">
                <button class=BUTTON_SOFT on:click=move |_| adding.update(|a| *a = !*a)>
                    {move || if adding.get() { "Cancel" } else { "New person" }}
                </button>
            </PageHeader>
            <Show when=move || adding.get()>
                <form class=FORM_BAR
                      on:submit=move |ev| { ev.prevent_default(); submit(); }>
                    <input class=INPUT placeholder="Name" autofocus prop:value=move || name.get()
                           on:input=move |ev| name.set(event_target_value(&ev)) />
                    <input class=INPUT placeholder="Role (optional)" prop:value=move || role.get()
                           on:input=move |ev| role.set(event_target_value(&ev)) />
                    <button class=BUTTON_PRIMARY type="submit">"Add"</button>
                </form>
            </Show>
            <div class=column_head(COLS)>
                <span>"Name"</span><span>"Role"</span><span>"Teams"</span>
                <span class="text-right">"Tasks"</span><span class="text-right">"Waiting"</span>
            </div>
            <div class="flex-1 overflow-y-auto" role="table">
                {move || match rows.get() {
                    None => view! { <p class="p-4 text-muted">"Loading…"</p> }.into_any(),
                    Some(Err(_)) => view! { <p class="p-4 text-muted">"Couldn't load people."</p> }.into_any(),
                    Some(Ok(r)) if r.is_empty() => view! {
                        <EmptyState icon="people" title="No people yet"
                            hint="Add the people you work with; assign them tasks and track what you wait on." />
                    }.into_any(),
                    Some(Ok(r)) => r.into_iter().enumerate().map(|(i, p)| {
                        let node = NodeRef::new(NodeType::Person, p.person.id);
                        let teams = p.teams.iter().map(|t| t.label.clone()).collect::<Vec<_>>().join(", ");
                        view! {
                            <NodeRow node=node index=i>
                                <div class=COLS>
                                    <span class="truncate font-medium">
                                        {p.person.name}
                                        {p.person.is_self.then(|| view! { <span class="ml-1.5 text-[11px] font-normal text-muted">"you"</span> })}
                                    </span>
                                    <span class="truncate text-muted">{p.person.role_title}</span>
                                    <span class="truncate text-muted">{teams}</span>
                                    <span class="text-right tabular-nums">{p.active_task_count}</span>
                                    <span class="text-right tabular-nums">{p.open_waiting_on_count}</span>
                                </div>
                            </NodeRow>
                        }
                    }).collect_view().into_any(),
                }}
            </div>
        </div>
    }
}
