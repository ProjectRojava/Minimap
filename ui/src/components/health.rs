//! Health marks and wording shared by the Overview and the project and objective panels.
//!
//! The app is monochrome, so a level is a shape first and a colour second: red is a filled
//! dot in the danger colour, amber a half-filled dot, green an empty one, not scored a dash.
//! The word is always available as a tooltip and to screen readers.

use leptos::prelude::*;
use minimap_types::{
    Health, HealthLevel, HealthReason, OverloadedPerson, OverviewCounts, ProjectHealthRow,
    RiskItem, RiskKind,
};

use crate::timeline::day_text;

pub fn level_symbol(l: HealthLevel) -> &'static str {
    match l {
        HealthLevel::Red => "●",
        HealthLevel::Amber => "◐",
        HealthLevel::Green => "○",
        HealthLevel::Idle => "–",
    }
}

pub fn level_label(l: HealthLevel) -> &'static str {
    match l {
        HealthLevel::Red => "Red",
        HealthLevel::Amber => "Amber",
        HealthLevel::Green => "Green",
        HealthLevel::Idle => "Not scored",
    }
}

fn level_class(l: HealthLevel) -> &'static str {
    match l {
        HealthLevel::Red => "text-danger",
        HealthLevel::Amber => "text-fg",
        HealthLevel::Green | HealthLevel::Idle => "text-faint",
    }
}

/// The level as a mark, with its word as tooltip and accessible name.
#[component]
pub fn HealthMark(level: HealthLevel, #[prop(optional)] score: Option<u8>) -> impl IntoView {
    let tip = match score {
        Some(s) if level != HealthLevel::Idle => format!("{} (score {s}/100)", level_label(level)),
        _ => level_label(level).to_owned(),
    };
    view! {
        <span class=format!("inline-block w-4 shrink-0 text-center {}", level_class(level))
              role="img" aria-label=tip.clone() title=tip>
            {level_symbol(level)}
        </span>
    }
}

/// The worst few reasons as one line: "projected 3 working days late (…) · 1 task overdue (…)".
pub fn reasons_text(h: &Health, max: usize) -> String {
    h.reasons
        .iter()
        .take(max)
        .map(|r| r.text.clone())
        .collect::<Vec<_>>()
        .join(" · ")
}

/// "2 red · 3 amber · 8 green" (zeroes left out; "No projects yet" when there are none).
pub fn counts_text(c: &OverviewCounts) -> String {
    let mut parts = Vec::new();
    if c.red > 0 {
        parts.push(format!("{} red", c.red));
    }
    if c.amber > 0 {
        parts.push(format!("{} amber", c.amber));
    }
    if c.green > 0 {
        parts.push(format!("{} green", c.green));
    }
    if c.idle > 0 {
        parts.push(format!("{} not scored", c.idle));
    }
    if parts.is_empty() {
        "No projects yet".to_owned()
    } else {
        parts.join(" · ")
    }
}

pub fn risk_kind_label(k: RiskKind) -> &'static str {
    match k {
        RiskKind::Project => "Project",
        RiskKind::Task => "Task",
    }
}

/// Why a risk ranks where it does: its priority, and where that came from.
pub fn risk_priority_text(r: &RiskItem) -> String {
    match &r.via_objective {
        Some(o) => format!("P{} via {o}", r.priority),
        None => format!("P{}", r.priority),
    }
}

pub fn load_text(p: &OverloadedPerson) -> String {
    format!(
        "{}% of capacity this week · {} task{}",
        p.load_pct.round() as i64,
        p.open_tasks,
        if p.open_tasks == 1 { "" } else { "s" }
    )
}

/// "Fri 2027-03-05 (target 2027-03-12)" for a project row.
pub fn finish_text(p: &ProjectHealthRow) -> String {
    match (p.projected_finish, p.target_date) {
        (Some(f), Some(t)) => format!("{} · target {t}", day_text(f)),
        (Some(f), None) => day_text(f),
        (None, Some(t)) => format!("target {t}"),
        (None, None) => String::new(),
    }
}

/// The reasons as a list, each marked by its own level.
#[component]
pub fn ReasonList(reasons: Vec<HealthReason>) -> impl IntoView {
    view! {
        <ul class="space-y-0.5">
            {reasons.into_iter().map(|r| view! {
                <li class="flex gap-1">
                    <HealthMark level=r.level />
                    <span class=if r.level == HealthLevel::Green || r.level == HealthLevel::Idle { "text-muted" } else { "" }>
                        {r.text}
                    </span>
                </li>
            }).collect_view()}
        </ul>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{NodeRef, NodeSummary, NodeType, Uuid};
    use time::macros::date;

    fn h(reasons: &[(HealthLevel, &str)]) -> Health {
        Health {
            level: HealthLevel::Amber,
            score: 50,
            reasons: reasons
                .iter()
                .map(|(l, t)| HealthReason {
                    level: *l,
                    text: (*t).into(),
                })
                .collect(),
        }
    }

    #[test]
    fn every_level_has_a_distinct_shape_and_a_word() {
        let all = [
            HealthLevel::Red,
            HealthLevel::Amber,
            HealthLevel::Green,
            HealthLevel::Idle,
        ];
        let symbols: std::collections::HashSet<_> = all.iter().map(|l| level_symbol(*l)).collect();
        assert_eq!(symbols.len(), 4);
        assert!(all.iter().all(|l| !level_label(*l).is_empty()));
        assert_eq!(level_label(HealthLevel::Idle), "Not scored");
    }

    #[test]
    fn reasons_are_joined_worst_first_and_capped() {
        let health = h(&[
            (HealthLevel::Red, "late"),
            (HealthLevel::Amber, "overdue"),
            (HealthLevel::Green, "ok"),
        ]);
        assert_eq!(reasons_text(&health, 2), "late · overdue");
        assert_eq!(reasons_text(&health, 9), "late · overdue · ok");
        assert_eq!(reasons_text(&h(&[]), 2), "");
    }

    #[test]
    fn counts_read_naturally() {
        let c = |r, a, g, i| OverviewCounts {
            red: r,
            amber: a,
            green: g,
            idle: i,
        };
        assert_eq!(counts_text(&c(2, 3, 8, 0)), "2 red · 3 amber · 8 green");
        assert_eq!(counts_text(&c(0, 0, 1, 2)), "1 green · 2 not scored");
        assert_eq!(counts_text(&c(0, 0, 0, 0)), "No projects yet");
    }

    #[test]
    fn risk_and_load_wording() {
        let risk = RiskItem {
            kind: RiskKind::Project,
            node: NodeSummary {
                node: NodeRef::new(NodeType::Project, Uuid::nil()),
                label: "P".into(),
                archived: false,
            },
            health: h(&[]),
            risk_score: 80.0,
            priority: 1,
            via_objective: Some("North star".into()),
        };
        assert_eq!(risk_priority_text(&risk), "P1 via North star");
        assert_eq!(
            risk_priority_text(&RiskItem {
                via_objective: None,
                priority: 3,
                ..risk
            }),
            "P3"
        );
        let p = OverloadedPerson {
            person: NodeSummary {
                node: NodeRef::new(NodeType::Person, Uuid::nil()),
                label: "Priya".into(),
                archived: false,
            },
            load_pct: 140.04,
            open_tasks: 1,
        };
        assert_eq!(load_text(&p), "140% of capacity this week · 1 task");
    }

    #[test]
    fn finish_text_shows_the_target_when_there_is_one() {
        let row = |f, t| ProjectHealthRow {
            project: NodeSummary {
                node: NodeRef::new(NodeType::Project, Uuid::nil()),
                label: "P".into(),
                archived: false,
            },
            status: minimap_types::ProjectStatus::Active,
            priority: 3,
            health: h(&[]),
            projected_finish: f,
            target_date: t,
            open_tasks: 1,
            overdue_tasks: 0,
            blocked_tasks: 0,
            unestimated_tasks: 0,
            weight: None,
        };
        assert_eq!(
            finish_text(&row(
                Some(date!(2027 - 03 - 05)),
                Some(date!(2027 - 03 - 12))
            )),
            "Fri 2027-03-05 · target 2027-03-12"
        );
        assert_eq!(
            finish_text(&row(None, Some(date!(2027 - 03 - 12)))),
            "target 2027-03-12"
        );
        assert_eq!(finish_text(&row(None, None)), "");
    }
}
