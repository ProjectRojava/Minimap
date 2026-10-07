//! "What if this slips?" on a task or project: starts a scenario and opens the screen.

use leptos::prelude::*;
use leptos_router::hooks::use_navigate;
use minimap_types::NodeRef;

use crate::{
    components::{detail_pane::Section, form::BUTTON_SOFT},
    state::Scenario,
};

/// Slip used when starting from a button (working days); the screen lets you change it.
pub const DEFAULT_SLIP_DAYS: u32 = 5;

#[component]
pub fn WhatIfButton(node: NodeRef) -> impl IntoView {
    let scenario = expect_context::<Scenario>();
    let navigate = use_navigate();
    let go = move |_| {
        scenario.start(node, DEFAULT_SLIP_DAYS);
        navigate("/what-if", Default::default());
    };
    view! {
        <Section title="What if">
            <button class=BUTTON_SOFT on:click=go>"What if this slips?"</button>
            <p class="mt-1 text-[11px] text-muted">
                "See which tasks, projects, objectives and people it would push, before it happens."
            </p>
        </Section>
    }
}
