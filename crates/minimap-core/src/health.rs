//! Health scoring (CLAUDE.md 5.5, spec 15). Pure.
//!
//! A score is 0-100 (100 healthy). Each signal (lateness against the target, the share of tasks
//! blocked or overdue, the share without an estimate) gets its own score from the configurable
//! amber/red thresholds, and the project's score is the **worst signal's**, so the level always
//! matches the signal that caused it: above 60 green, above 25 amber, otherwise red. Reasons
//! come from the signals that fired.

use minimap_types::{
    Date, Health, HealthLevel, HealthReason, HealthThresholds, ProjectStatus, RiskKind,
};

/// Scores at or below this are red / amber (above amber is green).
pub const RED_MAX: u8 = 25;
pub const AMBER_MAX: u8 = 60;

pub fn level_of_score(score: u8) -> HealthLevel {
    if score > AMBER_MAX {
        HealthLevel::Green
    } else if score > RED_MAX {
        HealthLevel::Amber
    } else {
        HealthLevel::Red
    }
}

/// Score of one measurement against its thresholds: green (61-100) below `amber`, amber
/// (26-60) from `amber` up to `red`, red (0-25) from `red`, reaching 0 at twice `red`.
pub fn signal_score(value: f64, amber: f64, red: f64) -> u8 {
    let v = value.max(0.0);
    let score = if v < amber {
        61.0 + 39.0 * (1.0 - v / amber)
    } else if v < red {
        60.0 - 34.0 * (v - amber) / (red - amber)
    } else {
        25.0 - 25.0 * ((v - red) / red).min(1.0)
    };
    score.round().clamp(0.0, 100.0) as u8
}

fn reason(score: u8, text: String) -> HealthReason {
    HealthReason {
        level: level_of_score(score),
        text,
    }
}

fn severity(l: HealthLevel) -> u8 {
    match l {
        HealthLevel::Red => 0,
        HealthLevel::Amber => 1,
        HealthLevel::Green => 2,
        HealthLevel::Idle => 3,
    }
}

fn sorted(mut reasons: Vec<HealthReason>) -> Vec<HealthReason> {
    reasons.sort_by_key(|r| severity(r.level)); // stable: worst first, then as given
    reasons
}

fn idle(why: &str) -> Health {
    Health {
        level: HealthLevel::Idle,
        score: 100,
        reasons: vec![HealthReason {
            level: HealthLevel::Idle,
            text: why.to_owned(),
        }],
    }
}

fn plural(n: u32, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// What a project's health is computed from.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectFacts {
    pub status: ProjectStatus,
    pub open_tasks: u32,
    pub overdue: u32,
    pub blocked: u32,
    /// Blocked or overdue (a task can be both).
    pub risky: u32,
    pub unestimated: u32,
    pub target: Option<Date>,
    pub projected_finish: Option<Date>,
    /// Working days the projected finish is past the target.
    pub late_days: Option<u32>,
    /// Working days to spare before the target when on time.
    pub spare_days: Option<u32>,
}

pub fn project_health(f: &ProjectFacts, t: &HealthThresholds) -> Health {
    match f.status {
        ProjectStatus::Done => return idle("Done"),
        ProjectStatus::Cancelled => return idle("Cancelled"),
        ProjectStatus::Paused => return idle("Paused"),
        _ => {}
    }
    if f.open_tasks == 0 {
        return idle("No open tasks");
    }
    let mut reasons = Vec::new();
    let mut scores = Vec::new();

    // Lateness against the target.
    match (f.target, f.late_days) {
        (Some(target), Some(late)) if late > 0 => {
            let s = signal_score(
                f64::from(late),
                f64::from(t.late_amber_days),
                f64::from(t.late_red_days),
            );
            scores.push(s);
            reasons.push(reason(
                s,
                format!(
                    "projected {} late (target {target})",
                    plural(late, "working day", "working days")
                ),
            ));
        }
        (Some(target), _) => {
            scores.push(100);
            let finish = f
                .projected_finish
                .map_or(String::new(), |d| format!("projected {d}, "));
            let spare = f.spare_days.filter(|d| *d > 0).map_or(String::new(), |d| {
                format!(" ({} to spare)", plural(d, "working day", "working days"))
            });
            reasons.push(HealthReason {
                level: HealthLevel::Green,
                text: format!("on track: {finish}target {target}{spare}"),
            });
        }
        (None, _) => reasons.push(HealthReason {
            level: HealthLevel::Green,
            text: "no target date, so lateness can't be judged".into(),
        }),
    }

    // Blocked or overdue work.
    let pct = f64::from(f.risky) * 100.0 / f64::from(f.open_tasks);
    let s = signal_score(
        pct,
        f64::from(t.risky_amber_pct),
        f64::from(t.risky_red_pct),
    );
    scores.push(s);
    if f.risky > 0 {
        let mut parts = Vec::new();
        if f.overdue > 0 {
            parts.push(format!("{} overdue", plural(f.overdue, "task", "tasks")));
        }
        if f.blocked > 0 {
            parts.push(if f.overdue > 0 {
                format!("{} blocked", f.blocked)
            } else {
                format!("{} blocked", plural(f.blocked, "task", "tasks"))
            });
        }
        reasons.push(reason(
            s,
            format!(
                "{} ({} of {} open)",
                parts.join(", "),
                f.risky,
                f.open_tasks
            ),
        ));
    }

    // Work nobody has sized.
    let pct = f64::from(f.unestimated) * 100.0 / f64::from(f.open_tasks);
    let s = signal_score(
        pct,
        f64::from(t.unestimated_amber_pct),
        f64::from(t.unestimated_red_pct),
    );
    scores.push(s);
    if f.unestimated > 0 {
        reasons.push(reason(
            s,
            format!(
                "{} of {} open tasks have no estimate",
                f.unestimated, f.open_tasks
            ),
        ));
    }

    let score = scores.into_iter().min().unwrap_or(100);
    Health {
        level: level_of_score(score),
        score,
        reasons: sorted(reasons),
    }
}

/// An open task's health: overdue is red, blocked is amber (both: red). Fixed rules; a task
/// has no thresholds to tune.
pub fn task_health(due: Option<Date>, overdue: bool, blocked: bool) -> Health {
    let mut reasons = Vec::new();
    let mut score = 100;
    if overdue {
        score = RED_MAX;
        reasons.push(HealthReason {
            level: HealthLevel::Red,
            text: due.map_or("overdue".to_owned(), |d| format!("overdue (due {d})")),
        });
    }
    if blocked {
        score = score.min(AMBER_MAX);
        reasons.push(HealthReason {
            level: HealthLevel::Amber,
            text: "blocked".into(),
        });
    }
    if reasons.is_empty() {
        reasons.push(HealthReason {
            level: HealthLevel::Green,
            text: "on track".into(),
        });
    }
    Health {
        level: level_of_score(score),
        score,
        reasons,
    }
}

/// One thing contributing to an objective.
#[derive(Debug, Clone, PartialEq)]
pub struct Contribution {
    pub name: String,
    /// `contributes_to.weight` (default 1).
    pub weight: f64,
    pub score: u8,
    pub lead_reason: Option<String>,
}

/// The objective's own target date against the latest finish of what feeds it.
#[derive(Debug, Clone, PartialEq)]
pub struct TargetFacts {
    pub target: Date,
    pub finish: Date,
    pub late_days: Option<u32>,
}

/// An ongoing objective's review that is overdue.
pub struct ReviewFacts {
    pub due: Date,
    pub overdue_days: u32,
    pub every_days: u32,
}

/// Weighted roll-up of the contributors' scores (so a heavier project counts for more), held
/// to the same lateness thresholds if the objective has a target. **One red contributor caps
/// the objective at amber**, so an average can't hide a project that is in trouble.
pub fn objective_health(
    marked_done: bool,
    contributions: &[Contribution],
    target: Option<&TargetFacts>,
    review: Option<&ReviewFacts>,
    t: &HealthThresholds,
) -> Health {
    if marked_done {
        return idle("Marked done");
    }
    let weight: f64 = contributions.iter().map(|c| c.weight).sum();
    if contributions.is_empty() || weight <= 0.0 {
        return idle("Nothing active contributes to it");
    }
    let avg = contributions
        .iter()
        .map(|c| c.weight * f64::from(c.score))
        .sum::<f64>()
        / weight;
    let mut score = avg.round().clamp(0.0, 100.0) as u8;
    let any_red = contributions
        .iter()
        .any(|c| level_of_score(c.score) == HealthLevel::Red);
    if any_red {
        score = score.min(AMBER_MAX);
    }
    let mut reasons = Vec::new();

    // An ongoing objective has no deadline; a review that has not happened in time is the
    // overdue of perpetual work, and holds it at amber.
    if let Some(rf) = review {
        score = score.min(AMBER_MAX);
        reasons.push(reason(
            score,
            format!(
                "review overdue by {} (due {}, every {})",
                plural(rf.overdue_days, "day", "days"),
                rf.due,
                plural(rf.every_days, "day", "days")
            ),
        ));
    }

    if let Some(tf) = target {
        match tf.late_days {
            Some(late) if late > 0 => {
                let s = signal_score(
                    f64::from(late),
                    f64::from(t.late_amber_days),
                    f64::from(t.late_red_days),
                );
                score = score.min(s);
                reasons.push(reason(
                    s,
                    format!(
                        "projected {} after its target {} (finishes {})",
                        plural(late, "working day", "working days"),
                        tf.target,
                        tf.finish
                    ),
                ));
            }
            _ => reasons.push(HealthReason {
                level: HealthLevel::Green,
                text: format!("on track: finishes {}, target {}", tf.finish, tf.target),
            }),
        }
    }

    let mut weak: Vec<&Contribution> = contributions
        .iter()
        .filter(|c| level_of_score(c.score) != HealthLevel::Green)
        .collect();
    weak.sort_by_key(|c| c.score);
    for c in weak.iter().take(3) {
        let level = level_of_score(c.score);
        let word = if level == HealthLevel::Red {
            "red"
        } else {
            "amber"
        };
        reasons.push(HealthReason {
            level,
            text: match &c.lead_reason {
                Some(r) => format!("{} is {word}: {r}", c.name),
                None => format!("{} is {word}", c.name),
            },
        });
    }
    if weak.is_empty() {
        reasons.push(HealthReason {
            level: HealthLevel::Green,
            text: format!(
                "all {} on track",
                plural(contributions.len() as u32, "contributor", "contributors")
            ),
        });
    }
    Health {
        level: level_of_score(score),
        score,
        reasons: sorted(reasons),
    }
}

/// How much a priority (1 = highest) counts when ranking risks.
pub fn priority_weight(priority: u8) -> f64 {
    match priority {
        0 | 1 => 1.5,
        2 => 1.25,
        3 => 1.0,
        4 => 0.8,
        _ => 0.6,
    }
}

/// A single task is a smaller risk than a whole project.
pub fn scope_weight(kind: RiskKind) -> f64 {
    match kind {
        RiskKind::Project => 1.0,
        RiskKind::Task => 0.6,
    }
}

/// `(100 - health score) x priority weight x scope weight`, to one decimal.
pub fn risk_score(health_score: u8, priority: u8, kind: RiskKind) -> f64 {
    let raw =
        f64::from(100 - health_score.min(100)) * priority_weight(priority) * scope_weight(kind);
    (raw * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::date;

    fn t() -> HealthThresholds {
        HealthThresholds::default() // late 1/5 days, risky 15/35 %, unestimated 50/80 %
    }

    fn facts() -> ProjectFacts {
        ProjectFacts {
            status: ProjectStatus::Active,
            open_tasks: 10,
            overdue: 0,
            blocked: 0,
            risky: 0,
            unestimated: 0,
            target: Some(date!(2027 - 03 - 31)),
            projected_finish: Some(date!(2027 - 03 - 26)),
            late_days: None,
            spare_days: Some(3),
        }
    }

    // ------------------------------------------------------------- signals

    #[test]
    fn signal_scores_are_banded_at_the_thresholds() {
        // Healthy -> 100, just under amber -> still green, at amber -> amber, at red -> red.
        assert_eq!(signal_score(0.0, 1.0, 5.0), 100);
        assert_eq!(
            level_of_score(signal_score(0.999, 1.0, 5.0)),
            HealthLevel::Green
        );
        assert_eq!(signal_score(1.0, 1.0, 5.0), 60);
        assert_eq!(
            level_of_score(signal_score(1.0, 1.0, 5.0)),
            HealthLevel::Amber
        );
        assert_eq!(
            level_of_score(signal_score(4.9, 1.0, 5.0)),
            HealthLevel::Amber
        );
        assert_eq!(signal_score(5.0, 1.0, 5.0), 25);
        assert_eq!(
            level_of_score(signal_score(5.0, 1.0, 5.0)),
            HealthLevel::Red
        );
        // Twice the red threshold bottoms out; more doesn't go below zero.
        assert_eq!(signal_score(10.0, 1.0, 5.0), 0);
        assert_eq!(signal_score(1000.0, 1.0, 5.0), 0);
        // Amber equal to red: no amber band.
        assert_eq!(
            level_of_score(signal_score(2.9, 3.0, 3.0)),
            HealthLevel::Green
        );
        assert_eq!(
            level_of_score(signal_score(3.0, 3.0, 3.0)),
            HealthLevel::Red
        );
    }

    #[test]
    fn levels_follow_scores_exactly() {
        for s in 0..=100u8 {
            let want = if s > 60 {
                HealthLevel::Green
            } else if s > 25 {
                HealthLevel::Amber
            } else {
                HealthLevel::Red
            };
            assert_eq!(level_of_score(s), want, "{s}");
        }
    }

    // ------------------------------------------------------------- projects

    #[test]
    fn a_healthy_project_is_green_and_says_why() {
        let h = project_health(&facts(), &t());
        assert_eq!((h.level, h.score), (HealthLevel::Green, 100));
        assert_eq!(h.reasons.len(), 1);
        assert_eq!(
            h.reasons[0].text,
            "on track: projected 2027-03-26, target 2027-03-31 (3 working days to spare)"
        );
    }

    #[test]
    fn lateness_is_amber_then_red_with_the_numbers_in_the_reason() {
        let mut f = facts();
        f.late_days = Some(2);
        let h = project_health(&f, &t());
        assert_eq!(h.level, HealthLevel::Amber);
        assert_eq!(
            h.reasons[0].text,
            "projected 2 working days late (target 2027-03-31)"
        );
        assert_eq!(h.reasons[0].level, HealthLevel::Amber);
        f.late_days = Some(6);
        let h = project_health(&f, &t());
        assert_eq!(h.level, HealthLevel::Red);
        assert_eq!(
            h.reasons[0].text,
            "projected 6 working days late (target 2027-03-31)"
        );
        f.late_days = Some(1);
        assert_eq!(
            project_health(&f, &t()).reasons[0].text,
            "projected 1 working day late (target 2027-03-31)"
        );
    }

    #[test]
    fn overdue_and_blocked_work_count_once_together() {
        let mut f = facts();
        f.overdue = 3;
        f.blocked = 1;
        f.risky = 3; // one task is both
        let h = project_health(&f, &t());
        // 3 of 10 = 30%: amber (15-35).
        assert_eq!(h.level, HealthLevel::Amber);
        let r = h
            .reasons
            .iter()
            .find(|r| r.text.contains("overdue"))
            .unwrap();
        assert_eq!(r.text, "3 tasks overdue, 1 blocked (3 of 10 open)");
        f.risky = 4;
        f.blocked = 2;
        assert_eq!(project_health(&f, &t()).level, HealthLevel::Red);
        // Only blocked reads naturally too.
        let mut b = facts();
        b.blocked = 1;
        b.risky = 1;
        let h = project_health(&b, &t());
        assert!(h
            .reasons
            .iter()
            .any(|r| r.text == "1 task blocked (1 of 10 open)"));
        assert_eq!(h.level, HealthLevel::Green, "10% is under the amber line");
    }

    #[test]
    fn unestimated_work_is_the_gentlest_signal() {
        let mut f = facts();
        f.unestimated = 6;
        let h = project_health(&f, &t());
        assert_eq!(h.level, HealthLevel::Amber);
        assert!(h
            .reasons
            .iter()
            .any(|r| r.text == "6 of 10 open tasks have no estimate"));
        f.unestimated = 9;
        assert_eq!(project_health(&f, &t()).level, HealthLevel::Red);
        f.unestimated = 4;
        assert_eq!(project_health(&f, &t()).level, HealthLevel::Green);
    }

    #[test]
    fn the_worst_signal_decides_and_reasons_are_worst_first() {
        let mut f = facts();
        f.late_days = Some(7);
        f.overdue = 2;
        f.risky = 2;
        f.unestimated = 6;
        let h = project_health(&f, &t());
        assert_eq!(h.level, HealthLevel::Red);
        let levels: Vec<HealthLevel> = h.reasons.iter().map(|r| r.level).collect();
        assert_eq!(
            levels,
            [HealthLevel::Red, HealthLevel::Amber, HealthLevel::Amber]
        );
        assert!(h.reasons[0].text.contains("late"));
        // Level and score agree.
        assert_eq!(level_of_score(h.score), h.level);
    }

    #[test]
    fn thresholds_are_configurable() {
        let mut f = facts();
        f.late_days = Some(2);
        // Default: amber. Tolerant thresholds: still green. Strict ones: red.
        assert_eq!(project_health(&f, &t()).level, HealthLevel::Amber);
        let lax = HealthThresholds {
            late_amber_days: 3,
            late_red_days: 10,
            ..t()
        };
        assert_eq!(project_health(&f, &lax).level, HealthLevel::Green);
        let strict = HealthThresholds {
            late_amber_days: 1,
            late_red_days: 2,
            ..t()
        };
        assert_eq!(project_health(&f, &strict).level, HealthLevel::Red);
        // The reasons still name the lateness when it is under the amber line.
        assert!(project_health(&f, &lax).reasons[0]
            .text
            .contains("2 working days late"));
    }

    #[test]
    fn no_target_means_no_lateness_judgement() {
        let mut f = facts();
        f.target = None;
        f.projected_finish = None;
        let h = project_health(&f, &t());
        assert_eq!(h.level, HealthLevel::Green);
        assert!(h.reasons[0].text.contains("no target date"));
    }

    #[test]
    fn projects_that_are_not_active_work_are_not_scored() {
        for (status, why) in [
            (ProjectStatus::Done, "Done"),
            (ProjectStatus::Cancelled, "Cancelled"),
            (ProjectStatus::Paused, "Paused"),
        ] {
            let mut f = facts();
            f.status = status;
            f.late_days = Some(50);
            let h = project_health(&f, &t());
            assert_eq!((h.level, h.score), (HealthLevel::Idle, 100));
            assert_eq!(h.reasons[0].text, why);
        }
        let mut f = facts();
        f.open_tasks = 0;
        assert_eq!(project_health(&f, &t()).reasons[0].text, "No open tasks");
        // A planned project is scored like an active one.
        f.open_tasks = 2;
        f.status = ProjectStatus::Planned;
        assert_ne!(project_health(&f, &t()).level, HealthLevel::Idle);
    }

    // ---------------------------------------------------------------- tasks

    #[test]
    fn task_health_is_overdue_red_blocked_amber() {
        let due = Some(date!(2027 - 03 - 01));
        let h = task_health(due, true, false);
        assert_eq!((h.level, h.score), (HealthLevel::Red, 25));
        assert_eq!(h.reasons[0].text, "overdue (due 2027-03-01)");
        assert_eq!(task_health(None, false, true).level, HealthLevel::Amber);
        let both = task_health(due, true, true);
        assert_eq!((both.level, both.reasons.len()), (HealthLevel::Red, 2));
        assert_eq!(task_health(None, false, false).level, HealthLevel::Green);
    }

    // ----------------------------------------------------------- objectives

    fn c(name: &str, weight: f64, score: u8) -> Contribution {
        Contribution {
            name: name.into(),
            weight,
            score,
            lead_reason: Some("projected 6 working days late".into()),
        }
    }

    #[test]
    fn objectives_roll_up_by_weight() {
        // Heavy healthy project and light amber one: the average stays green.
        let h = objective_health(
            false,
            &[c("A", 0.9, 100), c("B", 0.1, 50)],
            None,
            None,
            &t(),
        );
        assert_eq!(h.level, HealthLevel::Green);
        assert_eq!(h.score, 95);
        // Same two with the weights swapped: amber.
        let h = objective_health(
            false,
            &[c("A", 0.1, 100), c("B", 0.9, 50)],
            None,
            None,
            &t(),
        );
        assert_eq!((h.level, h.score), (HealthLevel::Amber, 55));
        // Equal weights by default.
        let h = objective_health(
            false,
            &[c("A", 1.0, 100), c("B", 1.0, 80)],
            None,
            None,
            &t(),
        );
        assert_eq!(h.score, 90);
    }

    #[test]
    fn an_overdue_review_holds_an_ongoing_objective_at_amber_and_says_why() {
        let healthy = [c("Maintenance", 1.0, 100)];
        let on_time = objective_health(false, &healthy, None, None, &t());
        assert_eq!(on_time.level, HealthLevel::Green);
        let review = ReviewFacts {
            due: date!(2027 - 02 - 19),
            overdue_days: 12,
            every_days: 30,
        };
        let h = objective_health(false, &healthy, None, Some(&review), &t());
        assert_eq!(h.level, HealthLevel::Amber);
        assert_eq!(
            h.reasons[0].text,
            "review overdue by 12 days (due 2027-02-19, every 30 days)"
        );
        // Nothing active contributes: it is idle either way (the row still shows the review).
        assert_eq!(
            objective_health(false, &[], None, Some(&review), &t()).level,
            HealthLevel::Idle
        );
    }

    #[test]
    fn one_red_contributor_caps_the_objective_at_amber() {
        let h = objective_health(
            false,
            &[c("Good", 1.0, 100), c("Bad", 1.0, 20), c("Fine", 1.0, 100)],
            None,
            None,
            &t(),
        );
        // The average is 73 (green) but a red project is in there.
        assert_eq!(h.level, HealthLevel::Amber);
        assert_eq!(h.score, 60);
        assert_eq!(
            h.reasons[0].text,
            "Bad is red: projected 6 working days late"
        );
        // An amber contributor alone doesn't cap it.
        let h = objective_health(
            false,
            &[c("Good", 1.0, 100), c("Meh", 1.0, 55)],
            None,
            None,
            &t(),
        );
        assert_eq!(h.level, HealthLevel::Green);
        assert_eq!(h.reasons[0].level, HealthLevel::Amber);
    }

    #[test]
    fn an_objective_target_is_judged_like_a_project_target() {
        let tf = |late: Option<u32>| TargetFacts {
            target: date!(2027 - 03 - 31),
            finish: date!(2027 - 04 - 08),
            late_days: late,
        };
        let ok = objective_health(false, &[c("A", 1.0, 100)], Some(&tf(None)), None, &t());
        assert_eq!(ok.level, HealthLevel::Green);
        assert!(ok.reasons.iter().any(|r| r.text.starts_with("on track")));
        let late = objective_health(false, &[c("A", 1.0, 100)], Some(&tf(Some(6))), None, &t());
        assert_eq!(late.level, HealthLevel::Red);
        assert!(late.reasons[0]
            .text
            .contains("6 working days after its target 2027-03-31"));
    }

    #[test]
    fn objectives_without_active_work_or_marked_done_are_not_scored() {
        let h = objective_health(true, &[c("A", 1.0, 10)], None, None, &t());
        assert_eq!(
            (h.level, h.reasons[0].text.as_str()),
            (HealthLevel::Idle, "Marked done")
        );
        assert_eq!(
            objective_health(false, &[], None, None, &t()).level,
            HealthLevel::Idle
        );
        assert_eq!(
            objective_health(false, &[c("A", 0.0, 10)], None, None, &t()).level,
            HealthLevel::Idle
        );
        let all_good = objective_health(
            false,
            &[c("A", 1.0, 100), c("B", 1.0, 90)],
            None,
            None,
            &t(),
        );
        assert_eq!(all_good.reasons[0].text, "all 2 contributors on track");
    }

    // ----------------------------------------------------------------- risks

    #[test]
    fn risk_weighs_how_bad_by_how_much_it_matters() {
        // Same health, higher priority -> more urgent.
        assert!(risk_score(20, 1, RiskKind::Project) > risk_score(20, 3, RiskKind::Project));
        assert!(risk_score(20, 3, RiskKind::Project) > risk_score(20, 5, RiskKind::Project));
        // Same priority, worse health -> more urgent.
        assert!(risk_score(10, 3, RiskKind::Project) > risk_score(50, 3, RiskKind::Project));
        assert_eq!(risk_score(100, 1, RiskKind::Project), 0.0);
        assert_eq!(risk_score(20, 3, RiskKind::Project), 80.0);
        assert_eq!(risk_score(20, 1, RiskKind::Project), 120.0);
        // A task counts for less than a project of the same health and priority.
        assert_eq!(risk_score(25, 1, RiskKind::Task), 67.5);
        assert!(risk_score(25, 1, RiskKind::Task) < risk_score(25, 3, RiskKind::Project));
        // ...but a top-priority red task beats a low-priority amber project.
        assert!(risk_score(25, 1, RiskKind::Task) > risk_score(55, 5, RiskKind::Project));
    }

    #[test]
    fn priority_weights_fall_with_the_number() {
        let w: Vec<f64> = (1..=5).map(priority_weight).collect();
        assert!(w.windows(2).all(|p| p[0] > p[1]));
        assert_eq!(priority_weight(3), 1.0);
    }
}
