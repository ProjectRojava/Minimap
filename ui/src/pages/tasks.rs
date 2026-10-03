use leptos::prelude::*;

use crate::components::task_list::TaskList;

#[component]
pub fn Tasks() -> impl IntoView {
    view! { <TaskList inbox=false /> }
}

/// Tasks that belong to no project, to be triaged: file into a project, assign, set a due date.
#[component]
pub fn Inbox() -> impl IntoView {
    view! { <TaskList inbox=true /> }
}
