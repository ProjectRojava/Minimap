use leptos::prelude::*;
use minimap_types::{Activity, ActivityAction, EdgeLink, EdgeType, NodeRef, NodeType};

use crate::{
    api,
    components::{
        objective_panel::ObjectivePanel, people_panel::PersonPanel, team_panel::TeamPanel,
    },
    nav::type_label,
    state::{DataVersion, Selection, Toasts},
};

/// Right-hand panel for the selected node. Split view on wide windows, overlay on narrow ones.
#[component]
pub fn DetailPane() -> impl IntoView {
    let selection = expect_context::<Selection>();
    move || {
        selection.0.get().map(|node| {
            view! {
                // Scrim only below the split-view breakpoint.
                <div class="fixed inset-x-0 top-8 bottom-0 z-20 bg-scrim min-[1100px]:hidden"
                     on:click=move |_| selection.close()></div>
                <aside
                    class="fixed top-8 bottom-0 right-0 z-30 w-[420px] max-w-full \
                           min-[1100px]:static min-[1100px]:max-w-none \
                           flex flex-col shrink-0 overflow-y-auto border-l border-line \
                           bg-panel"
                    aria-label="Details"
                >
                    <PaneBody node=node />
                </aside>
            }
        })
    }
}

#[component]
fn PaneBody(node: NodeRef) -> impl IntoView {
    let selection = expect_context::<Selection>();
    let toasts = expect_context::<Toasts>();

    let version = expect_context::<DataVersion>();
    let summary = LocalResource::new(move || {
        version.track();
        api::get_node_summary(node)
    });
    let links = LocalResource::new(move || {
        version.track();
        api::list_edges_for(node.id)
    });
    let history = LocalResource::new(move || {
        version.track();
        api::list_activity_for(node.id)
    });

    // Surface backend errors as toasts as well as inline.
    Effect::new(move |_| {
        for e in [
            summary.get().and_then(|r| r.err()),
            links.get().and_then(|r| r.err()),
            history.get().and_then(|r| r.err()),
        ]
        .into_iter()
        .flatten()
        {
            toasts.error(&e);
        }
    });

    view! {
        <header class="flex items-start gap-2 px-4 py-3 border-b border-line">
            <div class="flex-1 min-w-0">
                <p class="text-[11px] uppercase tracking-wide text-muted">{type_label(node.node_type)}</p>
                <h2 class="text-base font-semibold break-words">
                    {move || match summary.get() {
                        Some(Ok(s)) => {
                            let archived = s.archived;
                            view! {
                                {s.label}
                                {archived.then(|| view! { <span class="ml-2 text-[11px] font-normal text-muted">"archived"</span> })}
                            }.into_any()
                        }
                        Some(Err(_)) => "Unavailable".into_any(),
                        None => "…".into_any(),
                    }}
                </h2>
            </div>
            <button class="rounded px-1.5 text-muted hover:bg-hover"
                    aria-label="Close details (Esc)" title="Close (Esc)"
                    on:click=move |_| selection.close()>"✕"</button>
        </header>

        {match node.node_type {
            NodeType::Person => view! { <PersonPanel id=node.id /> }.into_any(),
            NodeType::Team => view! { <TeamPanel id=node.id /> }.into_any(),
            NodeType::Objective => view! { <ObjectivePanel id=node.id /> }.into_any(),
            other => view! {
                <Section title="Fields">
                    <p class="text-muted">
                        "Editable fields appear here once the " {type_label(other).to_lowercase()} " screens land."
                    </p>
                </Section>
            }.into_any(),
        }}

        <Section title="Links">
            {move || match links.get() {
                None => view! { <p class="text-muted">"Loading…"</p> }.into_any(),
                Some(Err(e)) => view! { <p class="text-danger">{e.message}</p> }.into_any(),
                Some(Ok(l)) => {
                    let l: Vec<EdgeLink> = l.into_iter().filter(|x| !edited_elsewhere(node.node_type, x)).collect();
                    if l.is_empty() {
                        view! { <p class="text-muted">"No other links."</p> }.into_any()
                    } else {
                        view! { <LinkGroups links=l /> }.into_any()
                    }
                }
            }}
        </Section>

        <Section title="Activity">
            {move || match history.get() {
                None => view! { <p class="text-muted">"Loading…"</p> }.into_any(),
                Some(Err(e)) => view! { <p class="text-danger">{e.message}</p> }.into_any(),
                Some(Ok(h)) if h.is_empty() => view! { <p class="text-muted">"No activity."</p> }.into_any(),
                Some(Ok(h)) => view! {
                    <ul class="space-y-2">
                        {h.into_iter().map(|a| view! { <ActivityRow activity=a /> }).collect_view()}
                    </ul>
                }.into_any(),
            }}
        </Section>
    }
}

#[component]
pub(crate) fn Section(title: &'static str, children: Children) -> impl IntoView {
    view! {
        <section class="px-4 py-3 border-b border-line text-[13px]">
            <h3 class="mb-2 text-[11px] font-semibold uppercase tracking-wide text-muted">{title}</h3>
            {children()}
        </section>
    }
}

/// Links the node's own panel already shows and edits (teams and manager for a person,
/// members for a team, contributors for an objective), so the generic list doesn't repeat them.
pub fn edited_elsewhere(node_type: NodeType, link: &EdgeLink) -> bool {
    match node_type {
        NodeType::Person => {
            link.outgoing
                && matches!(
                    link.edge.edge_type,
                    EdgeType::MemberOf | EdgeType::ReportsTo
                )
        }
        NodeType::Team => !link.outgoing && link.edge.edge_type == EdgeType::MemberOf,
        NodeType::Objective => !link.outgoing && link.edge.edge_type == EdgeType::ContributesTo,
        _ => false,
    }
}

/// Group heading, e.g. "Blocks" / "Blocked by". Pure, so it is unit-tested.
pub fn link_heading(edge_type: EdgeType, outgoing: bool) -> &'static str {
    use EdgeType::*;
    match (edge_type, outgoing) {
        (Blocks, true) => "Blocks",
        (Blocks, false) => "Blocked by",
        (DependsOn, true) => "Depends on",
        (DependsOn, false) => "Needed by",
        (ContributesTo, true) => "Contributes to",
        (ContributesTo, false) => "Contributions",
        (AssignedTo, true) => "Assigned to",
        (AssignedTo, false) => "Assigned tasks",
        (MemberOf, true) => "Member of",
        (MemberOf, false) => "Members",
        (ReportsTo, true) => "Reports to",
        (ReportsTo, false) => "Direct reports",
        (RelatesTo, _) => "Related",
        (Mentions, true) => "Mentions",
        (Mentions, false) => "Mentioned in",
        (Affects, true) => "Affects",
        (Affects, false) => "Affected by",
        (About, true) => "About",
        (About, false) => "Waiting-ons",
    }
}

/// Edges grouped under their heading, keeping first-seen order.
pub fn group_links(links: Vec<EdgeLink>) -> Vec<(&'static str, Vec<EdgeLink>)> {
    let mut groups: Vec<(&'static str, Vec<EdgeLink>)> = Vec::new();
    for l in links {
        let heading = link_heading(l.edge.edge_type, l.outgoing);
        match groups.iter_mut().find(|(h, _)| *h == heading) {
            Some((_, v)) => v.push(l),
            None => groups.push((heading, vec![l])),
        }
    }
    groups
}

#[component]
fn LinkGroups(links: Vec<EdgeLink>) -> impl IntoView {
    let selection = expect_context::<Selection>();
    view! {
        <div class="space-y-3">
            {group_links(links).into_iter().map(|(heading, items)| view! {
                <div>
                    <p class="mb-1 text-muted">{heading}</p>
                    <ul class="space-y-px">
                        {items.into_iter().map(|l| {
                            let node = l.other.node;
                            view! {
                                <li>
                                    <button class="w-full text-left rounded px-1.5 py-0.5 hover:bg-hover"
                                            on:click=move |_| selection.open(node)>
                                        <span class="text-faint mr-1.5">{type_label(node.node_type)}</span>
                                        {l.other.label}
                                    </button>
                                </li>
                            }
                        }).collect_view()}
                    </ul>
                </div>
            }).collect_view()}
        </div>
    }
}

/// "field: old → new" lines for an activity row. Pure, so it is unit-tested.
pub fn describe(a: &Activity) -> Vec<String> {
    let show = |v: &serde_json::Value| match v {
        serde_json::Value::Null => "∅".to_owned(),
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    let Some(map) = a.diff.as_object() else {
        return Vec::new();
    };
    match a.action {
        ActivityAction::Created => map
            .iter()
            .filter(|(_, v)| !v[1].is_null() && v[1] != "")
            .map(|(k, v)| format!("{k}: {}", show(&v[1])))
            .collect(),
        ActivityAction::EdgeAdded | ActivityAction::EdgeRemoved => {
            let side = usize::from(a.action == ActivityAction::EdgeAdded);
            let desc = &map["edge"][side];
            vec![format!(
                "{} → {}",
                desc["edge_type"].as_str().unwrap_or("?"),
                desc["to_type"].as_str().unwrap_or("?")
            )]
        }
        _ => map
            .iter()
            .map(|(k, v)| format!("{k}: {} → {}", show(&v[0]), show(&v[1])))
            .collect(),
    }
}

fn action_label(a: ActivityAction) -> &'static str {
    match a {
        ActivityAction::Created => "Created",
        ActivityAction::Updated => "Updated",
        ActivityAction::Archived => "Archived",
        ActivityAction::Unarchived => "Restored",
        ActivityAction::Deleted => "Deleted",
        ActivityAction::EdgeAdded => "Link added",
        ActivityAction::EdgeRemoved => "Link removed",
    }
}

#[component]
fn ActivityRow(activity: Activity) -> impl IntoView {
    let when = minimap_types::timefmt::fmt_ts(activity.at)
        .replace('T', " ")
        .chars()
        .take(16)
        .collect::<String>();
    let lines = describe(&activity);
    view! {
        <li>
            <p>
                <span class="font-medium">{action_label(activity.action)}</span>
                <span class="ml-2 text-[11px] text-muted">{when} " UTC"</span>
            </p>
            {lines.into_iter().map(|l| view! { <p class="text-muted break-words">{l}</p> }).collect_view()}
        </li>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{NodeType, Uuid};
    use serde_json::json;

    fn act(action: ActivityAction, diff: serde_json::Value) -> Activity {
        Activity {
            id: Uuid::nil(),
            at: time::OffsetDateTime::UNIX_EPOCH,
            node_type: NodeType::Task,
            node_id: Uuid::nil(),
            action,
            diff,
        }
    }

    #[test]
    fn headings_cover_both_directions() {
        assert_eq!(link_heading(EdgeType::Blocks, true), "Blocks");
        assert_eq!(link_heading(EdgeType::Blocks, false), "Blocked by");
        for &t in EdgeType::ALL {
            assert_ne!(link_heading(t, true), "");
            assert_ne!(link_heading(t, false), "");
        }
    }

    #[test]
    fn describes_updates_and_creates() {
        let u = act(
            ActivityAction::Updated,
            json!({"title": ["a", "b"], "due_date": [null, "2027-01-01"]}),
        );
        let mut lines = describe(&u);
        lines.sort();
        assert_eq!(lines, vec!["due_date: ∅ → 2027-01-01", "title: a → b"]);

        let c = act(
            ActivityAction::Created,
            json!({"title": [null, "x"], "description": [null, ""], "due_date": [null, null]}),
        );
        assert_eq!(describe(&c), vec!["title: x"]);
    }

    fn link(edge_type: EdgeType, outgoing: bool) -> EdgeLink {
        use minimap_types::{Edge, NodeRef, NodeSummary};
        let id = Uuid::nil();
        EdgeLink {
            edge: Edge {
                id,
                edge_type,
                from_type: NodeType::Person,
                from_id: id,
                to_type: NodeType::Team,
                to_id: id,
                attrs: json!({}),
                created_at: time::OffsetDateTime::UNIX_EPOCH,
                archived_at: None,
            },
            outgoing,
            other: NodeSummary {
                node: NodeRef::new(NodeType::Team, id),
                label: String::new(),
                archived: false,
            },
        }
    }

    #[test]
    fn panels_hide_the_links_they_already_edit() {
        // A person's own teams and manager are edited in the panel...
        assert!(edited_elsewhere(
            NodeType::Person,
            &link(EdgeType::MemberOf, true)
        ));
        assert!(edited_elsewhere(
            NodeType::Person,
            &link(EdgeType::ReportsTo, true)
        ));
        // ...but reports, assigned tasks and mentions still show in the list.
        assert!(!edited_elsewhere(
            NodeType::Person,
            &link(EdgeType::ReportsTo, false)
        ));
        assert!(!edited_elsewhere(
            NodeType::Person,
            &link(EdgeType::AssignedTo, false)
        ));
        // A team's members are listed by its panel.
        assert!(edited_elsewhere(
            NodeType::Team,
            &link(EdgeType::MemberOf, false)
        ));
        assert!(!edited_elsewhere(
            NodeType::Task,
            &link(EdgeType::MemberOf, false)
        ));
    }

    #[test]
    fn describes_edges() {
        let d = json!({"edge_type": "blocks", "to_type": "task"});
        assert_eq!(
            describe(&act(
                ActivityAction::EdgeAdded,
                json!({"edge": [null, d.clone()]})
            )),
            vec!["blocks → task"]
        );
        assert_eq!(
            describe(&act(
                ActivityAction::EdgeRemoved,
                json!({"edge": [d, null]})
            )),
            vec!["blocks → task"]
        );
    }
}
