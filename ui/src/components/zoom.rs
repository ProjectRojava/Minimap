//! Zoom (spec 52): the host that carries zoom moves out and the breadcrumb that shows where you
//! are on the way from the whole portfolio down to one item.

use leptos::prelude::*;
use leptos_router::{
    hooks::{use_location, use_navigate},
    NavigateOptions,
};
use minimap_types::{NodeRef, ZoomStep};

use crate::{
    api,
    state::{DataVersion, ListNav, Selection, ZoomCtx},
    zoom::{join_route, path_of_route, project_of_route},
};

/// Carries out the moves `ZoomCtx` asks for (navigate, open the item, remember where to come
/// back to) and forgets where to come back to when you navigate by any other means. Renders
/// nothing. Mount once, inside the router.
#[component]
pub fn ZoomHost() -> impl IntoView {
    let ctx = expect_context::<ZoomCtx>();
    let selection = expect_context::<Selection>();
    let list = expect_context::<ListNav>();
    let navigate = use_navigate();
    let location = use_location();

    Effect::new(move |_| {
        let Some(m) = ctx.request.get() else {
            return;
        };
        ctx.request.set(None);
        let changes = m.route != ctx.route();
        if let Some(frame) = m.push {
            ctx.stack.update(|s| s.push(frame));
        }
        // The Tasks screen takes the project its route names (or none) even when the route
        // itself does not change, because the filter can have been changed by hand since.
        if path_of_route(&m.route) == "/tasks" {
            ctx.pending_project.set(Some(project_of_route(&m.route)));
        }
        if changes {
            ctx.expected.set_value(Some(m.route.clone()));
            navigate(&m.route, NavigateOptions::default());
        }
        match m.selection {
            Some(node) => selection.open(node),
            None => selection.close(),
        }
        if let Some(cursor) = m.cursor {
            list.restore_cursor(cursor, changes);
        }
    });

    // Arriving where a zoom went is not a navigation; any other change of screen is.
    Effect::new(move |_| {
        let here = join_route(&location.pathname.get(), &location.search.get());
        if ctx.expected.get_value().as_deref() == Some(here.as_str()) {
            ctx.expected.set_value(None);
        } else if ctx.stack.with_untracked(|s| !s.is_empty()) {
            ctx.stack.update(Vec::clear);
        }
    });
}

/// What a breadcrumb is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrumbFocus {
    /// No breadcrumb.
    Hidden,
    /// Just "World" (the Overview).
    World,
    /// The path down to this item.
    Node(NodeRef),
}

/// The breadcrumb for a screen filtered by a project (the Tasks screen): the path to the project,
/// or nothing when no project is chosen.
pub fn project_crumbs(project: RwSignal<String>) -> Signal<CrumbFocus> {
    Signal::derive(
        move || match minimap_types::Uuid::parse_str(&project.get()) {
            Ok(id) => CrumbFocus::Node(NodeRef::new(minimap_types::NodeType::Project, id)),
            Err(_) => CrumbFocus::Hidden,
        },
    )
}

/// "World › Launch in the EU ▾ › EU Region": the path to what you are looking at. Every step but
/// the last zooms there when clicked; the ▾ after an objective lists the other objectives its
/// project serves.
#[component]
pub fn Crumbs(focus: Signal<CrumbFocus>) -> impl IntoView {
    let ctx = expect_context::<ZoomCtx>();
    let version = expect_context::<DataVersion>();
    let path = LocalResource::new(move || {
        version.track();
        let focus = focus.get();
        async move {
            match focus {
                CrumbFocus::Hidden => Vec::new(),
                CrumbFocus::World => vec![ZoomStep::world()],
                CrumbFocus::Node(node) => api::get_zoom_path(node).await.unwrap_or_default(),
            }
        }
    });
    view! {
        <nav class="flex min-w-0 max-w-[36rem] items-center gap-1 overflow-hidden whitespace-nowrap text-[11px] leading-4 text-muted"
             aria-label="Zoom path">
            {move || {
                let steps = path.get().unwrap_or_default();
                let last = steps.len().saturating_sub(1);
                steps
                    .into_iter()
                    .enumerate()
                    .map(|(i, step)| view! {
                        {(i > 0).then(|| view! { <span class="text-faint" aria-hidden="true">"›"</span> })}
                        <Crumb ctx=ctx step=step current={i == last} />
                    })
                    .collect_view()
            }}
        </nav>
    }
}

#[component]
fn Crumb(ctx: ZoomCtx, step: ZoomStep, current: bool) -> impl IntoView {
    let label = step.label().to_owned();
    let alternatives = step.alternatives.clone();
    let open = RwSignal::new(false);
    let target = step.clone();
    view! {
        <span class="relative flex shrink-0 items-center gap-0.5">
            {if current {
                view! { <span class="truncate text-fg">{label}</span> }.into_any()
            } else {
                view! {
                    <button class="truncate rounded-sm hover:text-fg hover:underline"
                            title="Zoom here"
                            on:click=move |_| ctx.go_to(&target)>{label}</button>
                }.into_any()
            }}
            {(!alternatives.is_empty()).then(|| {
                let menu = alternatives.clone();
                view! {
                    <button class="rounded-sm px-0.5 hover:bg-hover hover:text-fg"
                            aria-label="Other objectives this serves" title="Other objectives this serves"
                            aria-expanded=move || open.get().to_string()
                            on:click=move |_| open.update(|o| *o = !*o)>"▾"</button>
                    {move || open.get().then(|| {
                        let menu = menu.clone();
                        view! {
                            <div class="fixed inset-0 z-40" on:click=move |_| open.set(false)></div>
                            <ul class="absolute left-0 top-full z-50 mt-1 min-w-44 rounded-sm border border-line bg-panel py-1 text-[12px] text-fg shadow-lg"
                                role="menu">
                                {menu.into_iter().map(|alt| {
                                    let name = alt.label.clone();
                                    let to = ZoomStep::of(minimap_types::Level::Objective, alt);
                                    view! {
                                        <li role="none">
                                            <button class="block w-full truncate px-3 py-1 text-left hover:bg-hover"
                                                    role="menuitem"
                                                    on:click=move |_| { open.set(false); ctx.go_to(&to); }>
                                                {name}
                                            </button>
                                        </li>
                                    }
                                }).collect_view()}
                            </ul>
                        }
                    })}
                }
            })}
        </span>
    }
}
