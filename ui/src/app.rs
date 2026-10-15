use leptos::prelude::*;
use leptos_router::{
    components::{Route, Router, Routes},
    path,
};

use crate::{
    components::{
        card_menu::CardMenuHost,
        detail_pane::DetailPane,
        first_run::FirstRun,
        link_dialog::LinkDialogHost,
        palette::PaletteHost,
        sidebar::Sidebar,
        sync_status::{use_sync_status, StatusBar},
        titlebar::TitleBar,
        toasts::ToastHost,
        unlock::UnlockScreen,
    },
    keyboard::use_global_shortcuts,
    pages::{
        capacity::Capacity,
        decisions::Decisions,
        deep_link::{DeepLink, NotFound},
        graph::Graph,
        help::Help,
        notes::Notes,
        objectives::Objectives,
        overview::Overview,
        people::People,
        projects::Projects,
        settings::Settings,
        archive::Archive,
        tasks::{Inbox, Tasks},
        teams::Teams,
        this_week::ThisWeek,
        waiting_on::WaitingOn,
        weekly_review::WeeklyReview,
        what_if::WhatIf,
    },
    state::{
        CardMenuState, DataVersion, LinkDialog, ListNav, PaletteOpen, Scenario, Selection, Toasts,
    },
    theme::ThemeCtx,
};

#[component]
pub fn App() -> impl IntoView {
    // The radar (spec 53) is a second window of the same app: a page of its own, with none of the
    // shell, that only reads.
    if crate::window::is_radar_window() {
        return view! { <crate::pages::radar::RadarApp /> }.into_any();
    }
    provide_context(Selection(RwSignal::new(None)));
    provide_context(PaletteOpen(RwSignal::new(false)));
    provide_context(LinkDialog(RwSignal::new(None)));
    provide_context(crate::state::MeetingDialog(RwSignal::new(None)));
    provide_context(crate::state::NowClock(RwSignal::new(None)));
    provide_context(CardMenuState(RwSignal::new(None)));
    provide_context(crate::components::ping::PingBoxState::new());
    provide_context(crate::components::save_points::SaveBoxState(RwSignal::new(
        false,
    )));
    provide_context(Scenario(RwSignal::new(Vec::new())));
    provide_context(ListNav::new());
    provide_context(Toasts::new());
    provide_context(crate::state::FinishDecision::new());
    provide_context(crate::components::confirm_archive::ConfirmArchive::new());
    provide_context(DataVersion::new());
    // Portfolio view or My view (spec 50), and what is in My view.
    crate::components::orientation::Orientation::provide();
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
    .into_any()
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
    // Zoom between the portfolio, an objective, a project and an item (spec 52). Before the
    // shortcuts, which ask for it.
    crate::state::ZoomCtx::provide();
    use_global_shortcuts();
    // The radar window asks for an item to be opened here (spec 53).
    crate::components::radar::use_open_requests();
    // Google Drive status, polled for the status bar, banners and Settings; a merge from another
    // computer reloads every screen.
    use_sync_status();
    // One hue per objective, worn by its projects and tasks (ADR-0012).
    crate::components::objective_colour::ObjectiveColours::provide();
    // The user's task types (spec 32), for chips, dropdowns and filters.
    crate::components::task_type::TaskTypes::provide();
    // How stale what is in play has become (spec 43), for the chips and fading on rows and cards.
    crate::components::fog::FogCtx::provide();
    // The live pings (ADR-0020), for the pin on rows and cards and the card on This week.
    crate::components::ping::PingsCtx::provide();
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
                    <Route path=path!("/archive") view=Archive />
                    <Route path=path!("/what-if") view=WhatIf />
                    <Route path=path!("/weekly-review") view=WeeklyReview />
                    <Route path=path!("/settings") view=Settings />
                    <Route path=path!("/help") view=Help />
                    <Route path=path!("/about") view=crate::pages::about::About />
                    <Route path=path!("/:type/:id") view=DeepLink />
                </Routes>
            </main>
            <DetailPane />
            </div>
            <StatusBar />
            <crate::components::zoom::ZoomHost />
            <PaletteHost />
            <LinkDialogHost />
            <crate::components::finish_decision::FinishDecisionHost />
            <crate::components::confirm_archive::ConfirmArchiveHost />
            <crate::components::meeting::MeetingDialogHost />
            <crate::components::meeting::MeetingClock />
            <CardMenuHost />
            <crate::components::ping::PingBoxHost />
            <crate::components::save_points::SaveBoxHost />
            <ToastHost />
            <FirstRun />
        </div>
    }
}
