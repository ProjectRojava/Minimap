use leptos::{prelude::*, task::spawn_local};
use minimap_types::{
    timefmt::parse_date, CreateObjective, NodeRef, NodeType, ObjectiveGrouping, ObjectiveRow,
    ObjectiveStatus,
};

use crate::{
    api,
    components::{
        form::{SelectField, BUTTON, BUTTON_PRIMARY, INPUT},
        node_row::NodeRow,
    },
    labels::{objective_status_label, priority_option, priority_short},
    state::{finish, DataVersion, ListNav, Selection, Toasts},
};

const COLS: &str =
    "grid w-full items-center gap-3 grid-cols-[2rem_minmax(0,1fr)_6rem_6.5rem_3.5rem]";

#[component]
pub fn Objectives() -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let list = expect_context::<ListNav>();
    let selection = expect_context::<Selection>();

    let by_quarter = RwSignal::new(false);
    let groups = LocalResource::new(move || {
        version.track();
        let grouping = if by_quarter.get() {
            ObjectiveGrouping::Quarter
        } else {
            ObjectiveGrouping::None
        };
        api::list_objectives(grouping)
    });
    // Keyboard rows are the objectives in on-screen order, across groups.
    Effect::new(move |_| match groups.get() {
        Some(Ok(g)) => list.set_items(
            g.iter()
                .flat_map(|g| g.rows.iter())
                .map(|r| NodeRef::new(NodeType::Objective, r.objective.id))
                .collect(),
        ),
        Some(Err(e)) => toasts.error(&e),
        None => {}
    });

    let adding = RwSignal::new(false);
    let title = RwSignal::new(String::new());
    let target = RwSignal::new(String::new());
    let priority = RwSignal::new("3".to_owned());
    let submit = move || {
        let t = title.get_untracked();
        if t.trim().is_empty() {
            return;
        }
        let date = target.get_untracked();
        let target_date = match date.trim() {
            "" => None,
            d => match parse_date(d) {
                Ok(d) => Some(d),
                Err(_) => {
                    toasts.error(&minimap_types::AppError {
                        code: "invalid".into(),
                        message: "Target date must look like 2027-03-31".into(),
                    });
                    return;
                }
            },
        };
        let p = priority.get_untracked().parse::<u8>().ok();
        spawn_local(async move {
            let input = CreateObjective {
                title: t,
                description: String::new(),
                target_date,
                status: None,
                priority: p,
            };
            if let Some(o) = finish(api::create_objective(input).await, toasts, version) {
                title.set(String::new());
                target.set(String::new());
                selection.open(NodeRef::new(NodeType::Objective, o.id));
            }
        });
    };

    view! {
        <div class="flex flex-col h-full">
            <header class="flex items-center gap-3 px-4 h-10 shrink-0 border-b border-line">
                <h1 class="text-[13px] font-semibold">"Objectives"</h1>
                <button class=BUTTON on:click=move |_| adding.update(|a| *a = !*a)>
                    {move || if adding.get() { "Cancel" } else { "New objective" }}
                </button>
                <button class=move || format!("{BUTTON} ml-auto {}", if by_quarter.get() { "bg-active" } else { "" })
                        aria-pressed=move || by_quarter.get().to_string()
                        title="Group by the quarter of the target date"
                        on:click=move |_| by_quarter.update(|b| *b = !*b)>
                    "Group by quarter"
                </button>
            </header>
            <Show when=move || adding.get()>
                <form class="flex items-end gap-2 px-4 py-2 border-b border-line bg-panel"
                      on:submit=move |ev| { ev.prevent_default(); submit(); }>
                    <input class=INPUT placeholder="Objective" autofocus prop:value=move || title.get()
                           on:input=move |ev| title.set(event_target_value(&ev)) />
                    <input class=format!("{INPUT} !w-40") type="date" title="Target date (optional)"
                           prop:value=move || target.get()
                           on:input=move |ev| target.set(event_target_value(&ev)) />
                    <SelectField compact=true options=priority_options() current="3".to_owned()
                                 on_change=move |v: String| priority.set(v) />
                    <button class=BUTTON_PRIMARY type="submit">"Add"</button>
                </form>
            </Show>
            <div class=format!("{COLS} px-3 py-1 text-[11px] uppercase tracking-wide text-muted border-b border-line")>
                <span>"Pri"</span><span>"Objective"</span><span>"Assessment"</span>
                <span>"Target"</span><span class="text-right">"Work"</span>
            </div>
            <div class="flex-1 overflow-y-auto" role="table">
                {move || match groups.get() {
                    None => view! { <p class="p-4 text-muted">"Loading…"</p> }.into_any(),
                    Some(Err(_)) => view! { <p class="p-4 text-muted">"Couldn't load objectives."</p> }.into_any(),
                    Some(Ok(g)) if g.is_empty() => view! {
                        <p class="p-4 text-muted">"No objectives yet. Add the outcomes your portfolio serves."</p>
                    }.into_any(),
                    Some(Ok(g)) => {
                        let mut index = 0;
                        let mut out = Vec::new();
                        for group in g {
                            if let Some(label) = group.label {
                                out.push(view! {
                                    <div class="px-3 pt-3 pb-1 text-[11px] font-semibold uppercase tracking-wide text-muted">
                                        {label} <span class="ml-1 font-normal text-faint">{group.rows.len()}</span>
                                    </div>
                                }.into_any());
                            }
                            for row in group.rows {
                                out.push(objective_row(row, index).into_any());
                                index += 1;
                            }
                        }
                        out.into_any()
                    }
                }}
            </div>
        </div>
    }
}

fn priority_options() -> Vec<(String, String)> {
    (1..=5u8)
        .map(|p| (p.to_string(), priority_option(p)))
        .collect()
}

fn objective_row(row: ObjectiveRow, index: usize) -> impl IntoView {
    let o = row.objective;
    let node = NodeRef::new(NodeType::Objective, o.id);
    // Without colour, "needs attention" reads as heavier text.
    let attention = matches!(
        o.status,
        ObjectiveStatus::AtRisk | ObjectiveStatus::OffTrack
    );
    let status_class = if attention {
        "text-fg font-medium"
    } else {
        "text-muted"
    };
    view! {
        <NodeRow node=node index=index>
            <div class=COLS>
                <span class="text-muted tabular-nums">{priority_short(o.priority)}</span>
                <span class="truncate font-medium">{o.title}</span>
                <span class=status_class>{objective_status_label(o.status)}</span>
                <span class="text-muted tabular-nums">{o.target_date.map(|d| d.to_string()).unwrap_or_default()}</span>
                <span class="text-right tabular-nums text-muted">{row.contribution_count}</span>
            </div>
        </NodeRow>
    }
}
