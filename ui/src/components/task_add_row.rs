//! The *Add* row of a task's pane: one place to start the things a task can have (sub-tasks and
//! links, reference links, files, notes) and to ask "what if this slips?". The sections for them
//! stay hidden while empty (`pane_reveal`), so this row is how the first one is added.

use leptos::prelude::*;
use leptos_router::hooks::use_navigate;
use minimap_types::{LinkRelation, NodeRef, NodeType, Uuid};

use crate::{
    api,
    components::{
        attachments::pick_and_attach,
        pane_reveal::Reveal,
        task_links::{group_links, part_buttons},
        what_if_button::DEFAULT_SLIP_DAYS,
    },
    state::{DataVersion, LinkDialog, Scenario},
};

/// An add button: the accent as an outline over a soft fill, so it reads as a button to press.
const ADD: &str = "inline-flex items-center gap-1 rounded-md border border-accent/50 bg-accent/10 px-2.5 py-1 \
                   text-[12px] font-medium text-accent shadow-sm transition-colors \
                   hover:border-accent hover:bg-accent/25";
/// The + at the start of an add button.
const PLUS: &str = "text-[14px] font-semibold leading-none";
/// "What if…" is analysis, not adding: amber, apart from the accent buttons.
const WHAT_IF: &str = "rounded-md border border-warning/50 bg-warning/10 px-2.5 py-0.5 text-[12px] \
                       font-medium text-warning transition-colors hover:border-warning hover:bg-warning/25";

#[component]
pub fn TaskAddRow(id: Uuid) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let dialog = expect_context::<LinkDialog>();
    let scenario = expect_context::<Scenario>();
    let reveal = expect_context::<Reveal>();
    let navigate = use_navigate();
    let node = NodeRef::new(NodeType::Task, id);
    let task = LocalResource::new(move || {
        version.track();
        api::get_task(id)
    });
    let links = LocalResource::new(move || {
        version.track();
        api::list_edges_for(id)
    });
    let what_if = Callback::new(move |()| {
        scenario.start(node, DEFAULT_SLIP_DAYS);
        navigate("/what-if", Default::default());
    });

    move || {
        let title = task.get().and_then(|t| t.ok())?.title;
        let links = links.get().and_then(|l| l.ok())?;
        let groups = group_links(&links);
        // A task is part of one task and the tree is one level deep.
        let (can_add_subtasks, can_join_parent) =
            part_buttons(!groups.parent.is_empty(), !groups.subtasks.is_empty());
        let (t1, t2, t3) = (title.clone(), title.clone(), title);
        // A tinted card in the accent colour, so it reads as the place to add things rather
        // than one more row of the pane: a heading line (with "what if" at its right, in the
        // amber of a warning) over the add buttons, each led by a +.
        Some(view! {
            <div class="mx-4 my-3 rounded-md border border-accent/30 bg-accent/5 px-3 py-2.5"
                 role="group" aria-label="Add to this task">
                <div class="mb-2 flex items-center gap-2">
                    <span class="flex h-4 w-4 items-center justify-center rounded-full bg-accent text-[13px] font-bold leading-none text-canvas"
                          aria-hidden="true">"+"</span>
                    <span class="text-[11px] font-semibold uppercase tracking-wider text-accent">"Add to this task"</span>
                    <button class=format!("{WHAT_IF} ml-auto") title="See what it would push if it slips"
                            on:click=move |_| what_if.run(())>
                        "What if…"
                    </button>
                </div>
                <div class="flex flex-wrap gap-1.5">
                    {can_add_subtasks.then(|| view! {
                        <button class=ADD title="A task that is part of this one"
                                on:click=move |_| dialog.open_as(id, t1.clone(), false, LinkRelation::Subtask)>
                            <span class=PLUS>"+"</span>"Sub-task"
                        </button>
                    })}
                    {can_join_parent.then(|| view! {
                        <button class=ADD title="Make this task part of another"
                                on:click=move |_| dialog.open_as(id, t2.clone(), true, LinkRelation::Parent)>
                            <span class=PLUS>"+"</span>"Part of…"
                        </button>
                    })}
                    <button class=ADD title="A task that blocks, follows or relates to this one"
                            on:click=move |_| dialog.open(id, t3.clone(), false)>
                        <span class=PLUS>"+"</span>"Linked task"
                    </button>
                    <button class=ADD title="A web page or Google Drive address"
                            on:click=move |_| reveal.ask("files")>
                        <span class=PLUS>"+"</span>"Reference link"
                    </button>
                    <button class=ADD title="Attach a file" on:click=move |_| pick_and_attach(node)>
                        <span class=PLUS>"+"</span>"File"
                    </button>
                    <button class=ADD title="Write a note or finding about this task"
                            on:click=move |_| reveal.ask("notes")>
                        <span class=PLUS>"+"</span>"Note"
                    </button>
                </div>
            </div>
        })
    }
}
