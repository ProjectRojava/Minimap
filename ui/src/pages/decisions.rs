use std::str::FromStr;

use leptos::{prelude::*, task::spawn_local};
use minimap_types::{
    CreateDecision, DecisionFilter, DecisionRow, DecisionStatus, NodeRef, NodeType,
};

use crate::{
    api,
    components::{
        decision_panel::status_options,
        form::{SelectField, BUTTON_SOFT, COMPACT_INPUT},
        node_row::NodeRow,
        page::{column_head, EmptyState, Hints, PageHeader, FILTER_BAR},
    },
    labels::{decision_status_label, decision_status_tone},
    state::{finish, DataVersion, ListNav, Selection, Toasts},
};

const COLS: &str = "grid w-full items-center gap-3 grid-cols-[6.5rem_5.5rem_minmax(0,1fr)_12rem]";

/// What was decided, why, and what it touches. Newest first.
#[component]
pub fn Decisions() -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let list = expect_context::<ListNav>();
    let selection = expect_context::<Selection>();

    let text = RwSignal::new(String::new());
    let status = RwSignal::new(String::new());

    let rows = LocalResource::new(move || {
        version.track();
        let t = text.get();
        api::list_decisions(DecisionFilter {
            status: DecisionStatus::from_str(&status.get()).ok(),
            text: (!t.trim().is_empty()).then_some(t),
            ..Default::default()
        })
    });
    Effect::new(move |_| match rows.get() {
        Some(Ok(r)) => list.set_items(
            r.iter()
                .map(|d| NodeRef::new(NodeType::Decision, d.id))
                .collect(),
        ),
        Some(Err(e)) => toasts.error(&e),
        None => {}
    });

    // Created at once (like notes) so links can be added straight away, then opened to fill in.
    let create = move || {
        spawn_local(async move {
            let input = CreateDecision {
                title: "Untitled decision".into(),
                context: String::new(),
                decision: String::new(),
                rationale: String::new(),
                decided_on: None,
                status: None,
            };
            if let Some(d) = finish(api::create_decision(input).await, toasts, version) {
                selection.open(NodeRef::new(NodeType::Decision, d.id));
            }
        });
    };
    list.on_new(create);

    let status_filter: Vec<(String, String)> =
        std::iter::once((String::new(), "Any status".to_owned()))
            .chain(status_options())
            .collect();

    view! {
        <div class="flex flex-col h-full">
            <PageHeader icon="decisions" title="Decisions" subtitle="What was decided, why, and what it touches">
                <button class=BUTTON_SOFT on:click=move |_| create()>"New decision"</button>
                <Hints keys=&[("n", "new"), ("j/k", "move"), ("Enter", "open")] />
            </PageHeader>
            <div class=FILTER_BAR>
                <input class=format!("{COMPACT_INPUT} w-44") type="search" placeholder="Search decisions"
                       prop:value=move || text.get() on:input=move |ev| text.set(event_target_value(&ev)) />
                <SelectField compact=true options=status_filter current=String::new()
                             on_change=move |v: String| status.set(v) />
            </div>
            <div class=column_head(COLS)>
                <span>"Decided"</span><span>"Status"</span><span>"Decision"</span><span>"Links"</span>
            </div>
            <div class="flex-1 overflow-y-auto" role="table">
                {move || match rows.get() {
                    None => view! { <p class="p-4 text-muted">"Loading…"</p> }.into_any(),
                    Some(Err(_)) => view! { <p class="p-4 text-muted">"Couldn't load decisions."</p> }.into_any(),
                    Some(Ok(r)) if r.is_empty() => view! {
                        <EmptyState icon="decisions" title="No decisions match"
                            hint="Press n to record one, or clear the filters." />
                    }.into_any(),
                    Some(Ok(r)) => r.into_iter().enumerate()
                        .map(|(i, row)| view! { <DecisionRowView row=row index=i /> })
                        .collect_view().into_any(),
                }}
            </div>
        </div>
    }
}

/// "Affects API, Billing", plus "replaced by X" for a superseded decision.
pub fn links_text(row: &DecisionRow) -> String {
    let mut parts = Vec::new();
    if let Some(next) = &row.superseded_by {
        parts.push(format!("replaced by {}", next.label));
    }
    if !row.affects.is_empty() {
        let names: Vec<&str> = row.affects.iter().map(|a| a.label.as_str()).collect();
        parts.push(format!("affects {}", names.join(", ")));
    }
    parts.join(" · ")
}

#[component]
fn DecisionRowView(row: DecisionRow, index: usize) -> impl IntoView {
    let node = NodeRef::new(NodeType::Decision, row.id);
    let links = links_text(&row);
    let dim = row.status == DecisionStatus::Superseded;
    let when = row
        .decided_on
        .map(|d| d.to_string())
        .unwrap_or_else(|| "—".to_owned());
    view! {
        <NodeRow node=node index=index>
            <div class=COLS>
                <span class="tabular-nums text-muted">{when}</span>
                <span><span class=decision_status_tone(row.status).chip()>{decision_status_label(row.status)}</span></span>
                <span class=if dim { "truncate text-muted line-through decoration-faint" } else { "truncate" }>
                    <span class="font-medium">{row.title}</span>
                    <span class="ml-2 text-muted">{row.excerpt}</span>
                </span>
                <span class="truncate text-muted">{links}</span>
            </div>
        </NodeRow>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{NodeSummary, Uuid};

    fn summary(label: &str) -> NodeSummary {
        NodeSummary {
            node: NodeRef::new(NodeType::Project, Uuid::nil()),
            label: label.into(),
            archived: false,
        }
    }

    fn row() -> DecisionRow {
        DecisionRow {
            id: Uuid::nil(),
            title: "t".into(),
            status: DecisionStatus::Decided,
            decided_on: None,
            excerpt: String::new(),
            affects: vec![],
            superseded_by: None,
        }
    }

    #[test]
    fn link_text_names_what_it_affects_and_what_replaced_it() {
        assert_eq!(links_text(&row()), "");
        let mut r = row();
        r.affects = vec![summary("API"), summary("Billing")];
        assert_eq!(links_text(&r), "affects API, Billing");
        r.superseded_by = Some(summary("New plan"));
        assert_eq!(
            links_text(&r),
            "replaced by New plan · affects API, Billing"
        );
    }
}
