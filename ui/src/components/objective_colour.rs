//! Objective colours (ADR-0012): every objective gets its own hue, and the projects and tasks
//! that serve it wear it, so what belongs together is visible at a glance.
//!
//! The hue is *derived*, never stored: the active objectives are ranked by id (UUID v7 ids sort
//! by creation time) and rank `n` takes the `n`-th hue of a hand-picked palette whose first eight
//! hues are at least 35 degrees apart. The same data therefore gives the same colours on every
//! device. Lightness and saturation come from the theme's light/dark kind (`input.css`), so a hue
//! stays readable on every theme. Colour is never the only signal: the objective's name is
//! always shown next to it (chips) or in a tooltip (dots and edges).

use std::collections::HashMap;

use leptos::prelude::*;
use minimap_types::{NodeSummary, NodeType, ProjectFilter, ProjectLayout, Uuid};

use crate::{api, state::DataVersion};

/// The first eight objectives, in the order they are handed out: far apart in hue, and kept
/// away from red and amber, which already mean "overdue" and "needs attention".
const PALETTE: [u16; 8] = [215, 25, 145, 285, 340, 75, 180, 250];

/// Saturation and lightness (percent) of an objective colour on dark and light themes. These are
/// the values in `input.css`; a test keeps the two in step.
#[cfg(test)]
pub const DARK_SL: (u16, u16) = (55, 70);
#[cfg(test)]
pub const LIGHT_SL: (u16, u16) = (55, 32);

/// The hue (0-359) of the objective at `rank` (0 = the oldest active objective). Beyond the
/// palette the hues keep spreading by the golden angle, which stays as far apart as it can.
pub fn hue_for(rank: usize) -> u16 {
    match PALETTE.get(rank) {
        Some(h) => *h,
        None => ((100.0 + (rank - PALETTE.len()) as f64 * 137.508) % 360.0) as u16,
    }
}

/// Hues for these objectives: ranked by id, so creation order.
pub fn assign(ids: &[Uuid]) -> HashMap<Uuid, u16> {
    let mut sorted = ids.to_vec();
    sorted.sort();
    sorted.dedup();
    sorted
        .into_iter()
        .enumerate()
        .map(|(rank, id)| (id, hue_for(rank)))
        .collect()
}

/// The inline style that gives an element (a chip, dot, edge or card) its objective hue.
fn style_of(hue: Option<u16>) -> String {
    hue.map(|h| format!("--obj-h: {h}")).unwrap_or_default()
}

/// What every screen needs to colour things: each objective's hue, and which objectives each
/// project serves (a task wears its project's). Provided once by the shell.
#[derive(Clone, Copy)]
pub struct ObjectiveColours {
    hues: Memo<HashMap<Uuid, u16>>,
    by_project: Memo<HashMap<Uuid, Vec<NodeSummary>>>,
}

impl ObjectiveColours {
    /// Loads the objectives and projects (again whenever data changes) and provides the result
    /// as a context.
    pub fn provide() -> Self {
        let version = expect_context::<DataVersion>();
        let objectives = LocalResource::new(move || {
            version.track();
            api::list_node_summaries(NodeType::Objective)
        });
        let projects = LocalResource::new(move || {
            version.track();
            api::list_projects(ProjectFilter::default(), ProjectLayout::Board)
        });
        // Keep the last answer while reloading, so colours never blink out after an edit.
        let hues = Memo::new(
            move |last: Option<&HashMap<Uuid, u16>>| match objectives.get() {
                Some(Ok(list)) => assign(&list.iter().map(|o| o.node.id).collect::<Vec<_>>()),
                _ => last.cloned().unwrap_or_default(),
            },
        );
        let by_project =
            Memo::new(
                move |last: Option<&HashMap<Uuid, Vec<NodeSummary>>>| match projects.get() {
                    Some(Ok(groups)) => groups
                        .into_iter()
                        .flat_map(|g| g.rows)
                        .map(|r| (r.project.id, r.objectives))
                        .collect(),
                    _ => last.cloned().unwrap_or_default(),
                },
            );
        let colours = Self { hues, by_project };
        provide_context(colours);
        colours
    }

    /// The hue of an objective (tracks, so call it inside a closure that redraws).
    pub fn hue(&self, objective: Uuid) -> Option<u16> {
        self.hues.with(|h| h.get(&objective).copied())
    }

    /// The hue of the first of these objectives (the one that colours a row's edge).
    pub fn first_hue(&self, objectives: &[NodeSummary]) -> Option<u16> {
        objectives.first().and_then(|o| self.hue(o.node.id))
    }

    /// The objectives a project serves (tracks).
    pub fn of_project(&self, project: Option<Uuid>) -> Vec<NodeSummary> {
        project
            .and_then(|p| self.by_project.with(|m| m.get(&p).cloned()))
            .unwrap_or_default()
    }

    /// The edge hue of one objective as a signal, for `NodeRow`.
    pub fn objective_edge(&self, objective: Uuid) -> Signal<Option<u16>> {
        let this = *self;
        Signal::derive(move || this.hue(objective))
    }
}

/// The colours, from the shell.
pub fn use_objective_colours() -> ObjectiveColours {
    expect_context::<ObjectiveColours>()
}

/// A small dot in the objective's colour (name in the tooltip).
#[component]
pub fn ObjectiveDot(objective: NodeSummary) -> impl IntoView {
    let colours = use_objective_colours();
    let id = objective.node.id;
    view! {
        <span class="obj-dot" style=move || style_of(colours.hue(id))
              title=format!("Objective: {}", objective.label) />
    }
}

/// The objective as a tinted chip: its colour and its name.
#[component]
pub fn ObjectiveChip(objective: NodeSummary) -> impl IntoView {
    let colours = use_objective_colours();
    let id = objective.node.id;
    let title = format!("Objective: {}", objective.label);
    view! {
        <span class="obj-chip" style=move || style_of(colours.hue(id)) title=title>
            <span class="obj-dot" />
            <span>{objective.label}</span>
        </span>
    }
}

/// The chips of the objectives something serves, side by side. With `max` set, the rest become a
/// "+n" (the tooltip lists them all).
#[component]
pub fn ObjectiveChips(
    objectives: Vec<NodeSummary>,
    /// Chips to show before "+n"; 0 shows all of them.
    #[prop(default = 0)]
    max: usize,
) -> impl IntoView {
    let all = objectives
        .iter()
        .map(|o| o.label.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let (shown, rest) = split_chips(objectives, max);
    let more = (!rest.is_empty()).then(|| {
        view! {
            <span class="shrink-0 text-[11px] text-muted" title=all>{format!("+{}", rest.len())}</span>
        }
    });
    view! {
        <span class="flex min-w-0 flex-wrap items-center gap-1">
            {shown.into_iter().map(|o| view! { <ObjectiveChip objective=o /> }).collect_view()}
            {more}
        </span>
    }
}

/// The chips to show and the ones folded into "+n".
fn split_chips(mut all: Vec<NodeSummary>, max: usize) -> (Vec<NodeSummary>, Vec<NodeSummary>) {
    if max == 0 || all.len() <= max {
        return (all, Vec::new());
    }
    let rest = all.split_off(max);
    (all, rest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::themes::{self, Kind};

    fn gap(a: u16, b: u16) -> u16 {
        let d = a.abs_diff(b);
        d.min(360 - d)
    }

    #[test]
    fn the_first_eight_objectives_are_far_apart_in_hue() {
        let hues: Vec<u16> = (0..8).map(hue_for).collect();
        for (i, a) in hues.iter().enumerate() {
            for b in &hues[i + 1..] {
                assert!(gap(*a, *b) >= 35, "{a} and {b} are too alike");
            }
        }
    }

    #[test]
    fn the_second_and_third_pick_the_most_different_colours() {
        // With only two or three objectives the choice should be generous.
        assert!(gap(hue_for(0), hue_for(1)) >= 150);
        assert!(gap(hue_for(0), hue_for(2)) >= 70 && gap(hue_for(1), hue_for(2)) >= 70);
    }

    #[test]
    fn the_hue_stays_on_the_wheel_and_keeps_spreading_past_the_palette() {
        let more: Vec<u16> = (8..40).map(hue_for).collect();
        assert!(more.iter().all(|h| *h < 360));
        let distinct: std::collections::HashSet<_> = more.iter().collect();
        assert_eq!(distinct.len(), more.len());
    }

    #[test]
    fn colours_follow_creation_order_and_ignore_the_order_given() {
        let (a, b, c) = (Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3));
        let one = assign(&[c, a, b]);
        let two = assign(&[a, b, c]);
        assert_eq!(one, two);
        assert_eq!(one[&a], hue_for(0));
        assert_eq!(one[&b], hue_for(1));
        assert_eq!(one[&c], hue_for(2));
        assert_eq!(assign(&[a, a]).len(), 1);
        assert!(assign(&[]).is_empty());
    }

    #[test]
    fn the_style_is_empty_until_the_hue_is_known() {
        assert_eq!(style_of(None), "");
        assert_eq!(style_of(Some(215)), "--obj-h: 215");
    }

    fn summary(label: &str) -> NodeSummary {
        NodeSummary {
            node: minimap_types::NodeRef::new(NodeType::Objective, Uuid::nil()),
            label: label.to_owned(),
            archived: false,
        }
    }

    #[test]
    fn extra_chips_fold_into_a_count() {
        let all = vec![summary("a"), summary("b"), summary("c")];
        let (shown, rest) = split_chips(all.clone(), 1);
        assert_eq!((shown.len(), rest.len()), (1, 2));
        let (shown, rest) = split_chips(all.clone(), 0);
        assert_eq!((shown.len(), rest.len()), (3, 0));
        let (shown, rest) = split_chips(all, 3);
        assert_eq!((shown.len(), rest.len()), (3, 0));
    }

    /// `#rrggbb` of an HSL colour (percent saturation and lightness).
    fn hex(h: u16, s: u16, l: u16) -> String {
        let (h, s, l) = (f64::from(h), f64::from(s) / 100.0, f64::from(l) / 100.0);
        let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
        let m = l - c / 2.0;
        let (r, g, b) = match (h / 60.0) as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        let byte = |v: f64| ((v + m) * 255.0).round() as u8;
        format!("#{:02x}{:02x}{:02x}", byte(r), byte(g), byte(b))
    }

    #[test]
    fn every_hue_is_visible_on_every_theme() {
        // Dots, edges and chip borders are not text, so the bar is 3:1 (WCAG non-text) on the
        // two surfaces they sit on.
        for theme in themes::ALL.iter() {
            let (s, l) = if theme.kind == Kind::Dark {
                DARK_SL
            } else {
                LIGHT_SL
            };
            for h in (0..360).step_by(5) {
                let colour = hex(h, s, l);
                for surface in [theme.tokens.canvas, theme.tokens.panel] {
                    let ratio = themes::contrast(&colour, surface);
                    assert!(
                        ratio >= 3.0,
                        "hue {h} on {} ({surface}) is only {ratio:.2}:1",
                        theme.id
                    );
                }
            }
        }
    }

    #[test]
    fn the_stylesheet_uses_the_same_saturation_and_lightness() {
        let css = include_str!("../../style/input.css");
        assert!(css.contains(&format!(
            "--obj-s: {}%; --obj-l: {}%;",
            DARK_SL.0, DARK_SL.1
        )));
        assert!(css.contains(&format!(
            "--obj-s: {}%; --obj-l: {}%;",
            LIGHT_SL.0, LIGHT_SL.1
        )));
    }
}
