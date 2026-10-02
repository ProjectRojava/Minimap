use leptos::prelude::*;
use leptos_router::{
    components::{Route, Router, Routes},
    path,
};

use crate::{
    components::{detail_pane::DetailPane, sidebar::Sidebar, toasts::ToastHost},
    keyboard::use_global_shortcuts,
    pages::{
        deep_link::{DeepLink, NotFound},
        overview::Overview,
        placeholder::Placeholder,
    },
    state::{ListNav, Selection, Toasts},
};

#[component]
pub fn App() -> impl IntoView {
    provide_context(Selection(RwSignal::new(None)));
    provide_context(ListNav::new());
    provide_context(Toasts::new());

    view! {
        <Router>
            <Shell />
        </Router>
    }
}

/// Sidebar | page | detail pane, plus toasts and keyboard shortcuts.
#[component]
fn Shell() -> impl IntoView {
    use_global_shortcuts();
    let list = expect_context::<ListNav>();
    // Rows belong to the screen that registered them; clear them on route change.
    let location = leptos_router::hooks::use_location();
    Effect::new(move |_| {
        location.pathname.track();
        list.clear();
    });

    view! {
        <div class="flex h-screen overflow-hidden text-[13px]">
            <Sidebar />
            <main class="flex-1 min-w-0 overflow-y-auto">
                <Routes fallback=NotFound>
                    <Route path=path!("/") view=Overview />
                    <Route path=path!("/inbox") view=|| view! { <Placeholder title="Inbox" spec="06-tasks-and-inbox" /> } />
                    <Route path=path!("/objectives") view=|| view! { <Placeholder title="Objectives" spec="04-objectives" /> } />
                    <Route path=path!("/projects") view=|| view! { <Placeholder title="Projects" spec="05-projects" /> } />
                    <Route path=path!("/tasks") view=|| view! { <Placeholder title="Tasks" spec="06-tasks-and-inbox" /> } />
                    <Route path=path!("/people") view=|| view! { <Placeholder title="People" spec="03-people-and-teams" /> } />
                    <Route path=path!("/teams") view=|| view! { <Placeholder title="Teams" spec="03-people-and-teams" /> } />
                    <Route path=path!("/notes") view=|| view! { <Placeholder title="Notes" spec="09-notes-and-mentions" /> } />
                    <Route path=path!("/decisions") view=|| view! { <Placeholder title="Decisions" spec="10-decisions" /> } />
                    <Route path=path!("/waiting-on") view=|| view! { <Placeholder title="Waiting on" spec="08-waiting-on" /> } />
                    <Route path=path!("/settings") view=|| view! { <Placeholder title="Settings" spec="23-settings" /> } />
                    <Route path=path!("/:type/:id") view=DeepLink />
                </Routes>
            </main>
            <DetailPane />
            <ToastHost />
        </div>
    }
}
