//! "Health" sections for the project and objective detail panes: the computed level, its
//! score and the reasons. The numbers come from `get_portfolio_overview`, the same ones the
//! Overview shows.

use leptos::prelude::*;
use minimap_types::{Health, HealthLevel, PortfolioOverview, Uuid};

use crate::{
    api,
    components::{
        detail_pane::Section,
        health::{level_label, HealthMark, ReasonList},
        people_panel::error_line,
    },
    labels::objective_status_label,
    state::DataVersion,
};

fn overview_resource() -> LocalResource<Result<PortfolioOverview, minimap_types::AppError>> {
    let version = expect_context::<DataVersion>();
    LocalResource::new(move || {
        version.track();
        api::get_portfolio_overview()
    })
}

#[component]
fn HealthBody(health: Health, #[prop(optional)] note: Option<String>) -> impl IntoView {
    let scored = health.level != HealthLevel::Idle;
    view! {
        <p class="mb-1 flex items-center gap-1 font-medium">
            <HealthMark level=health.level score=health.score />
            {level_label(health.level)}
            {scored.then(|| view! { <span class="font-normal text-muted">{format!("· score {}/100", health.score)}</span> })}
        </p>
        <ReasonList reasons=health.reasons />
        {note.map(|n| view! { <p class="mt-1 text-[11px] text-muted">{n}</p> })}
    }
}

#[component]
pub fn ProjectHealthSection(project: Uuid) -> impl IntoView {
    let overview = overview_resource();
    view! {
        <Section title="Health">
            {move || match overview.get() {
                Some(Ok(o)) => match o.project_health(project) {
                    Some(row) => view! {
                        <HealthBody health=row.health.clone()
                            note="Thresholds are in Settings. Lateness comes from the Schedule below.".to_owned() />
                    }.into_any(),
                    None => view! { <p class="text-muted">"Not scored."</p> }.into_any(),
                },
                Some(Err(e)) => error_line(e),
                None => view! { <p class="text-muted">"Loading…"</p> }.into_any(),
            }}
        </Section>
    }
}

#[component]
pub fn ObjectiveHealthSection(objective: Uuid) -> impl IntoView {
    let overview = overview_resource();
    view! {
        <Section title="Health">
            {move || match overview.get() {
                Some(Ok(o)) => match o.objective_health(objective) {
                    Some(row) => {
                        let note = format!(
                            "Your assessment is {}; this is computed from the projects and tasks that contribute to it.",
                            objective_status_label(row.status)
                        );
                        view! { <HealthBody health=row.health.clone() note=note /> }.into_any()
                    }
                    None => view! { <p class="text-muted">"Not scored."</p> }.into_any(),
                },
                Some(Err(e)) => error_line(e),
                None => view! { <p class="text-muted">"Loading…"</p> }.into_any(),
            }}
        </Section>
    }
}
