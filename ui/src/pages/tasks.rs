use leptos::prelude::*;
use leptos_router::{hooks::query_signal_with_options, NavigateOptions};

use crate::{
    components::{
        task_board::TaskBoard,
        task_list::{TaskFilters, TaskList},
    },
    state::ZoomCtx,
};

/// Tasks as a board of status columns (the default) or as a list. The filters are shared, so
/// switching keeps what you searched for.
#[component]
pub fn Tasks() -> impl IntoView {
    let board = RwSignal::new(true);
    let filters = TaskFilters::remembered();
    // `?person=<id>` opens the screen filtered to that person's tasks (a *Your people* frame's
    // *Tasks* button).
    let (person, _) = query_signal_with_options::<String>("person", NavigateOptions::default());
    if let Some(id) = person.get_untracked().filter(|p| !p.is_empty()) {
        filters.assignee.set(id);
    }
    // `?project=<id>` opens the screen filtered to that project (zooming to a project, spec 52).
    // A plain address leaves the remembered filter alone: filters stay until *Reset*.
    let (project, _) = query_signal_with_options::<String>("project", NavigateOptions::default());
    Effect::new(move |_| {
        if let Some(id) = project.get().filter(|p| !p.is_empty()) {
            filters.project.set(id);
        }
    });
    // A zoom that lands here names the project to filter by (or none), even when the address
    // already says so: the dropdown may have been changed by hand since.
    let zoom = expect_context::<ZoomCtx>();
    Effect::new(move |_| {
        if let Some(wanted) = zoom.pending_project.get() {
            zoom.pending_project.set(None);
            filters
                .project
                .set(wanted.map(|id| id.to_string()).unwrap_or_default());
        }
    });
    // Tell zoom which project this screen is about, so Alt+Up starts from it.
    Effect::new(move |_| {
        zoom.page_project
            .set(minimap_types::Uuid::parse_str(&filters.project.get()).ok());
    });
    on_cleanup(move || zoom.page_project.set(None));
    view! {
        {move || if board.get() {
            view! { <TaskBoard filters=filters layout=board /> }.into_any()
        } else {
            view! { <TaskList inbox=false filters=filters layout=board /> }.into_any()
        }}
    }
}

/// Tasks that belong to no project, to be triaged: file into a project, assign, set a due date.
#[component]
pub fn Inbox() -> impl IntoView {
    view! { <crate::components::task_list::TaskList inbox=true /> }
}
