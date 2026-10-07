//! The weekly status report (spec 19): a Markdown document for a board or executive audience,
//! made from a [`WeeklyReview`] and a user-editable template. Pure.
//!
//! A template is Markdown with `{{placeholders}}` (`minimap_types::REPORT_PLACEHOLDERS`). Each
//! section placeholder expands to the section's body without a heading, so the template decides
//! the headings, their order and which sections appear at all.

use std::ops::Range;

use minimap_types::{
    Date, Health, HealthLevel, NodeType, ReviewSlip, RiskKind, SlipKind, WaitingOnRow,
    WeeklyReview, REPORT_PLACEHOLDERS,
};

/// What `{{title}}` stands for.
pub const TITLE: &str = "Weekly status report";

/// Longest accepted template, in characters.
pub const MAX_TEMPLATE_CHARS: usize = 20_000;

/// Longest list of completed tasks printed; the rest is summarised as a count.
const DONE_LIST_CAP: usize = 15;

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/// "Tue 2 Mar".
fn day(d: Date) -> String {
    format!(
        "{} {} {}",
        DAYS[d.weekday().number_days_from_monday() as usize],
        d.day(),
        MONTHS[d.month() as usize - 1]
    )
}

/// "2 Mar 2027".
fn full(d: Date) -> String {
    format!(
        "{} {} {}",
        d.day(),
        MONTHS[d.month() as usize - 1],
        d.year()
    )
}

/// Makes user text safe inside a Markdown line or table cell: whitespace (including newlines)
/// collapses to single spaces and characters that would start formatting are escaped.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for word in text.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        for c in word.chars() {
            if matches!(
                c,
                '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '|' | '~'
            ) {
                out.push('\\');
            }
            out.push(c);
        }
    }
    out
}

fn bold(text: &str) -> String {
    format!("**{}**", escape(text))
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn level_word(level: HealthLevel) -> &'static str {
    match level {
        HealthLevel::Green => "On track",
        HealthLevel::Amber => "At risk",
        HealthLevel::Red => "Off track",
        HealthLevel::Idle => "Not scored",
    }
}

/// The two worst reasons behind a health verdict, or the first good-news one.
fn why(health: &Health) -> String {
    let bad: Vec<&str> = health
        .reasons
        .iter()
        .filter(|r| r.level != HealthLevel::Green)
        .take(2)
        .map(|r| r.text.as_str())
        .collect();
    if !bad.is_empty() {
        return escape(&bad.join("; "));
    }
    health
        .reasons
        .first()
        .map(|r| escape(&r.text))
        .unwrap_or_default()
}

/// " (Project · Priya)", or nothing.
fn context(project: Option<&str>, who: Option<&str>) -> String {
    let parts: Vec<String> = [project, who].into_iter().flatten().map(escape).collect();
    if parts.is_empty() {
        String::new()
    } else {
        format!(" ({})", parts.join(" · "))
    }
}

fn bullets(lines: Vec<String>, empty: &str) -> String {
    if lines.is_empty() {
        empty.to_owned()
    } else {
        lines.join("\n")
    }
}

// ------------------------------------------------------------------ sections

fn summary(r: &WeeklyReview) -> String {
    let mut levels = Vec::new();
    for (n, word) in [
        (r.counts.red, "off track"),
        (r.counts.amber, "at risk"),
        (r.counts.green, "on track"),
    ] {
        if n > 0 {
            levels.push(format!("{n} {word}"));
        }
    }
    let projects = if levels.is_empty() {
        "none scored".to_owned()
    } else {
        levels.join(", ")
    };

    let mut completed = if r.done.is_empty() {
        "none".to_owned()
    } else {
        plural(r.done.len(), "task", "tasks")
    };
    for (node_type, word) in [
        (NodeType::Project, "project"),
        (NodeType::Objective, "objective"),
    ] {
        let n = r
            .finished
            .iter()
            .filter(|f| f.node.node.node_type == node_type)
            .count();
        if n > 0 {
            completed.push_str(&format!(
                ", {n} {word}{} finished",
                if n == 1 { "" } else { "s" }
            ));
        }
    }

    let slipped = if r.slipped.is_empty() {
        "none".to_owned()
    } else {
        plural(r.slipped.len(), "item", "items")
    };
    let new_blocks = r.blocked.iter().filter(|b| b.newly_blocked).count();
    let blocked = match (r.blocked.len(), new_blocks) {
        (0, _) => "none".to_owned(),
        (n, 0) => plural(n, "task", "tasks"),
        (n, new) => format!("{}, {new} new this week", plural(n, "task", "tasks")),
    };
    let decisions = if r.decisions.is_empty() {
        "none".to_owned()
    } else {
        format!("{} made", r.decisions.len())
    };
    let capacity = if r.overloaded.is_empty() {
        "nobody".to_owned()
    } else {
        plural(r.overloaded.len(), "person", "people")
    };
    let waiting = match (r.waiting.len(), r.waiting_resolved.len()) {
        (0, 0) => "none".to_owned(),
        (s, 0) => format!("{s} stale"),
        (0, d) => format!("{d} resolved"),
        (s, d) => format!("{s} stale, {d} resolved"),
    };
    [
        ("Projects", projects),
        ("Completed", completed),
        ("Slipped", slipped),
        ("Blocked", blocked),
        ("Decisions", decisions),
        ("Over capacity", capacity),
        ("Waiting on others", waiting),
    ]
    .into_iter()
    .map(|(k, v)| format!("- **{k}:** {v}"))
    .collect::<Vec<_>>()
    .join("\n")
}

fn objectives(r: &WeeklyReview) -> String {
    if r.objectives.is_empty() {
        return "_No objectives yet._".to_owned();
    }
    let mut lines = vec![
        "| Objective | Health | Target | Why |".to_owned(),
        "|---|---|---|---|".to_owned(),
    ];
    for o in &r.objectives {
        let target = o.target_date.map(full).unwrap_or_else(|| "—".to_owned());
        let reason = why(&o.health);
        lines.push(format!(
            "| {} | {} | {} | {} |",
            escape(&o.objective.label),
            level_word(o.health.level),
            target,
            if reason.is_empty() {
                "—".to_owned()
            } else {
                reason
            },
        ));
    }
    lines.join("\n")
}

fn risks(r: &WeeklyReview) -> String {
    if r.risks.is_empty() {
        return "_No risks to report._".to_owned();
    }
    let mut lines: Vec<String> = r
        .risks
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let kind = if item.kind == RiskKind::Task {
                " (task)"
            } else {
                ""
            };
            let reason = why(&item.health);
            format!(
                "{}. {}{} — {}{}",
                i + 1,
                bold(&item.node.label),
                kind,
                level_word(item.health.level),
                if reason.is_empty() {
                    String::new()
                } else {
                    format!(": {reason}")
                }
            )
        })
        .collect();
    if r.more_risks > 0 {
        lines.push(format!("\n_…and {} more._", r.more_risks));
    }
    lines.join("\n")
}

fn done(r: &WeeklyReview) -> String {
    let mut lines: Vec<String> = r
        .finished
        .iter()
        .map(|f| {
            let what = if f.node.node.node_type == NodeType::Objective {
                "objective"
            } else {
                "project"
            };
            format!("- {} ({what}) finished {}", bold(&f.node.label), day(f.on))
        })
        .collect();
    lines.extend(r.done.iter().take(DONE_LIST_CAP).map(|d| {
        format!(
            "- {}{} — {}",
            escape(&d.node.label),
            context(d.project.as_deref(), d.assignee.as_deref()),
            day(d.completed_on)
        )
    }));
    if r.done.len() > DONE_LIST_CAP {
        lines.push(format!("- …and {} more", r.done.len() - DONE_LIST_CAP));
    }
    bullets(lines, "_No tasks were completed this week._")
}

fn slip_line(s: &ReviewSlip) -> String {
    let what = match s.node.node.node_type {
        NodeType::Task => context(
            s.project.as_deref(),
            s.assignee.as_ref().map(|a| a.label.as_str()),
        ),
        NodeType::Objective => " (objective)".to_owned(),
        _ => " (project)".to_owned(),
    };
    let detail = match (s.kind, s.from, s.to) {
        (SlipKind::DueMoved, Some(from), Some(to)) => format!(
            "due date moved from {} to {} ({} later)",
            day(from),
            day(to),
            plural(s.days as usize, "working day", "working days")
        ),
        (SlipKind::TargetMoved, Some(from), Some(to)) => format!(
            "target moved from {} to {} ({} later)",
            day(from),
            day(to),
            plural(s.days as usize, "working day", "working days")
        ),
        (_, _, Some(due)) => format!(
            "was due {}, still open ({} overdue)",
            day(due),
            plural(s.days as usize, "day", "days")
        ),
        _ => "slipped".to_owned(),
    };
    format!("- {}{} — {}", bold(&s.node.label), what, detail)
}

fn slipped(r: &WeeklyReview) -> String {
    bullets(
        r.slipped.iter().map(slip_line).collect(),
        "_Nothing slipped this week._",
    )
}

fn blocked(r: &WeeklyReview) -> String {
    bullets(
        r.blocked
            .iter()
            .map(|b| {
                let t = &b.task.row;
                let by = if b.task.blocked_by.is_empty() {
                    "no open blocker recorded".to_owned()
                } else {
                    let names: Vec<String> =
                        b.task.blocked_by.iter().map(|n| escape(&n.label)).collect();
                    format!("blocked by {}", names.join(", "))
                };
                format!(
                    "- {}{} — {}{}",
                    bold(&t.task.title),
                    context(
                        t.project.as_ref().map(|p| p.label.as_str()),
                        t.assignee.as_ref().map(|a| a.label.as_str())
                    ),
                    by,
                    if b.newly_blocked {
                        " · new this week"
                    } else {
                        ""
                    }
                )
            })
            .collect(),
        "_Nothing is blocked._",
    )
}

fn decisions(r: &WeeklyReview) -> String {
    bullets(
        r.decisions
            .iter()
            .map(|d| {
                let mut notes = Vec::new();
                if let Some(on) = d.decided_on {
                    notes.push(format!("decided {}", day(on)));
                }
                if !d.affects.is_empty() {
                    let names: Vec<String> = d.affects.iter().map(|a| escape(&a.label)).collect();
                    notes.push(format!("affects {}", names.join(", ")));
                }
                let excerpt = escape(&d.excerpt);
                format!(
                    "- {}{}{}",
                    bold(&d.title),
                    if excerpt.is_empty() {
                        String::new()
                    } else {
                        format!(" — {excerpt}")
                    },
                    if notes.is_empty() {
                        String::new()
                    } else {
                        format!(" _({})_", notes.join("; "))
                    }
                )
            })
            .collect(),
        "_No decisions were made this week._",
    )
}

fn capacity(r: &WeeklyReview) -> String {
    bullets(
        r.overloaded
            .iter()
            .map(|p| {
                let mut parts = Vec::new();
                if p.load_pct > 100.0 {
                    let week = p
                        .peak_week
                        .map(|w| format!(" in the week of {}", day(w)))
                        .unwrap_or_default();
                    parts.push(format!("{:.0}% of capacity{week}", p.load_pct));
                }
                if p.over_task_limit || parts.is_empty() {
                    parts.push(format!(
                        "{}{}",
                        plural(p.open_tasks as usize, "open task", "open tasks"),
                        if p.over_task_limit {
                            ", above the limit"
                        } else {
                            ""
                        }
                    ));
                }
                format!("- {} — {}", bold(&p.person.label), parts.join(" · "))
            })
            .collect(),
        "_Nobody is over capacity._",
    )
}

fn waiting_line(w: &WaitingOnRow) -> String {
    let mut parts = vec![format!(
        "waiting {}",
        plural(w.age_days.max(0) as usize, "day", "days")
    )];
    if let Some(by) = w.waiting.expected_by {
        parts.push(format!("expected {}", day(by)));
    }
    if w.overdue {
        parts.push("overdue".to_owned());
    }
    format!(
        "- {} — {}: {}",
        bold(&w.person.label),
        escape(&w.waiting.description),
        parts.join(", ")
    )
}

fn waiting(r: &WeeklyReview) -> String {
    let mut lines: Vec<String> = r.waiting.iter().map(waiting_line).collect();
    lines.extend(r.waiting_resolved.iter().map(|w| {
        format!(
            "- Resolved: {} — {}{}",
            bold(&w.person.label),
            escape(&w.waiting.description),
            w.waiting
                .resolved_on
                .map(|d| format!(" ({})", day(d)))
                .unwrap_or_default()
        )
    }));
    bullets(lines, "_Nothing stale, and nothing resolved this week._")
}

fn value(review: &WeeklyReview, name: &str) -> Option<String> {
    Some(match name {
        "title" => TITLE.to_owned(),
        "week_start" => full(review.week_start),
        "week_end" => full(review.week_end),
        "generated_on" => full(review.today),
        "summary" => summary(review),
        "objectives" => objectives(review),
        "risks" => risks(review),
        "done" => done(review),
        "slipped" => slipped(review),
        "blocked" => blocked(review),
        "decisions" => decisions(review),
        "capacity" => capacity(review),
        "waiting" => waiting(review),
        _ => return None,
    })
}

// ------------------------------------------------------------------ templates

/// Every `{{name}}` in the text: its byte range and trimmed name. An opening `{{` without a
/// closing `}}` is an error.
fn placeholders(template: &str) -> Result<Vec<(Range<usize>, String)>, String> {
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(open) = template[from..].find("{{") {
        let start = from + open;
        let Some(close) = template[start + 2..].find("}}") else {
            let line = template[..start].matches('\n').count() + 1;
            return Err(format!("The {{{{ on line {line} is never closed with }}}}"));
        };
        let end = start + 2 + close + 2;
        found.push((start..end, template[start + 2..end - 2].trim().to_owned()));
        from = end;
    }
    Ok(found)
}

/// Checks a template before it is saved: not blank, not huge, every placeholder known.
pub fn validate_template(template: &str) -> Result<(), String> {
    if template.trim().is_empty() {
        return Err("The report template can't be empty".to_owned());
    }
    if template.chars().count() > MAX_TEMPLATE_CHARS {
        return Err(format!(
            "The report template is longer than {MAX_TEMPLATE_CHARS} characters"
        ));
    }
    for (_, name) in placeholders(template)? {
        if !REPORT_PLACEHOLDERS.iter().any(|(known, _)| *known == name) {
            let known: Vec<String> = REPORT_PLACEHOLDERS
                .iter()
                .map(|(k, _)| format!("{{{{{k}}}}}"))
                .collect();
            return Err(format!(
                "Unknown placeholder {{{{{name}}}}}. Available: {}",
                known.join(", ")
            ));
        }
    }
    Ok(())
}

/// The report: `template` with every placeholder filled in. A placeholder this code does not
/// know (templates are validated on save, so only a hand-edited database has one) stays as
/// written.
pub fn render(review: &WeeklyReview, template: &str) -> String {
    let spans = placeholders(template).unwrap_or_default();
    let mut out = String::with_capacity(template.len() * 2);
    let mut at = 0;
    for (range, name) in spans {
        out.push_str(&template[at..range.start]);
        match value(review, &name) {
            Some(text) => out.push_str(&text),
            None => out.push_str(&template[range.clone()]),
        }
        at = range.end;
    }
    out.push_str(&template[at..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::Uuid;
    use minimap_types::{
        DecisionRow, DecisionStatus, HealthReason, NodeRef, NodeSummary, ObjectiveHealthRow,
        ObjectiveStatus, OverloadedPerson, OverviewCounts, ReviewBlocked, ReviewDone,
        ReviewFinished, RiskItem, Task, TaskRow, TaskStatus, WaitingOn, WeekTask,
        DEFAULT_REPORT_TEMPLATE,
    };
    use proptest::prelude::*;
    use time::{macros::date, OffsetDateTime};

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn summary_of(node_type: NodeType, n: u128, label: &str) -> NodeSummary {
        NodeSummary {
            node: NodeRef::new(node_type, id(n)),
            label: label.into(),
            archived: false,
        }
    }

    fn health(level: HealthLevel, score: u8, reasons: &[(HealthLevel, &str)]) -> Health {
        Health {
            level,
            score,
            reasons: reasons
                .iter()
                .map(|(level, text)| HealthReason {
                    level: *level,
                    text: (*text).into(),
                })
                .collect(),
        }
    }

    fn empty() -> WeeklyReview {
        WeeklyReview {
            week_start: date!(2027 - 03 - 01),
            week_end: date!(2027 - 03 - 07),
            prev_week_start: date!(2027 - 02 - 22),
            next_week_start: date!(2027 - 03 - 08),
            today: date!(2027 - 03 - 05),
            is_current_week: true,
            slipped: vec![],
            blocked: vec![],
            overloaded: vec![],
            waiting: vec![],
            waiting_resolved: vec![],
            decisions: vec![],
            done: vec![],
            finished: vec![],
            counts: OverviewCounts {
                red: 0,
                amber: 0,
                green: 0,
                idle: 0,
            },
            objectives: vec![],
            risks: vec![],
            more_risks: 0,
            warnings: vec![],
        }
    }

    fn at(d: time::Date) -> OffsetDateTime {
        d.with_hms(9, 0, 0).unwrap().assume_utc()
    }

    fn blocked_task(title: &str, by: &[&str]) -> WeekTask {
        WeekTask {
            row: TaskRow {
                task: Task {
                    links: Vec::new(),
                    id: id(500),
                    title: title.into(),
                    description: String::new(),
                    project_id: None,
                    status: TaskStatus::Blocked,
                    estimate_days: None,
                    start_date: None,
                    due_date: None,
                    completed_at: None,
                    priority: 2,
                    recurrence: None,
                    created_at: at(date!(2027 - 01 - 01)),
                    updated_at: at(date!(2027 - 01 - 01)),
                    archived_at: None,
                },
                project: Some(summary_of(NodeType::Project, 10, "API launch")),
                assignee: Some(summary_of(NodeType::Person, 20, "Priya")),
            },
            overdue_days: None,
            blocked_by: by
                .iter()
                .enumerate()
                .map(|(i, b)| summary_of(NodeType::Task, 600 + i as u128, b))
                .collect(),
        }
    }

    fn waiting_row(
        person: &str,
        what: &str,
        age: i64,
        expected: Option<time::Date>,
        resolved: Option<time::Date>,
    ) -> WaitingOnRow {
        WaitingOnRow {
            waiting: WaitingOn {
                id: id(700),
                description: what.into(),
                person_id: id(20),
                asked_on: date!(2027 - 02 - 01),
                expected_by: expected,
                follow_up_on: None,
                resolved_on: resolved,
                created_at: at(date!(2027 - 02 - 01)),
                updated_at: at(date!(2027 - 02 - 01)),
                archived_at: None,
            },
            person: summary_of(NodeType::Person, 20, person),
            about: None,
            age_days: age,
            stale: resolved.is_none(),
            snoozed: false,
            overdue: expected.is_some_and(|e| e < date!(2027 - 03 - 05)) && resolved.is_none(),
        }
    }

    /// A review with something in every section.
    fn busy() -> WeeklyReview {
        let mut r = empty();
        r.counts = OverviewCounts {
            red: 1,
            amber: 1,
            green: 2,
            idle: 0,
        };
        r.objectives = vec![
            ObjectiveHealthRow {
                objective: summary_of(NodeType::Objective, 1, "Launch the EU region"),
                status: ObjectiveStatus::AtRisk,
                priority: 1,
                target_date: Some(date!(2027 - 03 - 31)),
                health: health(
                    HealthLevel::Red,
                    20,
                    &[
                        (HealthLevel::Red, "Projected 6 days late"),
                        (HealthLevel::Amber, "3 tasks overdue"),
                        (HealthLevel::Amber, "a third reason"),
                    ],
                ),
                projects: vec![],
            },
            ObjectiveHealthRow {
                objective: summary_of(NodeType::Objective, 2, "Cut cloud cost | 20%"),
                status: ObjectiveStatus::OnTrack,
                priority: 3,
                target_date: None,
                health: health(
                    HealthLevel::Green,
                    90,
                    &[(HealthLevel::Green, "On schedule")],
                ),
                projects: vec![],
            },
        ];
        r.risks = vec![
            RiskItem {
                kind: RiskKind::Project,
                node: summary_of(NodeType::Project, 10, "API launch"),
                health: health(
                    HealthLevel::Red,
                    20,
                    &[(HealthLevel::Red, "Projected 6 days late")],
                ),
                risk_score: 120.0,
                priority: 1,
                via_objective: None,
            },
            RiskItem {
                kind: RiskKind::Task,
                node: summary_of(NodeType::Task, 500, "Security review"),
                health: health(HealthLevel::Amber, 50, &[]),
                risk_score: 60.0,
                priority: 2,
                via_objective: None,
            },
        ];
        r.more_risks = 3;
        r.done = vec![
            ReviewDone {
                node: summary_of(NodeType::Task, 1, "Ship the *beta*"),
                project: Some("API launch".into()),
                assignee: Some("Priya".into()),
                priority: 1,
                completed_on: date!(2027 - 03 - 02),
            },
            ReviewDone {
                node: summary_of(NodeType::Task, 2, "Write runbook"),
                project: None,
                assignee: None,
                priority: 3,
                completed_on: date!(2027 - 03 - 04),
            },
        ];
        r.finished = vec![ReviewFinished {
            node: summary_of(NodeType::Project, 11, "Onboarding revamp"),
            on: date!(2027 - 03 - 03),
        }];
        r.slipped = vec![
            ReviewSlip {
                node: summary_of(NodeType::Project, 10, "API launch"),
                kind: SlipKind::TargetMoved,
                project: None,
                assignee: None,
                from: Some(date!(2027 - 03 - 19)),
                to: Some(date!(2027 - 03 - 31)),
                days: 8,
            },
            ReviewSlip {
                node: summary_of(NodeType::Task, 3, "Load test"),
                kind: SlipKind::DueMoved,
                project: Some("API launch".into()),
                assignee: Some(summary_of(NodeType::Person, 20, "Priya")),
                from: Some(date!(2027 - 03 - 03)),
                to: Some(date!(2027 - 03 - 08)),
                days: 3,
            },
            ReviewSlip {
                node: summary_of(NodeType::Task, 4, "Pen test"),
                kind: SlipKind::Overdue,
                project: None,
                assignee: None,
                from: None,
                to: Some(date!(2027 - 03 - 02)),
                days: 3,
            },
        ];
        r.blocked = vec![
            ReviewBlocked {
                task: blocked_task("Security review", &["Vendor contract", "Legal sign-off"]),
                newly_blocked: true,
            },
            ReviewBlocked {
                task: blocked_task("Data migration", &[]),
                newly_blocked: false,
            },
        ];
        r.decisions = vec![DecisionRow {
            id: id(900),
            title: "Postgres over Mongo".into(),
            status: DecisionStatus::Decided,
            decided_on: Some(date!(2027 - 03 - 02)),
            excerpt: "Use Postgres for\nall services.".into(),
            affects: vec![summary_of(NodeType::Project, 10, "API launch")],
            superseded_by: None,
        }];
        r.overloaded = vec![
            OverloadedPerson {
                person: summary_of(NodeType::Person, 20, "Priya"),
                load_pct: 135.4,
                peak_week: Some(date!(2027 - 03 - 08)),
                open_tasks: 7,
                over_task_limit: false,
            },
            OverloadedPerson {
                person: summary_of(NodeType::Person, 21, "Sam"),
                load_pct: 80.0,
                peak_week: None,
                open_tasks: 14,
                over_task_limit: true,
            },
        ];
        r.waiting = vec![waiting_row(
            "Raj",
            "Security review sign-off",
            20,
            Some(date!(2027 - 03 - 01)),
            None,
        )];
        r.waiting_resolved = vec![waiting_row(
            "Mia",
            "Budget approval",
            9,
            None,
            Some(date!(2027 - 03 - 03)),
        )];
        r
    }

    #[test]
    fn the_default_report_reads_without_edits() {
        insta::assert_snapshot!(render(&busy(), DEFAULT_REPORT_TEMPLATE));
    }

    #[test]
    fn an_empty_week_still_reads_cleanly() {
        insta::assert_snapshot!(render(&empty(), DEFAULT_REPORT_TEMPLATE));
    }

    #[test]
    fn the_default_template_is_valid_and_uses_every_placeholder() {
        assert_eq!(validate_template(DEFAULT_REPORT_TEMPLATE), Ok(()));
        for (name, _) in REPORT_PLACEHOLDERS {
            assert!(
                DEFAULT_REPORT_TEMPLATE.contains(&format!("{{{{{name}}}}}")),
                "{name} is not in the default template"
            );
            assert!(value(&busy(), name).is_some(), "{name} has no value");
        }
    }

    #[test]
    fn templates_are_checked_before_they_are_saved() {
        assert!(validate_template("# Hi {{ summary }} and {{title}}").is_ok());
        let e = validate_template("{{summary}} {{nope}}").unwrap_err();
        assert!(e.contains("{{nope}}") && e.contains("{{summary}}"), "{e}");
        let e = validate_template("line one\n{{summary").unwrap_err();
        assert!(e.contains("line 2"), "{e}");
        assert!(validate_template("   \n").is_err());
        assert!(validate_template(&"x".repeat(MAX_TEMPLATE_CHARS + 1)).is_err());
        assert!(validate_template("no placeholders at all").is_ok());
    }

    #[test]
    fn the_template_decides_headings_order_and_which_sections_appear() {
        let r = busy();
        let out = render(
            &r,
            "# Board update — {{ week_start }}\n\nBlocked first:\n{{blocked}}\n\nThen {{summary}}",
        );
        assert!(
            out.starts_with("# Board update — 1 Mar 2027\n\nBlocked first:\n- **Security review**")
        );
        assert!(
            !out.contains("Pen test"),
            "sections left out of the template are left out"
        );
        assert!(out.contains("- **Projects:** 1 off track, 1 at risk, 2 on track"));
        assert_eq!(render(&r, "plain"), "plain");
    }

    #[test]
    fn an_unknown_placeholder_is_left_as_written() {
        assert_eq!(
            render(&empty(), "a {{mystery}} b {{title}}"),
            format!("a {{{{mystery}}}} b {TITLE}")
        );
        assert_eq!(render(&empty(), "broken {{ title"), "broken {{ title");
    }

    #[test]
    fn long_done_lists_are_cut_with_a_count() {
        let mut r = empty();
        r.done = (0..20)
            .map(|i| ReviewDone {
                node: summary_of(NodeType::Task, i, &format!("Task {i}")),
                project: None,
                assignee: None,
                priority: 3,
                completed_on: date!(2027 - 03 - 02),
            })
            .collect();
        let out = done(&r);
        assert_eq!(out.lines().count(), DONE_LIST_CAP + 1);
        assert!(out.ends_with("- …and 5 more"));
        assert!(summary(&r).contains("**Completed:** 20 tasks"));
    }

    #[test]
    fn user_text_cannot_break_the_markdown() {
        assert_eq!(
            escape("a | b\n*c* _d_ [e](f) <g>"),
            "a \\| b \\*c\\* \\_d\\_ \\[e\\](f) \\<g\\>"
        );
        let mut r = empty();
        r.objectives = busy().objectives;
        let table = objectives(&r);
        for line in table.lines() {
            // Four cells = five unescaped pipes per row.
            assert_eq!(line.replace("\\|", "").matches('|').count(), 5, "{line}");
        }
    }

    proptest! {
        #[test]
        fn escaped_text_has_no_bare_markup_or_newlines(text in "\\PC{0,60}") {
            let out = escape(&text);
            prop_assert!(!out.contains('\n'));
            let mut chars = out.chars();
            let mut bare = false;
            while let Some(c) = chars.next() {
                if c == '\\' {
                    chars.next();
                } else if matches!(c, '`' | '*' | '_' | '[' | ']' | '<' | '>' | '|' | '~') {
                    bare = true;
                }
            }
            prop_assert!(!bare, "{out:?}");
        }

        #[test]
        fn rendering_never_panics_and_validated_templates_leave_no_braces(
            template in "[a-z {}#\\n]{0,80}"
        ) {
            let out = render(&busy(), &template);
            if validate_template(&template).is_ok() {
                prop_assert!(!out.contains("{{") || template.contains("{{{"), "{out:?}");
            }
        }
    }
}
