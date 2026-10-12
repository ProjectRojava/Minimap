//! Fog of war (spec 43, ADR-0019): how stale what is in play has become, shown wherever the item
//! appears. The rules are in `minimap_core::fog`; the screen only looks marks up by id from the one
//! `FogReport` the shell loads, and says what a mark means.
//!
//! Fog is a way of looking, never a colour of alarm: haze and fog fade a row and add a chip
//! (`≋ 19d`) so it is never colour alone, and a *Still accurate* button clears it without a change.

use leptos::{ev, prelude::*, task::spawn_local};
use minimap_types::{
    FogLevel, FogMark, FogReason, FogReport, FogThresholds, NodeRef, NodeType, Uuid,
};

use crate::{
    api,
    components::{form::BUTTON_SOFT, page::Tone},
    state::{finish, DataVersion, Toasts},
};

/// The fog over everything in play, and the thresholds it was made with. Provided once by the
/// shell; it reloads when data changes and keeps the last answer meanwhile, so marks never blink.
#[derive(Clone, Copy)]
pub struct FogCtx {
    report: RwSignal<Option<FogReport>>,
    thresholds: RwSignal<FogThresholds>,
}

/// Put on a screen that must not show fog (This week looking back at a past day: the data cannot
/// be rewound that far, so there is none to show).
#[derive(Clone, Copy)]
pub struct FogHidden;

impl FogCtx {
    pub fn provide() -> Self {
        let version = expect_context::<DataVersion>();
        let report = RwSignal::new(None::<FogReport>);
        let thresholds = RwSignal::new(FogThresholds::default());
        let loaded = LocalResource::new(move || {
            version.track();
            api::get_fog()
        });
        Effect::new(move |_| {
            if let Some(Ok(r)) = loaded.get() {
                report.set(Some(r));
            }
        });
        let settings = LocalResource::new(move || {
            version.track();
            api::get_settings()
        });
        Effect::new(move |_| {
            if let Some(Ok(s)) = settings.get() {
                thresholds.set(s.fog);
            }
        });
        let ctx = Self { report, thresholds };
        provide_context(ctx);
        ctx
    }

    /// The mark of an item, if it is hazy or fogged (tracks, so call it inside a closure).
    pub fn mark(&self, id: Uuid) -> Option<FogMark> {
        self.report
            .with(|r| r.as_ref().and_then(|r| r.mark(id).cloned()))
    }

    /// The share of what is in play that is clear, as a percent; `None` while loading or when
    /// nothing is in play.
    pub fn visibility(&self) -> Option<u8> {
        self.report.with(|r| r.as_ref().and_then(|r| r.visibility))
    }

    /// Days an item stays clear after it was confirmed (its haze threshold).
    pub fn clear_days(&self, node_type: NodeType) -> u32 {
        let t = self.thresholds.get_untracked();
        match node_type {
            NodeType::Project => t.project_haze,
            NodeType::Objective => t.objective_haze,
            _ => t.task_haze,
        }
    }
}

/// The mark to show on `id` here: none when the screen hides fog or the shell has none.
pub fn mark_for(id: Uuid) -> Option<FogMark> {
    if use_context::<FogHidden>().is_some() {
        return None;
    }
    use_context::<FogCtx>().and_then(|ctx| ctx.mark(id))
}

/// The CSS class that fades a row or card (`input.css`).
pub fn fog_class(level: Option<FogLevel>) -> &'static str {
    match level {
        Some(FogLevel::Fog) => "fog-fog",
        Some(FogLevel::Haze) => "fog-haze",
        _ => "",
    }
}

/// "1 day", "19 days".
pub fn days_text(days: u32) -> String {
    if days == 1 {
        "1 day".to_owned()
    } else {
        format!("{days} days")
    }
}

/// The text of the chip: `≋ 19d`.
pub fn chip_text(mark: &FogMark) -> String {
    format!("≋ {}d", mark.age_days)
}

/// What the chip says when hovered.
pub fn fog_hint(mark: &FogMark) -> String {
    let level = match mark.level {
        FogLevel::Fog => "Fog",
        _ => "Haze",
    };
    let ask = match mark.node.node_type {
        NodeType::Task => "Is it still accurate?",
        _ => "Is it still on track?",
    };
    match mark.reason {
        FogReason::OnlyConfirmed => format!(
            "{level}: last confirmed {} ago. {ask}",
            days_text(mark.age_days)
        ),
        FogReason::NoChange => {
            format!("{level}: no change for {}. {ask}", days_text(mark.age_days))
        }
    }
}

/// The toast after *Still accurate*.
pub fn confirmed_message(days: u32) -> String {
    format!(
        "Confirmed. Clear for {}. Press Ctrl/Cmd+Z to undo.",
        days_text(days)
    )
}

/// The Overview figure: `62%`, or a dash when nothing is in play.
pub fn visibility_text(visibility: Option<u8>) -> String {
    visibility.map_or_else(|| "—".to_owned(), |v| format!("{v}%"))
}

/// Under half is a nudge (amber), never an error.
pub fn visibility_tone(visibility: Option<u8>) -> Tone {
    match visibility {
        Some(v) if v < 50 => Tone::Warning,
        _ => Tone::Neutral,
    }
}

/// A node kind fog can apply to.
pub fn can_confirm(node_type: NodeType) -> bool {
    matches!(
        node_type,
        NodeType::Task | NodeType::Project | NodeType::Objective
    )
}

/// Says an item is still accurate, and tells the person.
pub fn confirm(node: NodeRef, days: u32, toasts: Toasts, version: DataVersion) {
    spawn_local(async move {
        if finish(api::confirm_item(node).await, toasts, version).is_some() {
            toasts.info(confirmed_message(days));
        }
    });
}

/// The row-key `c` handler: confirms the item under the cursor (anything fog does not apply to
/// is left alone).
pub fn confirm_row(node: NodeRef) {
    let (toasts, version) = (expect_context::<Toasts>(), expect_context::<DataVersion>());
    let days = use_context::<FogCtx>().map_or(7, |f| f.clear_days(node.node_type));
    if can_confirm(node.node_type) {
        confirm(node, days, toasts, version);
    }
}

/// `≋ 19d`: how long since it was last seen. Always shows with the fade, so fog is never colour
/// alone. The chip stays at full contrast.
#[component]
pub fn FogChip(mark: FogMark) -> impl IntoView {
    let class = match mark.level {
        FogLevel::Fog => {
            "fog-keep inline-block shrink-0 rounded-sm border border-line-strong px-1.5 text-[10px] \
             font-medium leading-4 tabular-nums text-fg"
        }
        _ => {
            "fog-keep inline-block shrink-0 rounded-sm border border-line px-1.5 text-[10px] \
             leading-4 tabular-nums text-muted"
        }
    };
    let hint = fog_hint(&mark);
    view! { <span class=class title=hint.clone() aria-label=hint>{chip_text(&mark)}</span> }
}

/// The chip for `id` on a screen that shows fog, if it has any.
#[component]
pub fn FogChipFor(id: Uuid) -> impl IntoView {
    let ctx = use_context::<FogCtx>();
    let hidden = use_context::<FogHidden>().is_some();
    move || {
        if hidden {
            return None;
        }
        ctx.and_then(|c| c.mark(id))
            .map(|mark| view! { <FogChip mark=mark /> })
    }
}

/// The chip on its own line at the top of a card (a direct child of the card, so the fade leaves
/// it at full contrast). Nothing when the item is clear.
#[component]
pub fn FogLine(id: Uuid) -> impl IntoView {
    let ctx = use_context::<FogCtx>();
    let hidden = use_context::<FogHidden>().is_some();
    move || {
        if hidden {
            return None;
        }
        ctx.and_then(|c| c.mark(id))
            .map(|mark| view! { <div class="fog-keep mb-1.5 flex"><FogChip mark=mark /></div> })
    }
}

/// "Fog 19d" / "Haze 9d": the pill in a detail pane's strip, said in words.
pub fn pill_text(mark: &FogMark) -> String {
    let level = match mark.level {
        FogLevel::Fog => "Fog",
        _ => "Haze",
    };
    format!("{level} {}d", mark.age_days)
}

/// The detail pane's fog: a pill saying how long it has been quiet, and *Still accurate* beside
/// it. Nothing while the item is clear.
#[component]
pub fn FogPill(node: NodeRef) -> impl IntoView {
    let ctx = use_context::<FogCtx>();
    move || {
        ctx.and_then(|c| c.mark(node.id)).map(|mark| {
            view! {
                <span class=Tone::Neutral.chip() title=fog_hint(&mark)>{pill_text(&mark)}</span>
                <StillAccurate node=node />
            }
        })
    }
}

/// *Still accurate*: clears an item's fog without changing it. Only shown while it has fog.
#[component]
pub fn StillAccurate(node: NodeRef) -> impl IntoView {
    let ctx = use_context::<FogCtx>();
    let (toasts, version) = (expect_context::<Toasts>(), expect_context::<DataVersion>());
    let hidden = use_context::<FogHidden>().is_some();
    let foggy = move || !hidden && ctx.is_some_and(|c| c.mark(node.id).is_some());
    let days = move || ctx.map_or(7, |c| c.clear_days(node.node_type));
    let click = move |ev: ev::MouseEvent| {
        // A row behind the button opens its pane on click; this is not that.
        ev.stop_propagation();
        confirm(node, days(), toasts, version);
    };
    view! {
        <Show when=foggy>
            <button type="button" class=BUTTON_SOFT on:click=click
                    title="Say it is still accurate: clears the fog without changing anything">
                "Still accurate"
            </button>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::date;

    fn mark(node_type: NodeType, level: FogLevel, age: u32, reason: FogReason) -> FogMark {
        FogMark {
            node: NodeRef::new(node_type, Uuid::nil()),
            label: "x".into(),
            level,
            age_days: age,
            last_seen: date!(2027 - 03 - 03),
            reason,
        }
    }

    #[test]
    fn the_chip_says_how_old() {
        let m = mark(NodeType::Task, FogLevel::Haze, 9, FogReason::NoChange);
        assert_eq!(chip_text(&m), "≋ 9d");
    }

    #[test]
    fn the_hover_names_the_level_the_age_and_asks_the_right_question() {
        let project = mark(NodeType::Project, FogLevel::Fog, 19, FogReason::NoChange);
        assert_eq!(
            fog_hint(&project),
            "Fog: no change for 19 days. Is it still on track?"
        );
        let task = mark(NodeType::Task, FogLevel::Haze, 8, FogReason::NoChange);
        assert_eq!(
            fog_hint(&task),
            "Haze: no change for 8 days. Is it still accurate?"
        );
        let confirmed = mark(
            NodeType::Objective,
            FogLevel::Haze,
            1,
            FogReason::OnlyConfirmed,
        );
        assert_eq!(
            fog_hint(&confirmed),
            "Haze: last confirmed 1 day ago. Is it still on track?"
        );
    }

    #[test]
    fn the_pill_says_the_level_and_the_age_in_words() {
        let fog = mark(NodeType::Project, FogLevel::Fog, 19, FogReason::NoChange);
        assert_eq!(pill_text(&fog), "Fog 19d");
        let haze = mark(NodeType::Task, FogLevel::Haze, 9, FogReason::NoChange);
        assert_eq!(pill_text(&haze), "Haze 9d");
    }

    #[test]
    fn only_haze_and_fog_fade() {
        assert_eq!(fog_class(None), "");
        assert_eq!(fog_class(Some(FogLevel::Clear)), "");
        assert_eq!(fog_class(Some(FogLevel::Haze)), "fog-haze");
        assert_eq!(fog_class(Some(FogLevel::Fog)), "fog-fog");
    }

    #[test]
    fn confirming_says_how_long_it_stays_clear_and_how_to_undo() {
        assert_eq!(
            confirmed_message(7),
            "Confirmed. Clear for 7 days. Press Ctrl/Cmd+Z to undo."
        );
        assert!(confirmed_message(1).contains("1 day."));
    }

    #[test]
    fn visibility_is_a_dash_when_nothing_is_in_play_and_amber_under_half() {
        assert_eq!(visibility_text(None), "—");
        assert_eq!(visibility_text(Some(62)), "62%");
        assert_eq!(visibility_tone(None), Tone::Neutral);
        assert_eq!(visibility_tone(Some(50)), Tone::Neutral);
        assert_eq!(visibility_tone(Some(49)), Tone::Warning);
    }

    #[test]
    fn fog_applies_to_tasks_projects_and_objectives_only() {
        assert!(can_confirm(NodeType::Task));
        assert!(can_confirm(NodeType::Project));
        assert!(can_confirm(NodeType::Objective));
        assert!(!can_confirm(NodeType::Person));
        assert!(!can_confirm(NodeType::Note));
    }
}
