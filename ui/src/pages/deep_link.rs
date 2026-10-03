use std::str::FromStr;

use leptos::prelude::*;
use leptos_router::{components::Redirect, hooks::use_params_map};
use minimap_types::{NodeRef, NodeType, Uuid};

use crate::{nav::list_path, state::Selection};

/// `/:type/:id` (e.g. `/task/0192…`): opens the node in the detail pane over its list screen.
#[component]
pub fn DeepLink() -> impl IntoView {
    let params = use_params_map();
    let selection = expect_context::<Selection>();

    move || {
        let p = params.get();
        let parsed = p
            .get("type")
            .and_then(|t| NodeType::from_str(&t.replace('-', "_")).ok())
            .zip(p.get("id").and_then(|i| Uuid::parse_str(&i).ok()));
        match parsed {
            Some((node_type, id)) => {
                selection.open(NodeRef::new(node_type, id));
                view! { <Redirect path=list_path(node_type) /> }.into_any()
            }
            None => view! { <NotFound /> }.into_any(),
        }
    }
}

#[component]
pub fn NotFound() -> impl IntoView {
    view! {
        <div class="p-6">
            <h1 class="text-[13px] font-semibold">"Not found"</h1>
            <p class="mt-1 text-[13px] text-muted">"That page doesn't exist."</p>
        </div>
    }
}
