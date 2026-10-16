//! Layout of the Gantt screen (spec 57): which rows are drawn, where every bar and arrow goes in
//! pixels, and what a drag of the pointer means in days. Pure, so it is unit-tested natively;
//! `pages/gantt.rs` draws it. The rules (what a drop writes, what is refused) are in
//! `minimap_core::gantt`; this only turns pixels into working days and back.

use std::collections::HashSet;

use minimap_types::{
    short_day, Band, Date, DragEdge, GanttData, GanttGroup, GanttLink, GanttLinkKind, GanttProject,
    GanttTask, ProjectStatus, TaskStatus, Uuid, Zone, ZoneLevel,
};

use crate::timeline::Granularity;

pub const ROW_H: f64 = 26.0;
pub const HEAD_H: f64 = 36.0;
pub const LABEL_W: f64 = 300.0;
pub const BAR_H: f64 = 12.0;
/// The grab zone at each end of a task bar.
pub const HANDLE_W: f64 = 7.0;
pub const MIN_BAR_W: f64 = 4.0;
/// A press that moves less than this many pixels is a click.
pub const CLICK_SLOP: f64 = 4.0;
const EPS: f64 = 1e-9;

/// The key of the "no objective" group.
pub const NO_OBJECTIVE: Uuid = Uuid::nil();

// ------------------------------------------------------------------------------ the rows

pub enum RowKind<'a> {
    Group(&'a GanttGroup),
    Project(&'a GanttProject),
    Task(&'a GanttTask),
}

pub struct Row<'a> {
    pub kind: RowKind<'a>,
    /// What the row is: the objective (or [`NO_OBJECTIVE`]), the project or the task.
    pub key: Uuid,
    /// 0 objective, 1 project, 2 task, 3 sub-task.
    pub depth: u8,
}

pub fn group_key(g: &GanttGroup) -> Uuid {
    g.objective.as_ref().map_or(NO_OBJECTIVE, |o| o.id)
}

/// Objectives start open and projects start closed; the person's clicks are kept as the set of rows
/// that differ from that.
pub fn is_open(default_open: bool, key: Uuid, toggled: &HashSet<Uuid>) -> bool {
    default_open != toggled.contains(&key)
}

fn project_finished(p: &GanttProject) -> bool {
    matches!(p.status, ProjectStatus::Done | ProjectStatus::Cancelled)
}

/// The rows to draw, top to bottom.
pub fn rows<'a>(data: &'a GanttData, toggled: &HashSet<Uuid>, show_done: bool) -> Vec<Row<'a>> {
    let mut out = Vec::new();
    for g in &data.groups {
        let key = group_key(g);
        out.push(Row {
            kind: RowKind::Group(g),
            key,
            depth: 0,
        });
        if !is_open(true, key, toggled) {
            continue;
        }
        for p in g
            .projects
            .iter()
            .filter(|p| show_done || !project_finished(p))
        {
            out.push(Row {
                kind: RowKind::Project(p),
                key: p.id,
                depth: 1,
            });
            if !is_open(false, p.id, toggled) {
                continue;
            }
            for t in p.tasks.iter().filter(|t| show_done || !t.schedule.done) {
                out.push(Row {
                    kind: RowKind::Task(t),
                    key: t.schedule.id,
                    depth: if t.parent.is_some() { 3 } else { 2 },
                });
            }
        }
    }
    out
}

/// The rows that can be opened (objectives and projects with something under them).
pub fn can_open(row: &Row) -> bool {
    match row.kind {
        RowKind::Group(g) => !g.projects.is_empty(),
        RowKind::Project(p) => !p.tasks.is_empty(),
        RowKind::Task(_) => false,
    }
}

/// The toggles that show every task (every project open, objectives as they are).
pub fn all_open(data: &GanttData) -> HashSet<Uuid> {
    data.groups
        .iter()
        .flat_map(|g| g.projects.iter().map(|p| p.id))
        .collect()
}

/// How many projects show their tasks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TasksShown {
    /// Every project is folded.
    None,
    /// Some are open and some folded (the person opened a few by hand).
    Some,
    /// Every project is open.
    All,
}

/// Whether the projects are all folded, all open, or a mix. Only projects with tasks count.
pub fn tasks_shown(data: &GanttData, toggled: &HashSet<Uuid>) -> TasksShown {
    let mut open = 0;
    let mut total = 0;
    for p in data
        .groups
        .iter()
        .flat_map(|g| g.projects.iter())
        .filter(|p| !p.tasks.is_empty())
    {
        total += 1;
        if is_open(false, p.id, toggled) {
            open += 1;
        }
    }
    match (open, total) {
        (0, _) => TasksShown::None,
        (o, t) if o == t => TasksShown::All,
        _ => TasksShown::Some,
    }
}

/// `toggled` with every project opened or folded; folded objectives stay as they are.
pub fn with_all_projects(data: &GanttData, toggled: &HashSet<Uuid>, open: bool) -> HashSet<Uuid> {
    let mut out = toggled.clone();
    for p in data.groups.iter().flat_map(|g| g.projects.iter()) {
        // Projects start folded: open means "differs from the default".
        if open {
            out.insert(p.id);
        } else {
            out.remove(&p.id);
        }
    }
    out
}

// --------------------------------------------------------------------------- the geometry

#[derive(Clone, Copy, Debug)]
pub struct Geo {
    pub first: f64,
    pub ppd: f64,
}

impl Geo {
    pub fn new(data: &GanttData, scale: Granularity) -> Self {
        Self {
            first: data.first_offset as f64,
            ppd: scale.px_per_day(),
        }
    }

    pub fn x(&self, offset: f64) -> f64 {
        (offset - self.first) * self.ppd
    }
}

pub fn chart_width(data: &GanttData, scale: Granularity) -> f64 {
    data.days.len() as f64 * scale.px_per_day()
}

/// Whether the zoom is wide enough to grab the edge of a bar.
pub fn can_resize(scale: Granularity) -> bool {
    scale.px_per_day() >= 20.0
}

/// The offset of a working day on the axis.
pub fn offset_of(data: &GanttData, d: Date) -> Option<i64> {
    data.days
        .binary_search(&d)
        .ok()
        .map(|i| data.first_offset + i as i64)
}

/// The date of an offset, kept on the axis.
pub fn day_at(data: &GanttData, offset: i64) -> Date {
    let last = data.days.len().saturating_sub(1) as i64;
    let i = (offset - data.first_offset).clamp(0, last) as usize;
    data.days.get(i).copied().unwrap_or(data.today)
}

/// How many working days the pointer moved: the axis has one column per working day.
pub fn snap_days(dx: f64, ppd: f64) -> i64 {
    (dx / ppd).round() as i64
}

/// The marks of a project or objective row.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Marks {
    /// The span of the work itself: x and width.
    pub work: Option<(f64, f64)>,
    /// Room between the end of the work and the target.
    pub room: Option<(f64, f64)>,
    /// The work that runs past the target.
    pub over: Option<(f64, f64)>,
    /// The pinned start (a project's start date) and the target flag.
    pub start_x: Option<f64>,
    pub target_x: Option<f64>,
}

pub fn marks(b: &Band, geo: Geo) -> Marks {
    let work = match (b.work_from, b.work_to) {
        (Some(from), Some(to)) => Some((geo.x(from), (geo.x(to) - geo.x(from)).max(MIN_BAR_W))),
        _ => None,
    };
    let target = b.target_offset;
    let room = target.and_then(|t| {
        let from = b.work_to.or(b.start_offset).unwrap_or(0.0);
        (t > from + EPS).then(|| (geo.x(from), geo.x(t) - geo.x(from)))
    });
    let over = match (b.work_to, target) {
        (Some(w), Some(t)) if w > t + EPS => Some((geo.x(t), geo.x(w) - geo.x(t))),
        _ => None,
    };
    Marks {
        work,
        room,
        over,
        start_x: b.start_offset.map(|o| geo.x(o)),
        target_x: target.map(|t| geo.x(t)),
    }
}

// ----------------------------------------------------------------------------- a task bar

/// The class of a task's bar: its status, and how it stands.
pub fn bar_class(t: &GanttTask) -> String {
    let s = &t.schedule;
    let status = match s.status {
        TaskStatus::Todo => "todo",
        TaskStatus::InProgress => "progress",
        TaskStatus::Blocked => "blocked",
        TaskStatus::Done => "done",
        TaskStatus::Cancelled => "cancelled",
    };
    format!(
        "gb {status}{}{}",
        if s.critical && !s.done {
            " critical"
        } else {
            ""
        },
        if s.unestimated { " unestimated" } else { "" },
    )
}

/// What a status is called on the legend and in the hover text.
pub fn status_word(s: TaskStatus) -> &'static str {
    match s {
        TaskStatus::Todo => "To do",
        TaskStatus::InProgress => "In progress",
        TaskStatus::Blocked => "Blocked",
        TaskStatus::Done => "Done",
        TaskStatus::Cancelled => "Cancelled",
    }
}

fn days_word(n: f64) -> String {
    let n = (n * 10.0).round() / 10.0;
    if (n - 1.0).abs() < EPS {
        "1 day".to_owned()
    } else {
        format!("{n} days")
    }
}

/// The hover text of a task bar.
pub fn task_tip(t: &GanttTask) -> String {
    let s = &t.schedule;
    let mut out = format!(
        "{}\n{} · {} to {} ({})",
        s.title,
        status_word(s.status),
        short_day(s.start),
        short_day(s.finish),
        days_word(s.duration_days)
    );
    if let Some(d) = t.due_date {
        out.push_str(&format!("\nDue {}", short_day(d)));
        if t.overdue {
            out.push_str(" (overdue)");
        }
    }
    if s.done {
        out.push_str("\nFinished: it stays on the days it was done.");
    } else if let Some(late) = s.late_by_days {
        out.push_str(&format!(
            "\nLate: {late} working day{} past what the target allows",
            if late == 1 { "" } else { "s" }
        ));
    } else if s.critical {
        out.push_str("\nCritical: any slip moves the finish");
    } else {
        out.push_str(&format!("\nSlack: {}", days_word(s.slack_days)));
    }
    if s.unestimated {
        out.push_str("\nNo estimate: counted as 1 day");
    }
    if s.earliest_start > s.start {
        // Never happens for a plan the schedule built, but the line is true if it ever does.
        out.push_str(&format!("\nEarliest start {}", short_day(s.earliest_start)));
    }
    out
}

/// The bar of a task if the pointer is `delta` working days along on `edge`, as (es, ef) offsets,
/// and whether the earliest start stopped it. The bar never starts before it can, never ends
/// before it starts and never leaves the axis.
pub fn task_candidate(data: &GanttData, t: &GanttTask, edge: DragEdge, delta: i64) -> Candidate {
    let s = &t.schedule;
    let base_start = offset_of(data, s.start).unwrap_or(s.es.floor() as i64);
    let base_end = offset_of(data, s.finish)
        .unwrap_or(s.ef.ceil() as i64 - 1)
        .max(base_start);
    let earliest = offset_of(data, s.earliest_start).unwrap_or(base_start.min(0));
    let last = data.first_offset + data.days.len() as i64 - 1;
    let (start, end, stopped) = match edge {
        DragEdge::Move => {
            let want = base_start + delta;
            let start = want.max(earliest).min(last - (base_end - base_start));
            (start, start + (base_end - base_start), want < earliest)
        }
        DragEdge::Start => {
            let want = base_start + delta;
            (want.max(earliest).min(base_end), base_end, want < earliest)
        }
        DragEdge::End => (
            base_start,
            (base_end + delta).clamp(base_start, last),
            false,
        ),
    };
    let moved_start = start != base_start;
    let moved_end = end != base_end;
    Candidate {
        es: if moved_start { start as f64 } else { s.es },
        ef: if moved_end { end as f64 + 1.0 } else { s.ef },
        start: day_at(data, start),
        end: day_at(data, end),
        days: (end - start + 1) as u32,
        stopped,
        changed: moved_start || moved_end,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candidate {
    pub es: f64,
    pub ef: f64,
    /// The first and last working day the bar would cover.
    pub start: Date,
    pub end: Date,
    pub days: u32,
    /// The pointer went past the earliest the task can start.
    pub stopped: bool,
    pub changed: bool,
}

/// Why a bar stopped where it did: what holds the task back from starting earlier.
pub fn stop_reason(data: &GanttData, t: &GanttTask) -> String {
    let s = &t.schedule;
    match s.held_by.and_then(|id| find_task(data, id)) {
        Some(h) => format!(
            "can't start before {} is done ({})",
            h.schedule.title,
            short_day(s.earliest_start)
        ),
        None => format!("can't start before {}", short_day(s.earliest_start)),
    }
}

/// What the floating label says while dragging.
pub fn drag_label(c: &Candidate) -> String {
    format!(
        "{} → {} · {}",
        short_day(c.start),
        short_day(c.end),
        days_word(f64::from(c.days))
    )
}

// ------------------------------------------------------------------ project and objective bars

/// Which part of a project's or objective's bar is dragged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BandPart {
    /// The left end: the date nothing in it starts before.
    Start,
    /// The right end, the flag: the target. More room for the work to grow into.
    Target,
    /// The whole bar: start and target together.
    Both,
}

/// The band as it would be if `part` were dragged `delta` working days.
pub fn shift_band(b: &Band, part: BandPart, delta: i64) -> Band {
    let d = delta as f64;
    let mut out = *b;
    if matches!(part, BandPart::Start | BandPart::Both) {
        out.start_offset = b.start_offset.map(|o| o + d);
    }
    if matches!(part, BandPart::Target | BandPart::Both) {
        out.target_offset = b.target_offset.map(|o| o + d);
    }
    // The start never passes the target, nor the target the start.
    if let (Some(s), Some(t)) = (out.start_offset, out.target_offset) {
        if s > t - 1.0 {
            match part {
                BandPart::Start => out.start_offset = Some(t - 1.0),
                BandPart::Target => out.target_offset = Some(s + 1.0),
                BandPart::Both => {}
            }
        }
    }
    out
}

/// The dates to write for a drag of a band: (start, target), each only if it changes.
pub fn band_dates(
    data: &GanttData,
    b: &Band,
    part: BandPart,
    delta: i64,
) -> (Option<Date>, Option<Date>) {
    let moved = shift_band(b, part, delta);
    let start = match (part, moved.start_offset) {
        (BandPart::Start | BandPart::Both, Some(o)) => Some(day_at(data, o as i64)),
        _ => None,
    };
    // The target is the end of its day: the day is one before.
    let target = match (part, moved.target_offset) {
        (BandPart::Target | BandPart::Both, Some(o)) => Some(day_at(data, o as i64 - 1)),
        _ => None,
    };
    (
        start.filter(|d| b.start_date != Some(*d)),
        target.filter(|d| b.target_date != Some(*d)),
    )
}

pub fn find_task(data: &GanttData, id: Uuid) -> Option<&GanttTask> {
    data.groups
        .iter()
        .flat_map(|g| g.projects.iter())
        .flat_map(|p| p.tasks.iter())
        .find(|t| t.schedule.id == id)
}

pub fn find_project(data: &GanttData, id: Uuid) -> Option<&GanttProject> {
    data.groups
        .iter()
        .flat_map(|g| g.projects.iter())
        .find(|p| p.id == id)
}

pub fn find_group(data: &GanttData, objective: Uuid) -> Option<&GanttGroup> {
    data.groups.iter().find(|g| group_key(g) == objective)
}

/// The toast after a band is dropped.
pub fn band_dropped_text(title: &str, start: Option<Date>, target: Option<Date>) -> String {
    match (start, target) {
        (Some(s), Some(t)) => format!("{title}: starts {}, target {}", short_day(s), short_day(t)),
        (Some(s), None) => format!(
            "{title}: starts {}. Nothing in it starts earlier.",
            short_day(s)
        ),
        (None, Some(t)) => format!("{title}: target {}", short_day(t)),
        (None, None) => format!("{title}: nothing changed"),
    }
}

// ---------------------------------------------------------------------------- the arrows

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkMode {
    /// Every arrow quietly, and the chain of the task in focus lit.
    Focus,
    All,
    /// Only the chains that decide each project's finish.
    Critical,
    Off,
}

impl LinkMode {
    pub const ALL: [LinkMode; 4] = [
        LinkMode::Focus,
        LinkMode::All,
        LinkMode::Critical,
        LinkMode::Off,
    ];

    pub fn label(self) -> &'static str {
        match self {
            LinkMode::Focus => "Focus",
            LinkMode::All => "All",
            LinkMode::Critical => "Critical",
            LinkMode::Off => "Off",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            LinkMode::Focus => {
                "All the arrows, quietly. Point at or open a task to light up everything it waits for and everything waiting for it."
            }
            LinkMode::All => "Every arrow, at full strength",
            LinkMode::Critical => "Only the chain of tasks that decides each project's finish",
            LinkMode::Off => "No arrows",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkTone {
    Hidden,
    /// Out of the chain in focus.
    Dim,
    /// Part of the whole picture.
    Quiet,
    Normal,
    /// On the chain in focus or the critical chain.
    Lit,
}

/// Every task the chain through `start` touches: what it waits for, and what waits for it.
pub fn chain_set(links: &[GanttLink], start: Uuid) -> HashSet<Uuid> {
    let mut seen = HashSet::from([start]);
    let mut backward = vec![start];
    while let Some(at) = backward.pop() {
        for l in links
            .iter()
            .filter(|l| l.kind == GanttLinkKind::Blocks && l.to == at)
        {
            if seen.insert(l.from) {
                backward.push(l.from);
            }
        }
    }
    let mut forward = vec![start];
    while let Some(at) = forward.pop() {
        for l in links
            .iter()
            .filter(|l| l.kind == GanttLinkKind::Blocks && l.from == at)
        {
            if seen.insert(l.to) {
                forward.push(l.to);
            }
        }
    }
    seen
}

/// The links between consecutive tasks of each project's critical chain.
pub fn critical_pairs(data: &GanttData) -> HashSet<(Uuid, Uuid)> {
    data.groups
        .iter()
        .flat_map(|g| g.projects.iter())
        .flat_map(|p| p.chain.windows(2).map(|w| (w[0], w[1])))
        .collect()
}

/// How an arrow is drawn. `focus` is the chain of the task pointed at or open, if any.
pub fn link_tone(
    mode: LinkMode,
    link: &GanttLink,
    focus: Option<&HashSet<Uuid>>,
    critical: &HashSet<(Uuid, Uuid)>,
) -> LinkTone {
    match mode {
        LinkMode::Off => LinkTone::Hidden,
        LinkMode::All => LinkTone::Normal,
        LinkMode::Critical => {
            if link.kind == GanttLinkKind::Blocks && critical.contains(&(link.from, link.to)) {
                LinkTone::Lit
            } else {
                LinkTone::Hidden
            }
        }
        LinkMode::Focus => match focus {
            None => LinkTone::Quiet,
            Some(set) if set.contains(&link.from) && set.contains(&link.to) => LinkTone::Lit,
            Some(_) => LinkTone::Dim,
        },
    }
}

/// An elbow from the end of one bar to the start of the next, as SVG path data.
pub fn elbow(x1: f64, y1: f64, x2: f64, y2: f64) -> String {
    if (y2 - y1).abs() < EPS {
        format!("M{x1} {y1}H{x2}")
    } else if x2 >= x1 + 12.0 {
        format!("M{x1} {y1}H{}V{y2}H{x2}", x1 + 6.0)
    } else {
        // The next bar starts before this one ends (a lag of nothing, or a finished task):
        // down between the rows and in from the left.
        let ym = y1 + (y2 - y1).signum() * ROW_H / 2.0;
        format!("M{x1} {y1}H{}V{ym}H{}V{y2}H{x2}", x1 + 6.0, x2 - 6.0)
    }
}

/// A small arrowhead pointing right at (x, y).
pub fn arrow_head(x: f64, y: f64) -> String {
    format!("M{} {}L{x} {y}L{} {}Z", x - 5.0, y - 3.0, x - 5.0, y + 3.0)
}

// ------------------------------------------------------------------------------- the words

pub fn group_title(g: &GanttGroup) -> String {
    g.objective
        .as_ref()
        .map_or_else(|| "No objective".to_owned(), |o| o.title.clone())
}

/// A project's counts: "4 open · 1 blocked · 2 done".
pub fn project_counts(p: &GanttProject) -> String {
    let mut parts = vec![format!("{} open", p.open_tasks)];
    if p.blocked_tasks > 0 {
        parts.push(format!("{} blocked", p.blocked_tasks));
    }
    if p.done_tasks > 0 {
        parts.push(format!("{} done", p.done_tasks));
    }
    parts.join(" · ")
}

/// What the room before a target comes to, in words, and whether it is bad news.
pub fn room_text(zone: &Zone, late_by: Option<u32>) -> Option<(String, bool)> {
    let plural = |n: u32| if n == 1 { "day" } else { "days" };
    match (zone, late_by) {
        (_, Some(over)) => Some((format!("{over} working {} over", plural(over)), true)),
        (Zone::Inside { room: 0, .. }, _) => Some(("no room left".to_owned(), false)),
        (Zone::Inside { room, .. }, _) => {
            Some((format!("{room} working {} of room", plural(*room)), false))
        }
        _ => None,
    }
}

/// [`room_text`] short enough for a chip in a row: "2d over", "14d of room", "no room".
pub fn room_chip(zone: &Zone, late_by: Option<u32>) -> Option<(String, bool)> {
    match (zone, late_by) {
        (_, Some(over)) => Some((format!("{over}d over"), true)),
        (Zone::Inside { room: 0, .. }, _) => Some(("no room".to_owned(), false)),
        (Zone::Inside { room, .. }, _) => Some((format!("{room}d room"), false)),
        _ => None,
    }
}

/// How wide the room is: the class that shades it.
pub fn room_class(zone: &Zone) -> &'static str {
    match zone {
        Zone::Inside {
            level: ZoneLevel::Wide,
            ..
        } => "room wide",
        Zone::Inside {
            level: ZoneLevel::Narrow,
            ..
        } => "room narrow",
        Zone::Inside {
            level: ZoneLevel::Closing,
            ..
        } => "room closing",
        _ => "room",
    }
}

/// The hover text of a project or objective bar.
pub fn band_tip(title: &str, b: &Band, zone: Option<&Zone>) -> String {
    let mut out = title.to_owned();
    if let Some(d) = b.start_date {
        out.push_str(&format!(
            "\nStarts {}: nothing in it starts earlier",
            short_day(d)
        ));
    }
    if let Some(d) = b.target_date {
        out.push_str(&format!("\nTarget {}", short_day(d)));
    }
    if let Some((text, _)) = zone.and_then(|z| room_text(z, b.late_by_days)) {
        out.push_str(&format!("\n{text}"));
    } else if let Some((text, _)) = room_text(&Zone::None, b.late_by_days) {
        out.push_str(&format!("\n{text}"));
    }
    out
}

/// The toast after a drop: what moved, and what it pushed.
pub fn dropped_text(title: &str, c: &Candidate, pushed: u32, days: Option<u32>) -> String {
    let mut out = format!("{title}: {}", drag_label(c));
    if pushed > 0 {
        out.push_str(&format!(
            ". {pushed} due date{} pushed later{}",
            if pushed == 1 { "" } else { "s" },
            days.map_or(String::new(), |d| format!(
                " (up to {})",
                days_word(f64::from(d))
            ))
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{GanttObjective, ObjectiveStatus, ScheduledTask};
    use time::macros::date;

    // Monday 2027-03-01, offset 0; the axis starts a day earlier.
    fn days(n: usize) -> (i64, Vec<Date>) {
        let mut out = Vec::new();
        let mut d = date!(2027 - 02 - 26);
        while out.len() < n {
            if d.weekday().number_days_from_monday() < 5 {
                out.push(d);
            }
            d = d.next_day().unwrap();
        }
        (-1, out)
    }

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn sched(n: u128, es: f64, ef: f64) -> ScheduledTask {
        let (_, axis) = days(40);
        let at = |o: f64| axis[(o.floor() as i64 + 1) as usize];
        ScheduledTask {
            id: id(n),
            title: format!("T{n}"),
            project_id: Some(id(10)),
            project_title: Some("P".into()),
            status: TaskStatus::Todo,
            done: false,
            unestimated: false,
            duration_days: ef - es,
            es,
            ef,
            start: at(es),
            finish: at((ef - 1.0).max(es)),
            ls: es,
            lf: ef,
            latest_start: at(es),
            latest_finish: at(ef - 1.0),
            slack_days: 0.0,
            critical: false,
            late_by_days: None,
            driver: None,
            earliest_start: at(0.0),
            held_by: None,
        }
    }

    fn task(n: u128, es: f64, ef: f64) -> GanttTask {
        GanttTask {
            schedule: sched(n, es, ef),
            start_date: None,
            due_date: None,
            due_offset: None,
            estimate_days: Some(ef - es),
            priority: 3,
            parent: None,
            overdue: false,
        }
    }

    fn project(n: u128, tasks: Vec<GanttTask>) -> GanttProject {
        GanttProject {
            id: id(n),
            title: format!("P{n}"),
            slug: format!("p{n}"),
            status: ProjectStatus::Active,
            band: Band::default(),
            zone: Zone::None,
            open_tasks: tasks.iter().filter(|t| !t.schedule.done).count() as u32,
            blocked_tasks: 0,
            done_tasks: 0,
            also_under: Vec::new(),
            chain: Vec::new(),
            tasks,
        }
    }

    fn data(groups: Vec<GanttGroup>, links: Vec<GanttLink>) -> GanttData {
        let (first, axis) = days(40);
        GanttData {
            today: date!(2027 - 03 - 01),
            first_offset: first,
            days: axis,
            groups,
            links,
        }
    }

    fn group(n: u128, projects: Vec<GanttProject>) -> GanttGroup {
        GanttGroup {
            objective: Some(GanttObjective {
                id: id(n),
                title: format!("O{n}"),
                status: ObjectiveStatus::OnTrack,
                ongoing: false,
            }),
            band: Band::default(),
            projects,
        }
    }

    fn blocks(a: u128, b: u128) -> GanttLink {
        GanttLink {
            kind: GanttLinkKind::Blocks,
            from: id(a),
            to: id(b),
            lag_days: 0,
        }
    }

    #[test]
    fn objectives_start_open_projects_start_closed_and_clicks_flip_them() {
        let d = data(
            vec![group(1, vec![project(10, vec![task(100, 0.0, 2.0)])])],
            vec![],
        );
        let none = HashSet::new();
        let kinds = |toggled: &HashSet<Uuid>| {
            rows(&d, toggled, true)
                .iter()
                .map(|r| r.depth)
                .collect::<Vec<_>>()
        };
        assert_eq!(kinds(&none), [0, 1]);
        assert_eq!(kinds(&HashSet::from([id(10)])), [0, 1, 2]);
        assert_eq!(kinds(&HashSet::from([id(1)])), [0]);
        assert_eq!(kinds(&all_open(&d)), [0, 1, 2]);
    }

    #[test]
    fn the_projects_are_all_folded_all_open_or_a_mix_and_objectives_keep_their_folds() {
        let d = data(
            vec![group(
                1,
                vec![
                    project(10, vec![task(100, 0.0, 2.0)]),
                    project(11, vec![task(101, 0.0, 1.0)]),
                    project(12, vec![]),
                ],
            )],
            vec![],
        );
        let none = HashSet::new();
        assert_eq!(tasks_shown(&d, &none), TasksShown::None);
        assert_eq!(tasks_shown(&d, &HashSet::from([id(10)])), TasksShown::Some);
        // A project with no tasks has nothing to open, so it doesn't stop "all".
        let all = with_all_projects(&d, &none, true);
        assert_eq!(tasks_shown(&d, &all), TasksShown::All);
        // A folded objective stays folded through both.
        let folded = HashSet::from([id(1)]);
        assert!(with_all_projects(&d, &folded, true).contains(&id(1)));
        let back = with_all_projects(&d, &all, false);
        assert_eq!(tasks_shown(&d, &back), TasksShown::None);
        assert!(with_all_projects(&d, &HashSet::from([id(1), id(10)]), false).contains(&id(1)));
    }

    #[test]
    fn finished_work_is_left_out_unless_asked_for() {
        let mut done = task(101, 0.0, 1.0);
        done.schedule.done = true;
        done.schedule.status = TaskStatus::Done;
        let mut finished = project(11, vec![]);
        finished.status = ProjectStatus::Done;
        let d = data(
            vec![group(
                1,
                vec![project(10, vec![task(100, 0.0, 2.0), done]), finished],
            )],
            vec![],
        );
        let open = all_open(&d);
        assert_eq!(rows(&d, &open, false).len(), 3);
        assert_eq!(rows(&d, &open, true).len(), 5);
    }

    #[test]
    fn sub_tasks_are_one_level_in() {
        let mut child = task(101, 0.0, 1.0);
        child.parent = Some(id(100));
        let d = data(
            vec![group(
                1,
                vec![project(10, vec![task(100, 0.0, 2.0), child])],
            )],
            vec![],
        );
        let depths: Vec<_> = rows(&d, &all_open(&d), true)
            .iter()
            .map(|r| r.depth)
            .collect();
        assert_eq!(depths, [0, 1, 2, 3]);
    }

    #[test]
    fn a_bar_is_known_by_its_status_and_how_it_stands() {
        let mut t = task(1, 0.0, 2.0);
        assert_eq!(bar_class(&t), "gb todo");
        t.schedule.status = TaskStatus::Blocked;
        t.schedule.critical = true;
        t.schedule.unestimated = true;
        assert_eq!(bar_class(&t), "gb blocked critical unestimated");
        t.schedule.status = TaskStatus::Done;
        t.schedule.done = true;
        assert_eq!(
            bar_class(&t),
            "gb done unestimated",
            "finished work is never critical"
        );
    }

    #[test]
    fn working_days_snap_and_dates_stay_on_the_axis() {
        assert_eq!(snap_days(30.0, 22.0), 1);
        assert_eq!(snap_days(-34.0, 22.0), -2);
        assert_eq!(snap_days(10.0, 22.0), 0);
        let d = data(vec![], vec![]);
        assert_eq!(offset_of(&d, date!(2027 - 03 - 01)), Some(0));
        assert_eq!(
            offset_of(&d, date!(2027 - 03 - 06)),
            None,
            "a Saturday is not on the axis"
        );
        assert_eq!(day_at(&d, 0), date!(2027 - 03 - 01));
        assert_eq!(day_at(&d, -50), d.days[0]);
        assert_eq!(day_at(&d, 5000), *d.days.last().unwrap());
    }

    #[test]
    fn dragging_a_bar_moves_it_whole_and_stops_at_the_earliest_start() {
        let d = data(vec![], vec![]);
        let mut t = task(1, 2.0, 4.0); // Wed-Thu
        t.schedule.earliest_start = date!(2027 - 03 - 02); // Tuesday
        let c = task_candidate(&d, &t, DragEdge::Move, 2);
        assert_eq!((c.es, c.ef), (4.0, 6.0));
        assert_eq!(
            (c.start, c.end),
            (date!(2027 - 03 - 05), date!(2027 - 03 - 08))
        );
        assert_eq!(c.days, 2);
        assert!(c.changed && !c.stopped);
        // Further left than Tuesday is held at Tuesday.
        let c = task_candidate(&d, &t, DragEdge::Move, -5);
        assert_eq!((c.es, c.ef), (1.0, 3.0));
        assert!(c.stopped);
    }

    #[test]
    fn the_edges_resize_and_never_cross() {
        let d = data(vec![], vec![]);
        let t = task(1, 1.0, 4.0); // Tue-Thu
        let c = task_candidate(&d, &t, DragEdge::End, 2);
        assert_eq!((c.es, c.ef, c.days), (1.0, 6.0, 5));
        let c = task_candidate(&d, &t, DragEdge::End, -9);
        assert_eq!(
            (c.es, c.ef, c.days),
            (1.0, 2.0, 1),
            "a task keeps at least a day"
        );
        let c = task_candidate(&d, &t, DragEdge::Start, 1);
        assert_eq!((c.es, c.ef, c.days), (2.0, 4.0, 2));
        let c = task_candidate(&d, &t, DragEdge::Start, 9);
        assert_eq!((c.es, c.ef, c.days), (3.0, 4.0, 1));
        let c = task_candidate(&d, &t, DragEdge::Start, 0);
        assert!(!c.changed);
    }

    #[test]
    fn the_chain_is_everything_before_and_after() {
        // 1 -> 2 -> 3, 4 -> 3 (4 only meets the chain at 3), and 5 -> 6 apart from it.
        let links = [blocks(1, 2), blocks(2, 3), blocks(4, 3), blocks(5, 6)];
        let set = chain_set(&links, id(2));
        let want: HashSet<Uuid> = [1, 2, 3].into_iter().map(id).collect();
        assert_eq!(set, want);
        assert_eq!(chain_set(&links, id(9)), HashSet::from([id(9)]));
    }

    #[test]
    fn arrows_are_quiet_lit_or_hidden_by_the_mode() {
        let links = [blocks(1, 2), blocks(3, 4)];
        let chain = chain_set(&links, id(1));
        let crit = HashSet::from([(id(1), id(2))]);
        let tone = |m, l: &GanttLink, f: Option<&HashSet<Uuid>>| link_tone(m, l, f, &crit);
        assert_eq!(tone(LinkMode::Focus, &links[0], None), LinkTone::Quiet);
        assert_eq!(
            tone(LinkMode::Focus, &links[0], Some(&chain)),
            LinkTone::Lit
        );
        assert_eq!(
            tone(LinkMode::Focus, &links[1], Some(&chain)),
            LinkTone::Dim
        );
        assert_eq!(tone(LinkMode::All, &links[1], None), LinkTone::Normal);
        assert_eq!(tone(LinkMode::Critical, &links[0], None), LinkTone::Lit);
        assert_eq!(tone(LinkMode::Critical, &links[1], None), LinkTone::Hidden);
        assert_eq!(tone(LinkMode::Off, &links[0], None), LinkTone::Hidden);
    }

    #[test]
    fn an_elbow_goes_out_down_and_in_and_backs_up_when_it_must() {
        assert_eq!(elbow(10.0, 13.0, 40.0, 39.0), "M10 13H16V39H40");
        assert_eq!(elbow(10.0, 13.0, 30.0, 13.0), "M10 13H30");
        let back = elbow(50.0, 13.0, 20.0, 39.0);
        assert!(back.starts_with("M50 13H56V26H14V39H20"), "{back}");
        assert!(arrow_head(40.0, 39.0).starts_with("M35 36L40 39"));
    }

    #[test]
    fn a_project_row_shows_the_work_the_room_and_the_overrun() {
        let geo = Geo {
            first: -1.0,
            ppd: 22.0,
        };
        let b = Band {
            work_from: Some(0.0),
            work_to: Some(5.0),
            start_date: Some(date!(2027 - 03 - 01)),
            start_offset: Some(0.0),
            target_date: Some(date!(2027 - 03 - 10)),
            target_offset: Some(8.0),
            late_by_days: None,
        };
        let m = marks(&b, geo);
        assert_eq!(m.work, Some((22.0, 110.0)));
        assert_eq!(m.room, Some((132.0, 66.0)));
        assert_eq!(m.over, None);
        assert_eq!((m.start_x, m.target_x), (Some(22.0), Some(198.0)));
        let late = Band {
            target_offset: Some(3.0),
            late_by_days: Some(2),
            ..b
        };
        let m = marks(&late, geo);
        assert_eq!(m.room, None);
        assert_eq!(m.over, Some((88.0, 44.0)));
        // No tasks yet: the room is the span from the start to the target.
        let empty = Band {
            work_from: None,
            work_to: None,
            ..b
        };
        let m = marks(&empty, geo);
        assert_eq!((m.work, m.room), (None, Some((22.0, 176.0))));
    }

    #[test]
    fn the_room_is_said_in_plain_words() {
        let inside = |room, level| Zone::Inside {
            remaining: 5,
            room,
            level,
        };
        assert_eq!(
            room_text(&inside(6, ZoneLevel::Wide), None),
            Some(("6 working days of room".to_owned(), false))
        );
        assert_eq!(
            room_text(&inside(1, ZoneLevel::Narrow), None),
            Some(("1 working day of room".to_owned(), false))
        );
        assert_eq!(
            room_text(&inside(0, ZoneLevel::Closing), None),
            Some(("no room left".to_owned(), false))
        );
        assert_eq!(
            room_text(
                &Zone::Outside {
                    remaining: 9,
                    over: 3
                },
                Some(3)
            ),
            Some(("3 working days over".to_owned(), true))
        );
        assert_eq!(room_text(&Zone::None, None), None);
        assert_eq!(room_class(&inside(2, ZoneLevel::Narrow)), "room narrow");
    }

    #[test]
    fn a_stopped_bar_says_what_holds_it() {
        let first = task(1, 0.0, 2.0);
        let mut second = task(2, 2.0, 3.0);
        second.schedule.held_by = Some(id(1));
        second.schedule.earliest_start = date!(2027 - 03 - 03);
        let d = data(
            vec![group(1, vec![project(10, vec![first, second.clone()])])],
            vec![],
        );
        assert_eq!(
            stop_reason(&d, &second),
            "can't start before T1 is done (Wed 3 Mar)"
        );
        second.schedule.held_by = None;
        assert_eq!(stop_reason(&d, &second), "can't start before Wed 3 Mar");
    }

    #[test]
    fn the_chip_is_the_room_in_a_few_characters() {
        let inside = |room| Zone::Inside {
            remaining: 5,
            room,
            level: ZoneLevel::Wide,
        };
        assert_eq!(
            room_chip(&inside(14), None),
            Some(("14d room".to_owned(), false))
        );
        assert_eq!(
            room_chip(&inside(0), None),
            Some(("no room".to_owned(), false))
        );
        assert_eq!(
            room_chip(
                &Zone::Outside {
                    remaining: 9,
                    over: 2
                },
                Some(2)
            ),
            Some(("2d over".to_owned(), true))
        );
        assert_eq!(room_chip(&Zone::None, None), None);
    }

    #[test]
    fn the_toast_after_a_drop_says_what_moved_and_what_it_pushed() {
        let d = data(vec![], vec![]);
        let c = task_candidate(&d, &task(1, 0.0, 2.0), DragEdge::Move, 2);
        let text = dropped_text("Design", &c, 2, Some(2));
        assert_eq!(
            text,
            "Design: Wed 3 Mar → Thu 4 Mar · 2 days. 2 due dates pushed later (up to 2 days)"
        );
        assert_eq!(
            dropped_text("Design", &c, 0, None),
            "Design: Wed 3 Mar → Thu 4 Mar · 2 days"
        );
    }

    #[test]
    fn dragging_the_flag_gives_the_work_more_room_and_the_start_never_passes_it() {
        let d = data(vec![], vec![]);
        let b = Band {
            work_from: Some(0.0),
            work_to: Some(3.0),
            start_date: Some(date!(2027 - 03 - 01)),
            start_offset: Some(0.0),
            target_date: Some(date!(2027 - 03 - 05)),
            target_offset: Some(5.0),
            late_by_days: None,
        };
        // Three days further: the target moves from Friday 5th to Wednesday 10th.
        let (start, target) = band_dates(&d, &b, BandPart::Target, 3);
        assert_eq!((start, target), (None, Some(date!(2027 - 03 - 10))));
        // The whole bar moves both.
        let (start, target) = band_dates(&d, &b, BandPart::Both, 2);
        assert_eq!(
            (start, target),
            (Some(date!(2027 - 03 - 03)), Some(date!(2027 - 03 - 09)))
        );
        // The start stops a day short of the target; the target a day after the start.
        let m = shift_band(&b, BandPart::Start, 20);
        assert_eq!(m.start_offset, Some(4.0));
        let m = shift_band(&b, BandPart::Target, -20);
        assert_eq!(m.target_offset, Some(1.0));
        // Dropping it where it is changes nothing.
        assert_eq!(band_dates(&d, &b, BandPart::Both, 0), (None, None));
        assert_eq!(
            band_dropped_text("API", None, Some(date!(2027 - 03 - 10))),
            "API: target Wed 10 Mar"
        );
    }

    #[test]
    fn grabbing_edges_needs_a_wide_enough_scale() {
        assert!(can_resize(Granularity::Days));
        assert!(!can_resize(Granularity::Weeks));
    }
}
