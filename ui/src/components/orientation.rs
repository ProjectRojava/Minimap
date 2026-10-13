//! Portfolio view and My view (docs/product-design/07-orientation.md, spec 50): the same map
//! north-up (everything) or player-up (everything as it touches you). One toggle in the title bar
//! (`v`), remembered on this computer only; the screens that draw a map follow it, and in My view
//! each item says why it is there.
//!
//! The rules (what touches you, and why) are `minimap_core::orientation`; this only words them.

use std::collections::HashMap;

use leptos::{prelude::*, web_sys};
use minimap_types::{MyScope, NodeType, Uuid, View, Why};

use crate::{
    api,
    components::{form::BUTTON, page::Icon},
    state::DataVersion,
};

const STORAGE_KEY: &str = "minimap.view";

/// The remembered view (a convenience of this computer: nothing breaks without it).
fn load_view() -> View {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item(STORAGE_KEY).ok().flatten())
        .map_or(View::Portfolio, |word| View::from_str_or_default(&word))
}

fn save_view(view: View) {
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.set_item(STORAGE_KEY, view.as_str());
    }
}

// ------------------------------------------------------------------ pure wording

/// Why something is in My view, in a few words. `name_of` gives the name of the task or project a
/// reason refers to.
pub fn why_text(why: Why, name_of: &dyn Fn(Uuid) -> String) -> String {
    match why {
        Why::Assigned => "assigned to you".to_owned(),
        Why::Owner => "you own it".to_owned(),
        Why::InMyProject => "in your project".to_owned(),
        Why::Reaches { via } => {
            let name = name_of(via);
            if name.is_empty() {
                "reaches you".to_owned()
            } else {
                format!("reaches you via {name}")
            }
        }
        Why::Blocks { .. } => "blocked by your work".to_owned(),
        Why::Serves => "serves your work".to_owned(),
        Why::Report => "reports to you".to_owned(),
        Why::OnMyProjects => "on your projects".to_owned(),
        Why::Me => "you".to_owned(),
    }
}

/// What a row's chip says: the first reason, or, for a project that is only here because it holds
/// your tasks, how many ("3 of your tasks").
pub fn chip_text(
    reasons: &[Why],
    own_tasks: Option<u32>,
    name_of: &dyn Fn(Uuid) -> String,
) -> Option<String> {
    match (reasons.first(), own_tasks) {
        // A project you own or that holds your tasks says how many of the tasks are yours.
        (Some(Why::Owner), _) | (None, None) => reasons.first().map(|w| why_text(*w, name_of)),
        (None, Some(1)) => Some("one of your tasks".to_owned()),
        (None, Some(n)) => Some(format!("{n} of your tasks")),
        (Some(w), _) => Some(why_text(*w, name_of)),
    }
}

/// The words on the title bar's button.
pub fn toggle_label(view: View) -> &'static str {
    match view {
        View::Portfolio => "Portfolio",
        View::Mine => "Mine",
    }
}

/// The hover text of the button.
pub fn toggle_title(view: View, can_be_mine: bool) -> String {
    if !can_be_mine {
        return "Set who you are in People first".to_owned();
    }
    let now = match view {
        View::Portfolio => "Portfolio view: everything",
        View::Mine => "My view: everything as it touches you",
    };
    format!(
        "{now}. Press v to switch to {} view",
        toggle_label(view.toggled()).to_lowercase()
    )
}

/// The line under a screen's header in My view.
pub const BANNER_TEXT: &str = "My view: showing what touches you.";

// ------------------------------------------------------------------ the shared state

/// The view in effect and what is in My view. Provided once by the app; the scope reloads when
/// data changes and keeps the last answer, so chips and filters never blink.
#[derive(Clone, Copy)]
pub struct Orientation {
    chosen: RwSignal<View>,
    scope: RwSignal<Option<MyScope>>,
    names: RwSignal<HashMap<Uuid, String>>,
}

impl Orientation {
    pub fn provide() -> Self {
        let version = expect_context::<DataVersion>();
        let chosen = RwSignal::new(load_view());
        let scope = RwSignal::new(None::<MyScope>);
        let names = RwSignal::new(HashMap::new());
        let loaded = LocalResource::new(move || {
            version.track();
            api::get_my_scope()
        });
        Effect::new(move |_| {
            if let Some(Ok(s)) = loaded.get() {
                scope.set(Some(s));
            }
        });
        // The names reasons refer to ("reaches you via Pen test"), needed only in My view.
        let named = LocalResource::new(move || {
            version.track();
            let wanted = chosen.get() == View::Mine;
            async move {
                if !wanted {
                    return Vec::new();
                }
                let mut all = api::list_node_summaries(NodeType::Task)
                    .await
                    .unwrap_or_default();
                all.extend(
                    api::list_node_summaries(NodeType::Project)
                        .await
                        .unwrap_or_default(),
                );
                all
            }
        });
        Effect::new(move |_| {
            if let Some(list) = named.get() {
                names.set(list.into_iter().map(|s| (s.node.id, s.label)).collect());
            }
        });
        let ctx = Self {
            chosen,
            scope,
            names,
        };
        provide_context(ctx);
        ctx
    }

    /// There is a "me" to look from (true while the scope is still loading).
    pub fn can_be_mine(&self) -> bool {
        self.scope
            .with(|s| s.as_ref().is_none_or(|s| !s.people.is_empty()))
    }

    /// The view in effect: Mine only when asked for and there is a "me" (tracks).
    pub fn view(&self) -> View {
        if self.chosen.get() == View::Mine && self.can_be_mine() {
            View::Mine
        } else {
            View::Portfolio
        }
    }

    pub fn is_mine(&self) -> bool {
        self.view() == View::Mine
    }

    pub fn set(&self, view: View) {
        self.chosen.set(view);
        save_view(view);
    }

    /// Flips the view. Returns false (and does nothing) when there is no "me" to look from.
    pub fn toggle(&self) -> bool {
        if !self.can_be_mine() {
            return false;
        }
        self.set(self.chosen.get_untracked().toggled());
        true
    }

    /// Whether `id` of this kind is in My view (tracks). Everything is, in Portfolio view.
    pub fn shows(&self, kind: Kind, id: Uuid) -> bool {
        if !self.is_mine() {
            return true;
        }
        self.scope.with(|s| {
            s.as_ref().is_some_and(|s| match kind {
                Kind::Task => s.tasks.contains_key(&id),
                Kind::Project => s.projects.contains_key(&id),
                Kind::Objective => s.objectives.contains_key(&id),
                Kind::Person => s.people.contains_key(&id),
            })
        })
    }

    /// The chip's words for an item in My view, if it is in it (tracks).
    pub fn chip(&self, kind: Kind, id: Uuid) -> Option<String> {
        if !self.is_mine() {
            return None;
        }
        let names = self.names;
        let name_of = move |via: Uuid| names.with(|n| n.get(&via).cloned().unwrap_or_default());
        self.scope.with(|s| {
            let s = s.as_ref()?;
            let (list, own) = match kind {
                Kind::Task => (&s.tasks, None),
                Kind::Project => (&s.projects, s.project_tasks.get(&id).copied()),
                Kind::Objective => (&s.objectives, None),
                Kind::Person => (&s.people, None),
            };
            chip_text(list.get(&id)?, own, &name_of)
        })
    }
}

/// What kind of item a chip is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Task,
    Project,
    Objective,
    Person,
}

/// The ambient context, when there is one (a few screens render in tests without it).
pub fn current() -> Option<Orientation> {
    use_context::<Orientation>()
}

// ------------------------------------------------------------------ the pieces

/// The title bar's button: the view you are in, with its glyph; click it or press `v`.
#[component]
pub fn ViewToggle() -> impl IntoView {
    let o = expect_context::<Orientation>();
    let toasts = expect_context::<crate::state::Toasts>();
    view! {
        <button class="mr-2 inline-flex h-6 items-center gap-1.5 self-center rounded-sm px-2 text-[12px] text-muted \
                       hover:bg-hover hover:text-fg disabled:opacity-50"
                aria-label="Switch between Portfolio view and My view"
                aria-pressed=move || (o.view() == View::Mine).to_string()
                title=move || toggle_title(o.view(), o.can_be_mine())
                disabled=move || !o.can_be_mine()
                on:click=move |_| {
                    if !o.toggle() {
                        toasts.info("Set who you are in People first.".to_owned());
                    }
                }>
            <Icon name=if o.view() == View::Mine { "me" } else { "north" } size="h-3.5 w-3.5" />
            <span>{move || toggle_label(o.view())}</span>
        </button>
    }
}

/// The slim line under a map-like screen's header in My view, with the way back.
#[component]
pub fn MineBanner() -> impl IntoView {
    let o = expect_context::<Orientation>();
    view! {
        {move || o.is_mine().then(|| view! {
            <div class="flex items-center gap-3 border-b border-accent/40 bg-accent/10 px-4 py-1 text-[12px] text-accent"
                 role="status">
                <span class="min-w-0 flex-1">{BANNER_TEXT}</span>
                <button class=BUTTON on:click=move |_| o.set(View::Portfolio)>"Show everything"</button>
            </div>
        })}
    }
}

/// Why this item is in My view, as a small muted chip (nothing in Portfolio view).
#[component]
pub fn WhyChip(kind: Kind, id: Uuid) -> impl IntoView {
    view! {
        {move || current().and_then(|o| o.chip(kind, id)).map(|text| view! {
            <span class="shrink-0 rounded-sm border border-line px-1 text-[10px] leading-4 text-muted"
                  title="Why this is in My view">{text}</span>
        })}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(id: Uuid) -> String {
        match id.as_u128() {
            1 => "Pen test".to_owned(),
            _ => String::new(),
        }
    }

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    #[test]
    fn every_reason_has_words() {
        let say = |w| why_text(w, &names);
        assert_eq!(say(Why::Assigned), "assigned to you");
        assert_eq!(say(Why::Owner), "you own it");
        assert_eq!(say(Why::InMyProject), "in your project");
        assert_eq!(say(Why::Reaches { via: id(1) }), "reaches you via Pen test");
        assert_eq!(say(Why::Reaches { via: id(9) }), "reaches you");
        assert_eq!(say(Why::Blocks { via: id(1) }), "blocked by your work");
        assert_eq!(say(Why::Serves), "serves your work");
        assert_eq!(say(Why::Report), "reports to you");
        assert_eq!(say(Why::OnMyProjects), "on your projects");
        assert_eq!(say(Why::Me), "you");
    }

    #[test]
    fn a_chip_says_the_first_reason_or_how_many_of_your_tasks_a_project_holds() {
        let chip = |reasons: &[Why], own| chip_text(reasons, own, &names);
        assert_eq!(
            chip(&[Why::Assigned, Why::InMyProject], None).as_deref(),
            Some("assigned to you")
        );
        assert_eq!(chip(&[Why::Owner], Some(4)).as_deref(), Some("you own it"));
        assert_eq!(chip(&[], Some(3)).as_deref(), Some("3 of your tasks"));
        assert_eq!(chip(&[], Some(1)).as_deref(), Some("one of your tasks"));
        assert_eq!(chip(&[], None), None);
        assert_eq!(
            chip(&[Why::Reaches { via: id(1) }], Some(2)).as_deref(),
            Some("reaches you via Pen test")
        );
    }

    #[test]
    fn the_button_names_the_view_and_the_hover_says_what_pressing_v_does() {
        assert_eq!(toggle_label(View::Portfolio), "Portfolio");
        assert_eq!(toggle_label(View::Mine), "Mine");
        assert_eq!(
            toggle_title(View::Portfolio, true),
            "Portfolio view: everything. Press v to switch to mine view"
        );
        assert!(toggle_title(View::Mine, true).starts_with("My view: everything as it touches you"));
        assert_eq!(
            toggle_title(View::Mine, false),
            "Set who you are in People first"
        );
    }
}
