//! Detail-pane body for an objective: fields, contributing work, archive.

use std::str::FromStr;

use leptos::{prelude::*, task::spawn_local};
use minimap_types::{
    timefmt::parse_date, AppError, Contribution, EdgeType, NewEdge, NodeRef, NodeSummary, NodeType,
    Objective, ObjectiveDetail, ObjectiveStatus, Patch, UpdateObjective, Uuid, DEFAULT_REVIEW_DAYS,
};

use crate::{
    api,
    calendar::format_ymd,
    components::{
        date_field::today_ymd,
        detail_pane::Section,
        form::{SelectField, TextField, BUTTON, BUTTON_DANGER, BUTTON_SOFT, INPUT},
        health_panel::ObjectiveHealthSection,
        item_notes::ItemNotes,
        people_panel::error_line,
        summary_chips::ObjectiveSummary,
    },
    labels::{
        humanize, objective_status_label, priority_option, status_word_tone, OBJECTIVE_STATUS_TINT,
        PRIORITY_TINT,
    },
    nav::type_label,
    state::{finish, DataVersion, Selection, Toasts},
};

#[component]
pub fn ObjectivePanel(id: Uuid) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    // Fields load once per opening so typing is never overwritten by a reload.
    let objective = LocalResource::new(move || api::get_objective(id));
    let detail = LocalResource::new(move || {
        version.track();
        api::get_objective_detail(id)
    });
    // Everything that could contribute: projects and tasks.
    let candidates = LocalResource::new(move || {
        version.track();
        async move {
            let mut all = api::list_node_summaries(NodeType::Project).await?;
            all.extend(api::list_node_summaries(NodeType::Task).await?);
            Ok::<_, AppError>(all)
        }
    });

    view! {
        <Section title="Fields">
            <ObjectiveSummary id=id />
            {move || match objective.get() {
                None => view! { <p class="text-muted">"Loading…"</p> }.into_any(),
                Some(Err(e)) => error_line(e),
                Some(Ok(o)) => view! { <ObjectiveFields objective=o /> }.into_any(),
            }}
        </Section>
        <ObjectiveHealthSection objective=id />
        {move || match (detail.get(), candidates.get()) {
            (Some(Ok(d)), Some(Ok(c))) => view! {
                <Contributions detail=d.clone() candidates=c />
                <ItemNotes node=NodeRef::new(NodeType::Objective, id) />
                <ArchiveObjective detail=d />
            }.into_any(),
            (Some(Err(e)), _) | (_, Some(Err(e))) => view! {
                <Section title="Contributing work">{error_line(e)}</Section>
            }.into_any(),
            _ => view! { <Section title="Contributing work"><p class="text-muted">"Loading…"</p></Section> }.into_any(),
        }}
    }
}

#[component]
fn ObjectiveFields(objective: Objective) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let id = objective.id;
    // What the server last answered with, so the ongoing/review parts follow what was saved.
    let obj = RwSignal::new(objective.clone());
    let save = move |patch: UpdateObjective| {
        spawn_local(async move {
            if let Some(o) = finish(api::update_objective(id, patch).await, toasts, version) {
                obj.set(o);
            }
        });
    };

    let save_date = move |v: String| {
        let v = v.trim().to_owned();
        if v.is_empty() {
            return save(UpdateObjective {
                target_date: Patch::Clear,
                ..Default::default()
            });
        }
        match parse_date(&v) {
            Ok(d) => save(UpdateObjective {
                target_date: Patch::Set(d),
                ..Default::default()
            }),
            Err(_) => {
                toasts.error(&AppError {
                    code: "invalid".into(),
                    message: "Target date must look like 2027-03-31".into(),
                });
                version.bump();
            }
        }
    };
    let save_status = move |v: String| {
        if let Ok(status) = ObjectiveStatus::from_str(&v) {
            save(UpdateObjective {
                status: Some(status),
                ..Default::default()
            });
        }
    };
    let save_priority = move |v: String| {
        if let Ok(p) = v.parse::<u8>() {
            save(UpdateObjective {
                priority: Some(p),
                ..Default::default()
            });
        }
    };
    // Goal or ongoing. Becoming ongoing starts a monthly review unless one is already set.
    let save_kind = move |v: String| {
        let ongoing = v == KIND_ONGOING;
        let rhythm = if ongoing && obj.get_untracked().review_every_days.is_none() {
            Patch::Set(DEFAULT_REVIEW_DAYS)
        } else {
            Patch::Keep
        };
        save(UpdateObjective {
            ongoing: Some(ongoing),
            review_every_days: rhythm,
            ..Default::default()
        });
    };
    let save_rhythm = move |v: String| {
        save(UpdateObjective {
            review_every_days: v.parse::<u32>().map_or(Patch::Clear, Patch::Set),
            ..Default::default()
        });
    };
    let mark_reviewed = move |_| {
        let (y, m, d) = today_ymd();
        if let Ok(today) = parse_date(&format_ymd(y, m, d)) {
            save(UpdateObjective {
                last_reviewed_on: Patch::Set(today),
                ..Default::default()
            });
        }
    };

    let priority_options: Vec<(String, String)> = (1..=5u8)
        .map(|p| (p.to_string(), priority_option(p)))
        .collect();
    let kind_options = vec![
        (KIND_GOAL.to_owned(), "Goal: has an end".to_owned()),
        (KIND_ONGOING.to_owned(), "Ongoing: no end".to_owned()),
    ];
    let kind_now = if objective.ongoing {
        KIND_ONGOING
    } else {
        KIND_GOAL
    };

    view! {
        <TextField label="Title" value=objective.title.clone()
            on_commit=move |v: String| save(UpdateObjective { title: Some(v), ..Default::default() }) />
        <TextField label="Description" multiline=true value=objective.description.clone()
            on_commit=move |v: String| save(UpdateObjective { description: Some(v), ..Default::default() }) />
        <div class="mb-2">
            <SelectField label="Kind" options=kind_options current=kind_now.to_owned() on_change=save_kind />
        </div>
        {move || if obj.with(|o| o.ongoing) {
            let o = obj.get();
            let rhythm_options = rhythm_options(o.review_every_days);
            let current = o.review_every_days.map(|n| n.to_string()).unwrap_or_default();
            let reviewed = o
                .last_reviewed_on
                .map_or_else(|| "never".to_owned(), |d| d.to_string());
            let next = o
                .review_due()
                .map_or_else(String::new, |d| format!(" · next {d}"));
            view! {
                <div class="mb-2">
                    <SelectField label="Review" options=rhythm_options current=current on_change=save_rhythm />
                </div>
                <p class="mb-2 flex flex-wrap items-center gap-2 text-[12px] text-muted">
                    <span>"Last reviewed " {reviewed} {next}</span>
                    <button class=BUTTON_SOFT on:click=mark_reviewed>"Mark reviewed"</button>
                </p>
            }.into_any()
        } else {
            view! {
                <TextField label="Target date" kind="date"
                    value=objective.target_date.map(|d| d.to_string()).unwrap_or_default()
                    on_commit=save_date />
            }.into_any()
        }}
        <div class="grid grid-cols-2 gap-3">
            {move || {
                // An ongoing objective is never "done": archive it when it ends.
                let ongoing = obj.with(|o| o.ongoing);
                let options: Vec<(String, String)> = ObjectiveStatus::ALL
                    .iter()
                    .filter(|s| !(ongoing && **s == ObjectiveStatus::Done))
                    .map(|s| (s.as_str().to_owned(), objective_status_label(*s).to_owned()))
                    .collect();
                let current = obj.with(|o| o.status.as_str().to_owned());
                view! {
                    <SelectField label="Your assessment" options=options current=current
                        on_change=save_status tint=OBJECTIVE_STATUS_TINT />
                }
            }}
            <SelectField label="Priority" options=priority_options
                current=objective.priority.to_string() on_change=save_priority
                tint=PRIORITY_TINT />
        </div>
    }
}

const KIND_GOAL: &str = "goal";
const KIND_ONGOING: &str = "ongoing";

/// The review rhythms offered, plus the current one when it isn't among them (set by hand or by
/// an import).
fn rhythm_options(current: Option<u32>) -> Vec<(String, String)> {
    let mut options = vec![(String::new(), "Never".to_owned())];
    let presets: [(u32, &str); 6] = [
        (7, "Every week"),
        (14, "Every 2 weeks"),
        (30, "Every month"),
        (90, "Every quarter"),
        (180, "Every 6 months"),
        (365, "Every year"),
    ];
    for (days, label) in presets {
        options.push((days.to_string(), label.to_owned()));
    }
    if let Some(n) = current.filter(|n| !presets.iter().any(|(d, _)| d == n)) {
        options.push((n.to_string(), format!("Every {n} days")));
    }
    options
}

pub(crate) fn candidate_value(n: &NodeSummary) -> String {
    format!("{}:{}", n.node.node_type, n.node.id)
}

pub(crate) fn parse_candidate(v: &str) -> Option<NodeRef> {
    let (t, id) = v.split_once(':')?;
    Some(NodeRef::new(
        NodeType::from_str(t).ok()?,
        Uuid::parse_str(id).ok()?,
    ))
}

/// Editable 0-1 weight of a `contributes_to` link. Empty means unset (full weight).
#[component]
pub fn WeightInput(edge_id: Uuid, weight: Option<f64>) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let set_weight = move |ev: leptos::ev::Event| {
        let v = event_target_value(&ev);
        let attrs = match v.trim() {
            "" => serde_json::json!({}),
            t => match t.parse::<f64>() {
                Ok(w) => serde_json::json!({ "weight": w }),
                Err(_) => {
                    toasts.error(&AppError {
                        code: "invalid".into(),
                        message: "Weight must be a number from 0 to 1".into(),
                    });
                    version.bump();
                    return;
                }
            },
        };
        spawn_local(async move {
            finish(
                api::update_edge_attrs(edge_id, attrs).await,
                toasts,
                version,
            );
        });
    };
    view! {
        <input class=format!("{INPUT} !w-14 text-right") type="number" min="0" max="1" step="0.1"
            placeholder="1" title="Weight (0-1)"
            prop:value=weight.map(|w| w.to_string()).unwrap_or_default()
            on:change=set_weight />
    }
}

#[component]
fn Contributions(detail: ObjectiveDetail, candidates: Vec<NodeSummary>) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let selection = expect_context::<Selection>();
    let objective_id = detail.objective.id;

    let linked: std::collections::HashSet<Uuid> = detail
        .contributions
        .iter()
        .map(|c| c.node.node.id)
        .collect();
    let add_options: Vec<(String, String)> =
        std::iter::once((String::new(), "Add a project or task…".to_owned()))
            .chain(
                candidates
                    .iter()
                    .filter(|c| !linked.contains(&c.node.id))
                    .map(|c| {
                        (
                            candidate_value(c),
                            format!("{} · {}", type_label(c.node.node_type), c.label),
                        )
                    }),
            )
            .collect();
    let nothing_to_link = candidates.is_empty();

    let add = move |v: String| {
        let Some(from) = parse_candidate(&v) else {
            return;
        };
        let new = NewEdge {
            edge_type: EdgeType::ContributesTo,
            from,
            to: NodeRef::new(NodeType::Objective, objective_id),
            attrs: serde_json::json!({ "weight": 1.0 }),
        };
        spawn_local(async move {
            finish(api::add_edge(new).await, toasts, version);
        });
    };

    let rows = detail
        .contributions
        .into_iter()
        .map(|c: Contribution| {
            let (edge_id, node) = (c.edge_id, c.node.node);
            let remove = move |_| {
                spawn_local(async move {
                    finish(api::remove_edge(edge_id).await, toasts, version);
                });
            };
            view! {
                <li class="flex items-center gap-2">
                    <span class="w-14 shrink-0 text-faint">{type_label(node.node_type)}</span>
                    <button class="flex-1 truncate text-left hover:underline" on:click=move |_| selection.open(node)>
                        {c.node.label}
                    </button>
                    <span class=status_word_tone(&c.status).chip()>{humanize(&c.status)}</span>
                    <WeightInput edge_id=edge_id weight=c.weight />
                    <button class="px-1 text-faint hover:text-danger" aria-label="Remove contribution"
                            on:click=remove>"✕"</button>
                </li>
            }
        })
        .collect_view();

    view! {
        <Section title="Contributing work">
            {if linked.is_empty() {
                view! { <p class="mb-2 text-muted">"Nothing linked yet."</p> }.into_any()
            } else {
                view! { <ul class="mb-2 space-y-1">{rows}</ul> }.into_any()
            }}
            <SelectField compact=true action=true options=add_options current=String::new() on_change=add />
            {nothing_to_link.then(|| view! {
                <p class="mt-2 text-[11px] text-muted">"Projects and tasks will be listed here once you create them."</p>
            })}
        </Section>
    }
}

#[component]
fn ArchiveObjective(detail: ObjectiveDetail) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let selection = expect_context::<Selection>();
    let id = detail.objective.id;
    let title = detail.objective.title.clone();
    let links = detail.contributions.len();
    let confirming = RwSignal::new(false);

    let confirm = move |_| {
        spawn_local(async move {
            if finish(api::archive_objective(id).await, toasts, version).is_some() {
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
                            "Archive " <strong>{title.clone()}</strong> "? "
                            {if links == 0 { "Nothing contributes to it.".to_owned() } else {
                                format!("Its {links} contribution link{} will be archived with it; the projects and tasks stay.", if links == 1 { "" } else { "s" })
                            }}
                        </p>
                        <div class="flex gap-2">
                            <button class=BUTTON_DANGER on:click=confirm>"Archive"</button>
                            <button class=BUTTON on:click=move |_| confirming.set(false)>"Cancel"</button>
                        </div>
                    </div>
                }.into_any()
            } else {
                view! { <button class=BUTTON_DANGER on:click=move |_| confirming.set(true)>"Archive objective…"</button> }.into_any()
            }}
        </Section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_values_roundtrip() {
        let n = NodeSummary {
            node: NodeRef::new(NodeType::WaitingOn, Uuid::from_u128(7)),
            label: String::new(),
            archived: false,
        };
        assert_eq!(parse_candidate(&candidate_value(&n)), Some(n.node));
        assert_eq!(parse_candidate(""), None);
        assert_eq!(parse_candidate("task:not-a-uuid"), None);
        assert_eq!(
            parse_candidate("nonsense:00000000-0000-0000-0000-000000000001"),
            None
        );
    }
}
