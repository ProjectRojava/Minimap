use leptos::prelude::*;
use leptos_router::{
    components::{Route, Router, Routes},
    path,
};

use crate::{
    components::{
        detail_pane::DetailPane, first_run::FirstRun, palette::PaletteHost, sidebar::Sidebar,
        titlebar::TitleBar, toasts::ToastHost, unlock::UnlockScreen,
    },
    keyboard::use_global_shortcuts,
    pages::{
        capacity::Capacity,
        decisions::Decisions,
        deep_link::{DeepLink, NotFound},
        graph::Graph,
        notes::Notes,
        objectives::Objectives,
        overview::Overview,
        people::People,
        projects::Projects,
        settings::Settings,
        tasks::{Inbox, Tasks},
        teams::Teams,
        this_week::ThisWeek,
        waiting_on::WaitingOn,
        weekly_review::WeeklyReview,
        what_if::WhatIf,
    },
    state::{DataVersion, ListNav, PaletteOpen, Scenario, Selection, Toasts},
    theme::ThemeCtx,
};

#[component]
pub fn App() -> impl IntoView {
    provide_context(Selection(RwSignal::new(None)));
    provide_context(PaletteOpen(RwSignal::new(false)));
    provide_context(Scenario(RwSignal::new(Vec::new())));
    provide_context(ListNav::new());
    provide_context(Toasts::new());
    provide_context(DataVersion::new());
    let theme = ThemeCtx::new();
    provide_context(theme);
    // The database is the source of truth for the theme; localStorage only speeds up startup.
    let settings = LocalResource::new(crate::api::get_settings);
    Effect::new(move |_| {
        if let Some(Ok(s)) = settings.get() {
            theme.select(&s.theme);
        }
    });

    view! {
        <Router>
            <LockGate />
        </Router>
    }
}

/// An encrypted database that is waiting for its key shows the unlock screen instead of the app
/// (every command would answer "locked"); otherwise the app.
#[component]
fn LockGate() -> impl IntoView {
    let status = LocalResource::new(crate::api::get_security_status);
    view! {
        {move || match status.get() {
            None => view! { <div class="h-screen"></div> }.into_any(),
            Some(Ok(s)) if s.locked => view! { <UnlockScreen /> }.into_any(),
            Some(_) => view! { <Shell /> }.into_any(),
        }}
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
        <div class="flex h-screen flex-col overflow-hidden text-[13px]">
            <TitleBar />
            <div class="flex min-h-0 flex-1">
            <Sidebar />
            <main class="flex-1 min-w-0 overflow-y-auto">
                <Routes fallback=NotFound>
                    <Route path=path!("/") view=ThisWeek />
                    <Route path=path!("/overview") view=Overview />
                    <Route path=path!("/inbox") view=Inbox />
                    <Route path=path!("/objectives") view=Objectives />
                    <Route path=path!("/projects") view=Projects />
                    <Route path=path!("/tasks") view=Tasks />
                    <Route path=path!("/people") view=People />
                    <Route path=path!("/capacity") view=Capacity />
                    <Route path=path!("/graph") view=Graph />
                    <Route path=path!("/teams") view=Teams />
                    <Route path=path!("/notes") view=Notes />
                    <Route path=path!("/decisions") view=Decisions />
                    <Route path=path!("/waiting-on") view=WaitingOn />
                    <Route path=path!("/what-if") view=WhatIf />
                    <Route path=path!("/weekly-review") view=WeeklyReview />
                    <Route path=path!("/settings") view=Settings />
                    <Route path=path!("/:type/:id") view=DeepLink />
                </Routes>
            </main>
            <DetailPane />
            </div>
            <PaletteHost />
            <ToastHost />
            <FirstRun />
        </div>
    }
}
