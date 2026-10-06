use leptos::prelude::*;

use crate::components::{
    task_board::TaskBoard,
    task_list::{TaskFilters, TaskList},
};

/// Tasks as a board of status columns (the default) or as a list. The filters are shared, so
/// switching keeps what you searched for.
#[component]
pub fn Tasks() -> impl IntoView {
    let board = RwSignal::new(true);
    let filters = TaskFilters::new();
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
