//! The generic "Links" section of the detail pane: see a node's links grouped by relation,
//! edit their attributes, remove them, and add new ones (relation, then a node search).
//! Links a node's own panel already edits (teams, assignee, ...) are left to that panel.

use leptos::{prelude::*, task::spawn_local};
use minimap_types::{
    AppError, AttrKind, AttrSpec, EdgeLink, EdgeType, LinkOption, NewEdge, NodeRef, NodeSummary,
    Uuid,
};
use serde_json::{json, Value};

use crate::{
    api,
    components::{
        detail_pane::{group_links, kind_edited_elsewhere, link_heading},
        form::{SelectField, BUTTON, COMPACT_INPUT, INPUT},
        people_panel::error_line,
    },
    nav::type_label,
    state::{finish, DataVersion, Selection, Toasts},
};

const MAX_RESULTS: usize = 30;

#[component]
pub fn LinksEditor(
    node: NodeRef,
    links: Vec<EdgeLink>,
    /// Used inside a panel that already shows the node's main links (a task's): no "No links
    /// yet" line, and the button reads "Link to something else…".
    #[prop(optional)]
    compact: bool,
    /// Extra buttons shown before the "add a link" button.
    #[prop(optional, into)]
    actions: ViewFn,
) -> impl IntoView {
    // The relations available from this node type don't change while the pane is open.
    let options = LocalResource::new(move || api::list_link_options(node.node_type));
    view! {
        {move || match options.get() {
            None => view! { <p class="text-muted">"Loading…"</p> }.into_any(),
            Some(Err(e)) => error_line(e),
            Some(Ok(opts)) => view! { <Editor node=node links=links.clone() options=opts compact=compact actions=actions.clone() /> }.into_any(),
        }}
    }
}

/// Turns what the user typed into an attribute value. Empty means "unset" (`None`).
pub fn attr_value(spec: &AttrSpec, text: &str) -> Result<Option<Value>, String> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(None);
    }
    let wrong = || format!("{} must be {}", spec.label, spec.hint);
    match &spec.kind {
        AttrKind::WholeNumber { min, max } => {
            let n: u64 = t.parse().map_err(|_| wrong())?;
            if n < u64::from(*min) || max.is_some_and(|m| n > u64::from(m)) {
                return Err(wrong());
            }
            Ok(Some(json!(n)))
        }
        AttrKind::Number { min, max } => {
            let n: f64 = t.replace(',', ".").parse().map_err(|_| wrong())?;
            if !(*min..=*max).contains(&n) {
                return Err(wrong());
            }
            Ok(Some(json!(n)))
        }
        AttrKind::Choice { options } => {
            if options.iter().any(|o| o == t) {
                Ok(Some(json!(t)))
            } else {
                Err(wrong())
            }
        }
        AttrKind::Text => Ok(Some(json!(t))),
    }
}

/// `attrs` with `key` set to `value`, or removed when `value` is `None`.
fn with_attr(attrs: &Value, key: &str, value: Option<Value>) -> Value {
    let mut map = attrs.as_object().cloned().unwrap_or_default();
    match value {
        Some(v) => map.insert(key.to_owned(), v),
        None => map.remove(key),
    };
    Value::Object(map)
}

/// The label of a relation as it reads from the node: "Blocks", "Blocked by", "Related".
fn relation_label(option: &LinkOption) -> &'static str {
    link_heading(option.edge_type, option.outgoing)
}

/// The links this list shows for `node`: all but the ones its own panel edits (a task's assignee
/// and blocks, a person's teams, ...) and a task's "related" links to other tasks, which sit in
/// its panel's Links section.
pub fn shown_links(node: NodeRef, links: &[EdgeLink]) -> Vec<EdgeLink> {
    links
        .iter()
        .filter(|l| !kind_edited_elsewhere(node.node_type, l.edge.edge_type, l.outgoing))
        .filter(|l| {
            !(node.node_type == minimap_types::NodeType::Task
                && l.edge.edge_type == EdgeType::RelatesTo
                && l.other.node.node_type == minimap_types::NodeType::Task)
        })
        .cloned()
        .collect()
}

#[component]
fn Editor(
    node: NodeRef,
    links: Vec<EdgeLink>,
    options: Vec<LinkOption>,
    compact: bool,
    actions: ViewFn,
) -> impl IntoView {
    let selection = expect_context::<Selection>();
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();

    let specs = |edge_type: EdgeType| -> Vec<AttrSpec> {
        options
            .iter()
            .find(|o| o.edge_type == edge_type)
            .map(|o| o.attrs.clone())
            .unwrap_or_default()
    };

    // Relations the node's own panel already handles are not shown or offered here.
    let shown = shown_links(node, &links);
    let addable: Vec<LinkOption> = options
        .iter()
        .filter(|o| !kind_edited_elsewhere(node.node_type, o.edge_type, o.outgoing))
        .cloned()
        .collect();

    let groups = group_links(shown.clone())
        .into_iter()
        .map(|(heading, items)| {
            let rows = items
                .into_iter()
                .map(|l| {
                    let edge_id = l.edge.id;
                    let other = l.other.node;
                    let fields = specs(l.edge.edge_type);
                    let remove = move |_| {
                        spawn_local(async move {
                            finish(api::remove_edge(edge_id).await, toasts, version);
                        });
                    };
                    view! {
                        <li class="flex items-center gap-2">
                            <span class="w-16 shrink-0 text-faint">{type_label(other.node_type)}</span>
                            <button class="min-w-0 flex-1 truncate text-left hover:underline"
                                    on:click=move |_| selection.open(other)>{l.other.label}</button>
                            <AttrFields edge_id=edge_id attrs=l.edge.attrs specs=fields />
                            <button class="px-1 text-faint hover:text-danger" aria-label="Remove link"
                                    on:click=remove>"✕"</button>
                        </li>
                    }
                })
                .collect_view();
            view! {
                <div>
                    <p class="mb-1 text-[11px] text-muted">{heading}</p>
                    <ul class="space-y-1">{rows}</ul>
                </div>
            }
        })
        .collect_view();

    view! {
        <div class="space-y-3">
            {if shown.is_empty() {
                (!compact).then(|| view! { <p class="text-muted">"No links yet."</p> }).into_any()
            } else {
                view! { <div class="space-y-3">{groups}</div> }.into_any()
            }}
            <AddLink node=node options=addable links=shown compact=compact actions=actions />
        </div>
    }
}

/// Inputs for one link's attributes (lag, weight, role, note, ...). Each saves on change.
#[component]
fn AttrFields(edge_id: Uuid, attrs: Value, specs: Vec<AttrSpec>) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();

    specs
        .into_iter()
        .map(|spec| {
            let current = attrs.get(&spec.key).cloned();
            let shown = match &current {
                Some(Value::String(s)) => s.clone(),
                Some(v) => v.to_string(),
                None => String::new(),
            };
            let (base, field) = (attrs.clone(), spec.clone());
            let save = move |text: String| match attr_value(&field, &text) {
                Ok(v) => {
                    let next = with_attr(&base, &field.key, v);
                    spawn_local(async move {
                        finish(api::update_edge_attrs(edge_id, next).await, toasts, version);
                    });
                }
                Err(msg) => {
                    toasts.error(&AppError { code: "invalid".into(), message: msg });
                    version.bump(); // snap the box back to the stored value
                }
            };
            let title = spec.label.clone();
            match spec.kind {
                AttrKind::Choice { options } => {
                    let choices: Vec<(String, String)> = std::iter::once((String::new(), "—".to_owned()))
                        .chain(options.into_iter().map(|o| (o.clone(), o)))
                        .collect();
                    view! { <SelectField compact=true options=choices current=shown on_change=save /> }.into_any()
                }
                AttrKind::Text => view! {
                    <input class=format!("{COMPACT_INPUT} w-28") type="text" placeholder=title.clone()
                           title=title prop:value=shown on:change=move |ev| save(event_target_value(&ev)) />
                }.into_any(),
                AttrKind::WholeNumber { .. } | AttrKind::Number { .. } => view! {
                    <input class=format!("{COMPACT_INPUT} w-16 text-right") type="number" step="any"
                           placeholder=title.clone() title=title prop:value=shown
                           on:change=move |ev| save(event_target_value(&ev)) />
                }.into_any(),
            }
        })
        .collect_view()
}

/// "Add a link": pick the relation, then search for the node on the other end.
#[component]
fn AddLink(
    node: NodeRef,
    options: Vec<LinkOption>,
    links: Vec<EdgeLink>,
    compact: bool,
    actions: ViewFn,
) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();

    let open = RwSignal::new(false);
    let relation = RwSignal::new(None::<usize>);
    let query = RwSignal::new(String::new());

    let labels: Vec<(usize, &'static str)> = options
        .iter()
        .enumerate()
        .map(|(i, o)| (i, relation_label(o)))
        .collect();
    let options = StoredValue::new(options);
    let links = StoredValue::new(links);

    // Candidates for the chosen relation: every node of an allowed type, loaded once per choice.
    let candidates = LocalResource::new(move || {
        let wanted = relation
            .get()
            .and_then(|i| options.with_value(|o| o.get(i).map(|o| o.others.clone())));
        async move {
            let mut all: Vec<NodeSummary> = Vec::new();
            for t in wanted.unwrap_or_default() {
                all.extend(api::list_node_summaries(t).await?);
            }
            Ok::<_, AppError>(all)
        }
    });

    let pick = move |other: NodeRef| {
        let Some(option) = relation
            .get_untracked()
            .and_then(|i| options.with_value(|o| o.get(i).cloned()))
        else {
            return;
        };
        let (from, to) = if option.outgoing {
            (node, other)
        } else {
            (other, node)
        };
        let new = NewEdge {
            edge_type: option.edge_type,
            from,
            to,
            attrs: json!({}),
        };
        spawn_local(async move {
            // Loops, duplicates and wrong pairings come back as a toast explaining why.
            if finish(api::add_edge(new).await, toasts, version).is_some() {
                relation.set(None);
                query.set(String::new());
                open.set(false);
            }
        });
    };

    // Not offered: the node itself and nodes already linked by this relation.
    let results = move || -> Vec<NodeSummary> {
        let Some(option) = relation
            .get()
            .and_then(|i| options.with_value(|o| o.get(i).cloned()))
        else {
            return Vec::new();
        };
        let linked: std::collections::HashSet<Uuid> = links.with_value(|ls| {
            ls.iter()
                .filter(|l| {
                    l.edge.edge_type == option.edge_type
                        && (option.edge_type == EdgeType::RelatesTo
                            || l.outgoing == option.outgoing)
                })
                .map(|l| l.other.node.id)
                .collect()
        });
        let q = query.get().to_lowercase();
        match candidates.get() {
            Some(Ok(all)) => all
                .into_iter()
                .filter(|c| c.node.id != node.id && !linked.contains(&c.node.id))
                .filter(|c| {
                    q.split_whitespace()
                        .all(|w| c.label.to_lowercase().contains(w))
                })
                .take(MAX_RESULTS)
                .collect(),
            _ => Vec::new(),
        }
    };

    view! {
        {move || if !open.get() {
            let label = if compact { "Link to something else…" } else { "Add a link…" };
            view! {
                <div class="flex flex-wrap gap-2">
                    {actions.run()}
                    <button class=BUTTON on:click=move |_| open.set(true)>{label}</button>
                </div>
            }.into_any()
        } else {
            let relation_choices: Vec<(String, String)> = std::iter::once((String::new(), "Choose a relation…".to_owned()))
                .chain(labels.iter().map(|(i, label)| (i.to_string(), (*label).to_owned())))
                .collect();
            view! {
                <div class="space-y-2 rounded-sm border border-line p-2">
                    <div class="flex items-center gap-2">
                        <SelectField compact=true options=relation_choices.clone() current=String::new()
                            on_change=move |v: String| {
                                relation.set(v.parse::<usize>().ok());
                                query.set(String::new());
                            } />
                        <button class=format!("{BUTTON} ml-auto")
                                on:click=move |_| { open.set(false); relation.set(None); query.set(String::new()); }>
                            "Cancel"
                        </button>
                    </div>
                    <Show when=move || relation.get().is_some()>
                        <input class=INPUT type="search" placeholder="Search by name" autofocus
                               prop:value=move || query.get() on:input=move |ev| query.set(event_target_value(&ev)) />
                        {move || match candidates.get() {
                            None => view! { <p class="text-[11px] text-muted">"Loading…"</p> }.into_any(),
                            Some(Err(e)) => error_line(e),
                            Some(Ok(_)) => {
                                let found = results();
                                if found.is_empty() {
                                    view! { <p class="text-[11px] text-muted">"Nothing to link to."</p> }.into_any()
                                } else {
                                    view! {
                                        <ul class="max-h-48 overflow-y-auto">
                                            {found.into_iter().map(|c| {
                                                let other = c.node;
                                                view! {
                                                    <li>
                                                        <button class="flex w-full gap-2 rounded px-1.5 py-0.5 text-left hover:bg-hover"
                                                                on:click=move |_| pick(other)>
                                                            <span class="w-16 shrink-0 text-faint">{type_label(other.node_type)}</span>
                                                            <span class="truncate">{c.label}</span>
                                                        </button>
                                                    </li>
                                                }
                                            }).collect_view()}
                                        </ul>
                                    }.into_any()
                                }
                            }
                        }}
                    </Show>
                </div>
            }.into_any()
        }}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::NodeType;

    fn spec(kind: AttrKind) -> AttrSpec {
        AttrSpec {
            key: "k".into(),
            label: "Thing".into(),
            kind,
            hint: "valid".into(),
        }
    }

    #[test]
    fn typed_text_becomes_attribute_values() {
        let whole = spec(AttrKind::WholeNumber {
            min: 1,
            max: Some(100),
        });
        assert_eq!(attr_value(&whole, " 50 "), Ok(Some(json!(50))));
        assert_eq!(attr_value(&whole, ""), Ok(None), "empty unsets");
        for bad in ["0", "101", "1.5", "-3", "abc"] {
            assert_eq!(
                attr_value(&whole, bad),
                Err("Thing must be valid".into()),
                "{bad}"
            );
        }
        let open_ended = spec(AttrKind::WholeNumber { min: 0, max: None });
        assert_eq!(attr_value(&open_ended, "0"), Ok(Some(json!(0))));
        assert_eq!(attr_value(&open_ended, "9999"), Ok(Some(json!(9999))));

        let weight = spec(AttrKind::Number { min: 0.0, max: 1.0 });
        assert_eq!(attr_value(&weight, "0.5"), Ok(Some(json!(0.5))));
        assert_eq!(attr_value(&weight, "0,25"), Ok(Some(json!(0.25))));
        assert!(attr_value(&weight, "1.5").is_err() && attr_value(&weight, "-0.1").is_err());

        let role = spec(AttrKind::Choice {
            options: vec!["lead".into(), "member".into()],
        });
        assert_eq!(attr_value(&role, "lead"), Ok(Some(json!("lead"))));
        assert!(attr_value(&role, "boss").is_err());
        assert_eq!(
            attr_value(&spec(AttrKind::Text), "  same customer "),
            Ok(Some(json!("same customer")))
        );
    }

    #[test]
    fn attributes_are_set_and_removed_without_touching_others() {
        let base = json!({"weight": 0.5, "note": "x"});
        assert_eq!(
            with_attr(&base, "weight", Some(json!(1.0))),
            json!({"weight": 1.0, "note": "x"})
        );
        assert_eq!(with_attr(&base, "note", None), json!({"weight": 0.5}));
        assert_eq!(
            with_attr(&json!({}), "role", Some(json!("lead"))),
            json!({"role": "lead"})
        );
        assert_eq!(with_attr(&Value::Null, "role", None), json!({}));
        // Unset attributes stay unset.
        assert_eq!(with_attr(&json!({}), "role", None), json!({}));
    }

    #[test]
    fn relations_read_naturally_from_the_node() {
        let opt = |e, out| LinkOption {
            edge_type: e,
            outgoing: out,
            others: vec![NodeType::Task],
            attrs: vec![],
        };
        assert_eq!(relation_label(&opt(EdgeType::Blocks, true)), "Blocks");
        assert_eq!(relation_label(&opt(EdgeType::Blocks, false)), "Blocked by");
        assert_eq!(relation_label(&opt(EdgeType::RelatesTo, true)), "Related");
    }
}
