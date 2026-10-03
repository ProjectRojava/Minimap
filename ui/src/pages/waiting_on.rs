use leptos::{prelude::*, task::spawn_local};
use minimap_types::{
    CreateWaitingOn, EdgeType, NewEdge, NodeRef, NodeSummary, NodeType, Uuid, WaitingOnFilter,
    WaitingOnRow,
};

use crate::{
    api,
    components::{
        form::{DateField, SelectField, BUTTON, BUTTON_PRIMARY, INPUT},
        node_row::NodeRow,
        objective_panel::{candidate_value, parse_candidate},
        waiting_panel::{age_text, snooze_options},
    },
    nav::type_label,
    state::{finish, DataVersion, ListNav, Selection, Toasts},
};

const COLS: &str =
    "grid w-full items-center gap-3 grid-cols-[4.5rem_minmax(0,1fr)_8rem_9rem_6.5rem_12.5rem]";

/// What you're waiting on other people for, oldest first. Stale items stand out; snoozed
/// ones are hidden until their follow-up date.
#[component]
pub fn WaitingOn() -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let list = expect_context::<ListNav>();
    let selection = expect_context::<Selection>();

    let person = RwSignal::new(String::new());
    let show_snoozed = RwSignal::new(false);
    let show_resolved = RwSignal::new(false);

    let rows = LocalResource::new(move || {
        version.track();
        api::get_waiting_on(WaitingOnFilter {
            person_id: Uuid::parse_str(&person.get()).ok(),
            include_resolved: show_resolved.get(),
            include_snoozed: show_snoozed.get(),
        })
    });
    let people = LocalResource::new(move || {
        version.track();
        api::list_people()
    });
    let targets = LocalResource::new(move || {
        version.track();
        async move {
            let mut all: Vec<NodeSummary> = api::list_node_summaries(NodeType::Project).await?;
            all.extend(api::list_node_summaries(NodeType::Task).await?);
            Ok::<_, minimap_types::AppError>(all)
        }
    });

    Effect::new(move |_| match rows.get() {
        Some(Ok(r)) => list.set_items(
            r.iter()
                .map(|w| NodeRef::new(NodeType::WaitingOn, w.waiting.id))
                .collect(),
        ),
        Some(Err(e)) => toasts.error(&e),
        None => {}
    });

    // Row shortcuts: x resolves (or reopens), s snoozes for three days.
    list.on_row_key(move |key, node| {
        let Some(Ok(all)) = rows.get_untracked() else {
            return;
        };
        let Some(row) = all.into_iter().find(|r| r.waiting.id == node.id) else {
            return;
        };
        let id = row.waiting.id;
        let resolved = row.waiting.resolved_on.is_some();
        match key.as_str() {
            "x" => {
                spawn_local(async move {
                    let result = if resolved {
                        api::reopen_waiting_on(id).await
                    } else {
                        api::resolve_waiting_on(id).await
                    };
                    finish(result, toasts, version);
                });
            }
            "s" if !resolved => {
                spawn_local(async move {
                    finish(api::snooze_waiting_on(id, Some(3)).await, toasts, version);
                });
            }
            _ => {}
        }
    });

    // New waiting-on.
    let adding = RwSignal::new(false);
    list.on_new(move || adding.set(true));
    let text = RwSignal::new(String::new());
    let who = RwSignal::new(String::new());
    let expected = RwSignal::new(String::new());
    let about = RwSignal::new(String::new());
    let submit = move || {
        let description = text.get_untracked();
        if description.trim().is_empty() {
            return;
        }
        let Ok(person_id) = Uuid::parse_str(&who.get_untracked()) else {
            toasts.error(&minimap_types::AppError {
                code: "invalid".into(),
                message: "Choose who you're waiting on".into(),
            });
            return;
        };
        let expected_by = match expected.get_untracked().trim() {
            "" => None,
            d => match minimap_types::timefmt::parse_date(d) {
                Ok(d) => Some(d),
                Err(_) => {
                    toasts.error(&minimap_types::AppError {
                        code: "invalid".into(),
                        message: "Expected by must be a date like 2027-03-31".into(),
                    });
                    return;
                }
            },
        };
        let target = parse_candidate(&about.get_untracked());
        spawn_local(async move {
            let input = CreateWaitingOn {
                description,
                person_id,
                asked_on: None,
                expected_by,
                follow_up_on: None,
            };
            let Some(w) = finish(api::create_waiting_on(input).await, toasts, version) else {
                return;
            };
            if let Some(to) = target {
                let link = NewEdge {
                    edge_type: EdgeType::About,
                    from: NodeRef::new(NodeType::WaitingOn, w.id),
                    to,
                    attrs: serde_json::json!({}),
                };
                finish(api::add_edge(link).await, toasts, version);
            }
            text.set(String::new());
            expected.set(String::new());
            about.set(String::new());
            selection.open(NodeRef::new(NodeType::WaitingOn, w.id));
        });
    };

    view! {
        <div class="flex flex-col h-full">
            <header class="flex items-center gap-3 px-4 h-10 shrink-0 border-b border-line">
                <h1 class="text-[13px] font-semibold">"Waiting on"</h1>
                <button class=BUTTON on:click=move |_| adding.update(|a| *a = !*a)>
                    {move || if adding.get() { "Cancel" } else { "New" }}
                </button>
                <span class="ml-auto text-[11px] text-muted">"n new · j/k move · x resolve · s snooze 3 days"</span>
            </header>
            <div class="flex flex-wrap items-center gap-3 px-4 py-1.5 shrink-0 border-b border-line">
                {move || {
                    let options: Vec<(String, String)> = std::iter::once((String::new(), "Anyone".to_owned()))
                        .chain(match people.get() {
                            Some(Ok(p)) => p.into_iter().map(|p| (p.person.id.to_string(), p.person.name)).collect(),
                            _ => Vec::new(),
                        })
                        .collect();
                    view! { <SelectField compact=true current=person.get_untracked() options=options
                                         on_change=move |v: String| person.set(v) /> }
                }}
                <label class="flex items-center gap-1 text-[11px] text-muted">
                    <input type="checkbox" prop:checked=move || show_snoozed.get()
                           on:change=move |ev| show_snoozed.set(event_target_checked(&ev)) />
                    "Show snoozed"
                </label>
                <label class="flex items-center gap-1 text-[11px] text-muted">
                    <input type="checkbox" prop:checked=move || show_resolved.get()
                           on:change=move |ev| show_resolved.set(event_target_checked(&ev)) />
                    "Show resolved"
                </label>
            </div>
            <Show when=move || adding.get()>
                <form class="flex flex-wrap items-end gap-2 px-4 py-2 border-b border-line bg-panel"
                      on:submit=move |ev| { ev.prevent_default(); submit(); }>
                    <input class=format!("{INPUT} !w-72") placeholder="What are you waiting for?" autofocus
                           prop:value=move || text.get() on:input=move |ev| text.set(event_target_value(&ev)) />
                    {move || {
                        let options: Vec<(String, String)> = std::iter::once((String::new(), "Who from…".to_owned()))
                            .chain(match people.get() {
                                Some(Ok(p)) => p.into_iter().map(|p| (p.person.id.to_string(), p.person.name)).collect(),
                                _ => Vec::new(),
                            })
                            .collect();
                        view! { <SelectField options=options current=who.get_untracked() on_change=move |v: String| who.set(v) /> }
                    }}
                    <DateField compact=true placeholder="Expected by" current=expected.get_untracked() on_commit=move |v: String| expected.set(v) />
                    {move || {
                        let options: Vec<(String, String)> = std::iter::once((String::new(), "About… (optional)".to_owned()))
                            .chain(match targets.get() {
                                Some(Ok(t)) => t.into_iter().map(|n| (candidate_value(&n), format!("{} · {}", type_label(n.node.node_type), n.label))).collect(),
                                _ => Vec::new(),
                            })
                            .collect();
                        view! { <SelectField options=options current=about.get_untracked() on_change=move |v: String| about.set(v) /> }
                    }}
                    <button class=BUTTON_PRIMARY type="submit">"Add"</button>
                </form>
            </Show>
            <div class=format!("{COLS} px-3 py-1 text-[11px] uppercase tracking-wide text-muted border-b border-line")>
                <span>"Age"</span><span>"Waiting for"</span><span>"From"</span><span>"About"</span>
                <span>"Expected"</span><span></span>
            </div>
            <div class="flex-1 overflow-y-auto" role="table">
                {move || match rows.get() {
                    None => view! { <p class="p-4 text-muted">"Loading…"</p> }.into_any(),
                    Some(Err(_)) => view! { <p class="p-4 text-muted">"Couldn't load waiting-ons."</p> }.into_any(),
                    Some(Ok(r)) if r.is_empty() => view! {
                        <p class="p-4 text-muted">"Nothing outstanding. Press n to note something you're waiting on."</p>
                    }.into_any(),
                    Some(Ok(r)) => r.into_iter().enumerate()
                        .map(|(i, row)| view! { <WaitingRowView row=row index=i /> })
                        .collect_view().into_any(),
                }}
            </div>
        </div>
    }
}

fn chip(text: String) -> impl IntoView {
    view! {
        <span class="ml-2 rounded-sm border border-line px-1 text-[10px] uppercase tracking-wide text-muted">{text}</span>
    }
}

#[component]
fn WaitingRowView(row: WaitingOnRow, index: usize) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let w = row.waiting;
    let id = w.id;
    let node = NodeRef::new(NodeType::WaitingOn, id);
    let resolved = w.resolved_on.is_some();

    let resolve = move |_| {
        spawn_local(async move {
            finish(api::resolve_waiting_on(id).await, toasts, version);
        });
    };
    let reopen = move |_| {
        spawn_local(async move {
            finish(api::reopen_waiting_on(id).await, toasts, version);
        });
    };
    let wake = move |_| {
        spawn_local(async move {
            finish(api::snooze_waiting_on(id, None).await, toasts, version);
        });
    };
    let snooze = move |v: String| {
        if let Ok(days) = v.parse::<u32>() {
            spawn_local(async move {
                finish(
                    api::snooze_waiting_on(id, Some(days)).await,
                    toasts,
                    version,
                );
            });
        }
    };

    let age_class = if row.stale {
        "tabular-nums font-medium"
    } else {
        "tabular-nums text-muted"
    };
    let title_class = if resolved {
        "truncate text-muted line-through"
    } else if row.stale {
        "truncate font-medium"
    } else {
        "truncate"
    };
    let about = row
        .about
        .map(|a| format!("{} · {}", type_label(a.node.node_type), a.label))
        .unwrap_or_default();
    let expected_class = if row.overdue {
        "tabular-nums font-medium"
    } else {
        "tabular-nums text-muted"
    };
    let status_chip = if resolved {
        w.resolved_on.map(|d| format!("resolved {d}"))
    } else if row.snoozed {
        w.follow_up_on.map(|d| format!("until {d}"))
    } else if row.stale {
        Some("stale".to_owned())
    } else {
        None
    };

    view! {
        <NodeRow node=node index=index>
            <div class=COLS>
                <span class=age_class>{age_text(row.age_days)}</span>
                <span class=title_class>{w.description}{status_chip.map(chip)}</span>
                <span class="truncate text-muted">{row.person.label}</span>
                <span class="truncate text-muted">{about}</span>
                <span class=expected_class>{w.expected_by.map(|d| d.to_string()).unwrap_or_default()}</span>
                <span class="flex items-center justify-end gap-1" on:click=|ev| ev.stop_propagation()>
                    {if resolved {
                        view! { <button class=BUTTON on:click=reopen>"Reopen"</button> }.into_any()
                    } else {
                        view! {
                            <button class=BUTTON on:click=resolve>"Resolve"</button>
                            {if row.snoozed {
                                view! { <button class=BUTTON on:click=wake>"Wake"</button> }.into_any()
                            } else {
                                view! { <SelectField compact=true options=snooze_options() current=String::new() on_change=snooze /> }.into_any()
                            }}
                        }.into_any()
                    }}
                </span>
            </div>
        </NodeRow>
    }
}
