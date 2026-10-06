//! Timeline layout for a schedule (spec 13): where each bar goes, in pixels. Pure, so it is
//! unit-tested natively; `components/schedule_panel.rs` draws it as SVG.
//!
//! The axis is working days (weekends are not drawn). Days gives every working day a column;
//! Weeks packs a week into five narrow ones for a long view.

use minimap_types::{Date, ProjectForecast, Schedule, ScheduledTask, Uuid};

use crate::calendar::MONTHS;

pub const ROW_H: f64 = 22.0;
pub const HEADER_H: f64 = 30.0;
pub const BAR_H: f64 = 10.0;
const MIN_BAR_W: f64 = 3.0;
const EPS: f64 = 1e-9;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Granularity {
    Days,
    Weeks,
}

impl Granularity {
    pub fn px_per_day(self) -> f64 {
        match self {
            Granularity::Days => 22.0,
            Granularity::Weeks => 8.0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Granularity::Days => "Days",
            Granularity::Weeks => "Weeks",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarKind {
    /// Decides the project's finish.
    Critical,
    Normal,
    Done,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Bar {
    pub id: Uuid,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub kind: BarKind,
    /// Past its latest finish (negative slack).
    pub late: bool,
    pub unestimated: bool,
    /// Where the slack ("float") line ends, for tasks that can slip without moving the finish.
    pub slack_end: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TickKind {
    Week,
    Day,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tick {
    pub x: f64,
    pub label: String,
    pub kind: TickKind,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub width: f64,
    pub height: f64,
    pub bars: Vec<Bar>,
    pub ticks: Vec<Tick>,
    pub today_x: f64,
    pub target_x: Option<f64>,
}

/// The tasks drawn, in row order.
pub fn visible_tasks(schedule: &Schedule, show_done: bool) -> Vec<&ScheduledTask> {
    schedule
        .tasks
        .iter()
        .filter(|t| show_done || !t.done)
        .collect()
}

fn short_month(d: Date) -> &'static str {
    let name = MONTHS[(d.month() as usize).saturating_sub(1).min(11)];
    &name[..3]
}

pub fn layout(
    schedule: &Schedule,
    granularity: Granularity,
    show_done: bool,
    target_offset: Option<f64>,
) -> Layout {
    let ppd = granularity.px_per_day();
    let first = schedule.first_offset as f64;
    let x_of = |offset: f64| (offset - first) * ppd;
    let rows = visible_tasks(schedule, show_done);

    let bars = rows
        .iter()
        .enumerate()
        .map(|(row, t)| {
            let x = x_of(t.es);
            let kind = if t.done {
                BarKind::Done
            } else if t.critical {
                BarKind::Critical
            } else {
                BarKind::Normal
            };
            Bar {
                id: t.id,
                x,
                y: HEADER_H + row as f64 * ROW_H + (ROW_H - BAR_H) / 2.0,
                w: (x_of(t.ef) - x).max(MIN_BAR_W),
                kind,
                late: t.late_by_days.is_some(),
                unestimated: t.unestimated,
                slack_end: (!t.done && !t.critical && t.slack_days > EPS)
                    .then(|| x_of(t.ef + t.slack_days)),
            }
        })
        .collect();

    let mut ticks = Vec::new();
    for (i, d) in schedule.days.iter().enumerate() {
        let x = i as f64 * ppd;
        // A new (Monday-first) week starts at the first working day after the weekday number
        // goes back down: Monday normally, Tuesday when Monday isn't worked.
        let new_week = i > 0
            && schedule.days.get(i - 1).is_some_and(|prev| {
                d.weekday().number_days_from_monday() < prev.weekday().number_days_from_monday()
            });
        if new_week || i == 0 {
            ticks.push(Tick {
                x,
                label: format!("{} {}", short_month(*d), d.day()),
                kind: TickKind::Week,
            });
        }
        if granularity == Granularity::Days && !(new_week || i == 0) {
            ticks.push(Tick {
                x,
                label: String::new(),
                kind: TickKind::Day,
            });
        }
        if granularity == Granularity::Days {
            ticks.push(Tick {
                x: x + ppd / 2.0,
                label: d.day().to_string(),
                kind: TickKind::Day,
            });
        }
    }

    Layout {
        width: schedule.days.len() as f64 * ppd,
        height: HEADER_H + rows.len() as f64 * ROW_H,
        bars,
        ticks,
        today_x: x_of(0.0),
        target_x: target_offset.map(x_of),
    }
}

const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/// `Fri 2027-03-05`.
pub fn day_text(d: Date) -> String {
    format!(
        "{} {d}",
        WEEKDAYS[d.weekday().number_days_from_monday() as usize]
    )
}

fn days_text(n: f64) -> String {
    let n = (n * 10.0).round() / 10.0;
    if (n - 1.0).abs() < EPS {
        "1 day".to_owned()
    } else {
        format!("{n} days")
    }
}

/// The hover text for a task: its dates, how long, and why it matters.
pub fn describe(t: &ScheduledTask) -> String {
    let mut s = format!(
        "{}\n{} to {} ({})",
        t.title,
        day_text(t.start),
        day_text(t.finish),
        days_text(t.duration_days)
    );
    if t.done {
        s.push_str("\nFinished");
    } else if let Some(late) = t.late_by_days {
        s.push_str(&format!(
            "\nLate: {late} working day{} past what the target allows",
            if late == 1 { "" } else { "s" }
        ));
    } else if t.critical {
        s.push_str("\nCritical: any slip moves the finish");
    } else {
        s.push_str(&format!("\nSlack: {}", days_text(t.slack_days)));
    }
    if t.unestimated {
        s.push_str("\nNo estimate: counted as 1 day");
    }
    s
}

/// One line about the project: when it finishes against its target.
pub fn forecast_text(f: &ProjectForecast) -> String {
    let mut parts: Vec<String> = Vec::new();
    match (f.open_tasks, f.projected_finish) {
        (0, Some(d)) => parts.push(format!("No open tasks; last finished {}", day_text(d))),
        (0, None) => parts.push("No tasks to schedule".to_owned()),
        (_, Some(d)) => parts.push(format!("Projected finish {}", day_text(d))),
        (_, None) => {}
    }
    if let (Some(target), true) = (f.target_date, f.open_tasks > 0) {
        match f.late_by_days {
            Some(n) => parts.push(format!(
                "target {target}: late by {n} working day{}",
                if n == 1 { "" } else { "s" }
            )),
            None => {
                let spare = match (f.target_offset, f.finish_offset) {
                    (Some(t), Some(fin)) => (t - fin + EPS).floor().max(0.0) as i64,
                    _ => 0,
                };
                parts.push(format!(
                    "target {target}: on track ({spare} working day{} to spare)",
                    if spare == 1 { "" } else { "s" }
                ));
            }
        }
    } else if f.open_tasks > 0 {
        parts.push("no target date".to_owned());
    }
    if f.unestimated_tasks > 0 {
        parts.push(format!(
            "{} task{} no estimate (counted as 1 day)",
            f.unestimated_tasks,
            if f.unestimated_tasks == 1 {
                " has"
            } else {
                "s have"
            }
        ));
    }
    parts.join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{ScheduleScope, TaskStatus};
    use time::macros::date;

    fn sched_task(
        n: u128,
        title: &str,
        es: f64,
        ef: f64,
        slack: f64,
        critical: bool,
        done: bool,
    ) -> ScheduledTask {
        ScheduledTask {
            id: Uuid::from_u128(n),
            title: title.into(),
            project_id: None,
            project_title: None,
            status: if done {
                TaskStatus::Done
            } else {
                TaskStatus::Todo
            },
            done,
            unestimated: false,
            duration_days: ef - es,
            es,
            ef,
            start: date!(2027 - 03 - 01),
            finish: date!(2027 - 03 - 03),
            ls: es + slack,
            lf: ef + slack,
            latest_start: date!(2027 - 03 - 01),
            latest_finish: date!(2027 - 03 - 03),
            slack_days: slack,
            critical,
            late_by_days: (slack < 0.0).then_some((-slack).ceil() as u32),
        }
    }

    /// Two weeks of working days from Monday 2027-03-01, with `first_offset` = -2.
    fn schedule(tasks: Vec<ScheduledTask>) -> Schedule {
        let mut days = Vec::new();
        let mut d = date!(2027 - 02 - 25); // Thursday: offset -2
        while days.len() < 12 {
            if d.weekday().number_days_from_monday() < 5 {
                days.push(d);
            }
            d = d.next_day().unwrap();
        }
        Schedule {
            scope: ScheduleScope::Portfolio,
            today: date!(2027 - 03 - 01),
            first_offset: -2,
            days,
            tasks,
            projects: vec![],
        }
    }

    #[test]
    fn bars_sit_on_the_axis_and_rows_follow_the_task_order() {
        let s = schedule(vec![
            sched_task(1, "A", 0.0, 3.0, 0.0, true, false),
            sched_task(2, "B", 3.0, 5.0, 2.0, false, false),
        ]);
        let l = layout(&s, Granularity::Days, false, None);
        assert_eq!(l.bars.len(), 2);
        // Offset 0 is the third column (first_offset -2).
        assert_eq!(l.today_x, 44.0);
        assert_eq!((l.bars[0].x, l.bars[0].w), (44.0, 66.0));
        assert_eq!(l.bars[0].kind, BarKind::Critical);
        assert_eq!(l.bars[0].slack_end, None);
        assert_eq!(l.bars[1].kind, BarKind::Normal);
        assert_eq!(l.bars[1].x, 110.0);
        assert_eq!(l.bars[1].slack_end, Some(l.bars[1].x + l.bars[1].w + 44.0));
        assert!(l.bars[1].y > l.bars[0].y);
        assert_eq!(l.height, HEADER_H + 2.0 * ROW_H);
        assert_eq!(l.width, 12.0 * 22.0);
    }

    #[test]
    fn weeks_are_narrower_but_keep_the_same_proportions() {
        let s = schedule(vec![sched_task(1, "A", 0.0, 5.0, 0.0, true, false)]);
        let days = layout(&s, Granularity::Days, false, None);
        let weeks = layout(&s, Granularity::Weeks, false, None);
        assert_eq!(weeks.bars[0].w, 5.0 * 8.0);
        assert!(weeks.width < days.width);
        assert_eq!(weeks.width, 12.0 * 8.0);
        // Weeks draw no per-day columns or labels.
        assert!(weeks.ticks.iter().all(|t| t.kind == TickKind::Week));
        assert!(days.ticks.iter().any(|t| t.kind == TickKind::Day));
    }

    #[test]
    fn week_ticks_are_the_mondays_and_the_first_column() {
        let s = schedule(vec![]);
        let l = layout(&s, Granularity::Weeks, false, None);
        let labels: Vec<&str> = l.ticks.iter().map(|t| t.label.as_str()).collect();
        // Feb 25 (first column), then Mondays Mar 1 and Mar 8.
        assert_eq!(labels, ["Feb 25", "Mar 1", "Mar 8"]);
        assert_eq!(l.ticks[1].x, 2.0 * 8.0);
    }

    #[test]
    fn a_week_that_skips_monday_still_gets_its_tick_on_its_first_working_day() {
        // Tuesday to Friday weeks (Monday not worked): ticks at the first column and at each
        // Tuesday.
        let mut s = schedule(vec![]);
        let mut days = Vec::new();
        let mut d = date!(2027 - 03 - 02); // Tuesday
        while days.len() < 8 {
            if (1..5).contains(&d.weekday().number_days_from_monday()) {
                days.push(d);
            }
            d = d.next_day().unwrap();
        }
        s.days = days;
        let l = layout(&s, Granularity::Weeks, false, None);
        let labels: Vec<&str> = l.ticks.iter().map(|t| t.label.as_str()).collect();
        assert_eq!(labels, ["Mar 2", "Mar 9"]);
        assert_eq!(l.ticks[1].x, 4.0 * 8.0);
    }

    #[test]
    fn finished_work_is_hidden_unless_asked_and_never_critical() {
        let s = schedule(vec![
            sched_task(1, "Done", -2.0, 0.0, 0.0, false, true),
            sched_task(2, "Open", 0.0, 2.0, 0.0, true, false),
        ]);
        assert_eq!(visible_tasks(&s, false).len(), 1);
        assert_eq!(visible_tasks(&s, true).len(), 2);
        let l = layout(&s, Granularity::Days, true, None);
        assert_eq!(l.bars[0].kind, BarKind::Done);
        assert_eq!(l.bars[0].x, 0.0);
        assert_eq!(l.bars.len(), 2);
    }

    #[test]
    fn late_work_is_marked_and_tiny_bars_stay_visible() {
        let s = schedule(vec![
            sched_task(1, "Late", 0.0, 2.0, -1.0, true, false),
            sched_task(2, "Milestone", 1.0, 1.0, 0.0, false, false),
        ]);
        let l = layout(&s, Granularity::Days, false, None);
        assert!(l.bars[0].late && !l.bars[1].late);
        assert_eq!(l.bars[1].w, MIN_BAR_W);
    }

    #[test]
    fn the_target_line_follows_the_offset() {
        let s = schedule(vec![]);
        let l = layout(&s, Granularity::Days, false, Some(5.0));
        assert_eq!(l.target_x, Some(7.0 * 22.0));
        assert_eq!(layout(&s, Granularity::Days, false, None).target_x, None);
    }

    fn forecast() -> ProjectForecast {
        ProjectForecast {
            project_id: Uuid::nil(),
            title: "P".into(),
            projected_finish: Some(date!(2027 - 03 - 05)),
            finish_offset: Some(5.0),
            target_date: Some(date!(2027 - 03 - 12)),
            target_offset: Some(10.0),
            late_by_days: None,
            open_tasks: 3,
            unestimated_tasks: 0,
            critical_tasks: 2,
        }
    }

    #[test]
    fn the_forecast_line_says_on_track_late_or_unknown() {
        assert_eq!(
            forecast_text(&forecast()),
            "Projected finish Fri 2027-03-05 · target 2027-03-12: on track (5 working days to spare)"
        );
        let mut late = forecast();
        late.late_by_days = Some(1);
        assert!(forecast_text(&late).contains("late by 1 working day"));
        assert!(!forecast_text(&late).contains("days"));
        let mut open = forecast();
        open.target_date = None;
        open.unestimated_tasks = 2;
        assert_eq!(
            forecast_text(&open),
            "Projected finish Fri 2027-03-05 · no target date · 2 tasks have no estimate (counted as 1 day)"
        );
        let mut empty = forecast();
        empty.open_tasks = 0;
        empty.projected_finish = None;
        assert_eq!(forecast_text(&empty), "No tasks to schedule");
        empty.projected_finish = Some(date!(2027 - 02 - 25));
        assert!(forecast_text(&empty).starts_with("No open tasks; last finished Thu"));
    }

    #[test]
    fn hover_text_explains_the_bar() {
        let mut t = sched_task(1, "Fix login", 0.0, 3.0, 0.0, true, false);
        assert!(describe(&t).contains("Fix login\nMon 2027-03-01 to Wed 2027-03-03 (3 days)"));
        assert!(describe(&t).contains("Critical"));
        t.critical = false;
        t.slack_days = 2.0;
        assert!(describe(&t).contains("Slack: 2 days"));
        t.slack_days = -2.0;
        t.late_by_days = Some(2);
        assert!(describe(&t).contains("Late: 2 working days"));
        t.unestimated = true;
        assert!(describe(&t).contains("No estimate"));
        t.done = true;
        assert!(describe(&t).contains("Finished"));
    }
}
