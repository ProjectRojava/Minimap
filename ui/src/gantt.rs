//! Layout of the Gantt screen (spec 57): which rows are drawn, where every bar and arrow goes in
//! pixels, and what a drag of the pointer means in days. Pure, so it is unit-tested natively;
//! `pages/gantt.rs` draws it. The rules (what a drop writes, what is refused) are in
//! `minimap_core::gantt`; this only turns pixels into working days and back.

use std::collections::HashSet;

use minimap_types::{
    short_day, Band, Capacity, Date, DragEdge, GanttData, GanttGroup, GanttLink, GanttLinkKind,
    GanttProject, GanttRef, GanttTask, ProjectStatus, TaskStatus, Uuid, Zone, ZoneLevel,
};

use crate::labels::{objective_status_label, project_status_label};

use crate::calendar::MONTHS;

pub const ROW_H: f64 = 26.0;
pub const HEAD_H: f64 = 38.0;
/// The label column's width until it is dragged; it stays between the two limits.
pub const LABEL_W: f64 = 340.0;
pub const LABEL_MIN: f64 = 200.0;
pub const LABEL_MAX: f64 = 600.0;
/// The counts and chips at the right of the label column (spec 58), when the column is wide
/// enough to spare it.
pub const COUNTS_W: f64 = 124.0;
pub const BAR_H: f64 = 12.0;
/// Pixels per working day of the *Days* and *Weeks* scales (as the project panel's timeline).
pub const DAYS_PPD: f64 = 22.0;
pub const WEEKS_PPD: f64 = 8.0;
/// *Fit* never squeezes a day below this.
const FIT_MIN_PPD: f64 = 1.5;
/// Names are drawn next to the bars from this many pixels per day.
const NAMES_MIN_PPD: f64 = 16.0;
/// Day numbers fit in the header from this many pixels per day.
const DAY_NUMBERS_MIN_PPD: f64 = 14.0;
/// A rough width of one character of the chart's 10-11 px text, to fit a name in the space left.
const CHAR_W: f64 = 6.0;
/// The same for the label column's 12-13 px text.
const LABEL_CHAR_W: f64 = 7.0;
/// How far an ongoing objective's line runs past its last work before it has faded out.
const FADE_DAYS: f64 = 15.0;
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
    /// The empty row under an open project's tasks, where dragging draws a new task (spec 58).
    Add(&'a GanttProject),
    /// A person, with the tasks they are the first assignee of under them (*By person*, spec 58).
    Person(&'a GanttRef),
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

/// Objectives and people start open; a project starts open when it needs a look (spec 58) and
/// folded otherwise. The person's clicks are kept as the set of rows that differ from that.
pub fn is_open(default_open: bool, key: Uuid, toggled: &HashSet<Uuid>) -> bool {
    default_open != toggled.contains(&key)
}

/// Whether a row starts open.
pub fn default_open(row: &Row) -> bool {
    match row.kind {
        RowKind::Group(_) | RowKind::Person(_) => true,
        RowKind::Project(p) => p.needs_a_look,
        RowKind::Task(_) | RowKind::Add(_) => false,
    }
}

/// Whether a row is open now.
pub fn row_open(row: &Row, toggled: &HashSet<Uuid>) -> bool {
    is_open(default_open(row), row.key, toggled)
}

/// The key of a project's add row (it can't share the project's own).
pub fn add_key(project: Uuid) -> Uuid {
    Uuid::from_u128(project.as_u128() ^ 0xadd0_adda_0000_0000_0000_0000_0000_00ad)
}

/// The row of the people with no task assigned (*By person*).
pub static UNASSIGNED: std::sync::LazyLock<GanttRef> = std::sync::LazyLock::new(|| GanttRef {
    id: Uuid::max(),
    title: "Not assigned".to_owned(),
});

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
            if !is_open(p.needs_a_look, p.id, toggled) || p.tasks.is_empty() {
                continue;
            }
            for t in p.tasks.iter().filter(|t| show_done || !t.schedule.done) {
                out.push(Row {
                    kind: RowKind::Task(t),
                    key: t.schedule.id,
                    depth: if t.parent.is_some() { 3 } else { 2 },
                });
            }
            if !project_finished(p) {
                out.push(Row {
                    kind: RowKind::Add(p),
                    key: add_key(p.id),
                    depth: 2,
                });
            }
        }
    }
    out
}

/// The tasks a person's row holds: those they are the first assignee of (by name), or for
/// [`UNASSIGNED`] those with nobody, in schedule order.
pub fn person_tasks(data: &GanttData, person: Uuid, show_done: bool) -> Vec<&GanttTask> {
    let rank: std::collections::HashMap<Uuid, usize> = data
        .people
        .iter()
        .enumerate()
        .map(|(i, p)| (p.id, i))
        .collect();
    let first = |t: &GanttTask| {
        t.assignees
            .iter()
            .filter_map(|a| rank.get(a).map(|r| (*r, *a)))
            .min()
            .map(|(_, a)| a)
    };
    let mut out: Vec<&GanttTask> = data
        .groups
        .iter()
        .flat_map(|g| g.projects.iter())
        .filter(|p| show_done || !project_finished(p))
        .flat_map(|p| p.tasks.iter())
        .filter(|t| show_done || !t.schedule.done)
        .filter(|t| match first(t) {
            Some(a) => a == person,
            None => person == UNASSIGNED.id,
        })
        .collect();
    out.sort_by(|a, b| {
        a.schedule
            .es
            .total_cmp(&b.schedule.es)
            .then_with(|| a.schedule.title.cmp(&b.schedule.title))
    });
    out
}

/// The rows *By person*: each person (and *Not assigned*, when anything is), with their tasks.
pub fn rows_by_person<'a>(
    data: &'a GanttData,
    toggled: &HashSet<Uuid>,
    show_done: bool,
) -> Vec<Row<'a>> {
    let mut out = Vec::new();
    let unassigned = !person_tasks(data, UNASSIGNED.id, show_done).is_empty();
    for p in data.people.iter().chain(unassigned.then_some(&*UNASSIGNED)) {
        out.push(Row {
            kind: RowKind::Person(p),
            key: p.id,
            depth: 0,
        });
        if !is_open(true, p.id, toggled) {
            continue;
        }
        for t in person_tasks(data, p.id, show_done) {
            out.push(Row {
                kind: RowKind::Task(t),
                key: t.schedule.id,
                depth: 1,
            });
        }
    }
    out
}

/// The rows of the chart: the plan's tree, or by person.
pub fn tree<'a>(
    data: &'a GanttData,
    toggled: &HashSet<Uuid>,
    show_done: bool,
    by_person: bool,
) -> Vec<Row<'a>> {
    if by_person {
        rows_by_person(data, toggled, show_done)
    } else {
        rows(data, toggled, show_done)
    }
}

/// The rows that can be opened (objectives and projects with something under them).
pub fn can_open(row: &Row) -> bool {
    match row.kind {
        RowKind::Group(g) => !g.projects.is_empty(),
        RowKind::Project(p) => !p.tasks.is_empty(),
        RowKind::Person(_) => true,
        RowKind::Task(_) | RowKind::Add(_) => false,
    }
}

/// The toggles that show every task (every project open, objectives as they are).
pub fn all_open(data: &GanttData) -> HashSet<Uuid> {
    data.groups
        .iter()
        .flat_map(|g| g.projects.iter())
        .filter(|p| !p.needs_a_look)
        .map(|p| p.id)
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
        if is_open(p.needs_a_look, p.id, toggled) {
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
        // A toggle means "differs from how it starts".
        if open == p.needs_a_look {
            out.remove(&p.id);
        } else {
            out.insert(p.id);
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
    pub fn new(data: &GanttData, ppd: f64) -> Self {
        Self {
            first: data.first_offset as f64,
            ppd,
        }
    }

    pub fn x(&self, offset: f64) -> f64 {
        (offset - self.first) * self.ppd
    }

    /// The offset of the working day under `x` pixels.
    pub fn day_under(&self, x: f64) -> i64 {
        (x / self.ppd).floor() as i64 + self.first as i64
    }
}

/// How wide a working day is drawn (spec 58): *Fit* the whole axis into the chart's width, or
/// the fixed *Days* and *Weeks*.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Scale {
    Fit,
    Days,
    Weeks,
    /// A step of `Ctrl+wheel` zoom between *Weeks* and *Days* ([`ZOOM_STEPS`]).
    Step(u8),
}

impl Scale {
    pub const ALL: [Scale; 3] = [Scale::Fit, Scale::Days, Scale::Weeks];

    pub fn label(self) -> &'static str {
        match self {
            Scale::Fit => "Fit",
            Scale::Days => "Days",
            Scale::Weeks => "Weeks",
            Scale::Step(_) => "Zoom",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Scale::Fit => "Fit the whole plan into the width of the chart",
            Scale::Days => "One wide column per working day: names on the bars, edges to grab",
            Scale::Weeks => "Narrow days, for a long plan",
            Scale::Step(_) => "Between Weeks and Days (Ctrl+wheel)",
        }
    }

    /// Pixels per working day for an axis of `days` in a chart `view_w` pixels wide.
    pub fn ppd(self, days: usize, view_w: f64) -> f64 {
        match self {
            Scale::Fit => fit_ppd(days, view_w),
            Scale::Days => DAYS_PPD,
            Scale::Weeks => WEEKS_PPD,
            Scale::Step(i) => ZOOM_STEPS[usize::from(i).min(ZOOM_STEPS.len() - 1)],
        }
    }
}

/// The width of a day that fits `days` into `view_w`: never wider than *Days*, never thinner
/// than [`FIT_MIN_PPD`] (a very long plan still scrolls).
pub fn fit_ppd(days: usize, view_w: f64) -> f64 {
    if days == 0 || view_w <= 0.0 {
        return DAYS_PPD;
    }
    (view_w / days as f64).clamp(FIT_MIN_PPD, DAYS_PPD)
}

pub fn chart_width(data: &GanttData, ppd: f64) -> f64 {
    data.days.len() as f64 * ppd
}

/// Whether the zoom is wide enough to grab the edge of a bar.
pub fn can_resize(ppd: f64) -> bool {
    ppd >= 20.0
}

/// Whether the zoom is wide enough to write names next to the bars.
pub fn shows_names(ppd: f64) -> bool {
    ppd >= NAMES_MIN_PPD
}

/// Where to scroll so today sits about a sixth of the way into a chart `view_w` wide.
pub fn today_scroll(today_x: f64, view_w: f64) -> f64 {
    (today_x - view_w.max(0.0) / 6.0).max(0.0)
}

// ------------------------------------------------------------------------------ the axis

/// A mark on the date axis and what it says (empty when there is no room to say it).
#[derive(Clone, Debug, PartialEq)]
pub struct AxisMark {
    pub x: f64,
    pub label: String,
}

/// The two rows of the header (spec 58): months on top; below, a number for every working day
/// when there is room (`days`), else the first day of each week (`weeks`, always there as lines).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Axis {
    pub months: Vec<AxisMark>,
    pub weeks: Vec<AxisMark>,
    /// The day numbers, centred on their column (empty when the days are too narrow).
    pub days: Vec<AxisMark>,
}

fn short_month(d: Date) -> &'static str {
    let name = MONTHS[(d.month() as usize).saturating_sub(1).min(11)];
    &name[..3]
}

/// Keeps a label only if it doesn't run into the one before it.
fn thin(marks: &mut [AxisMark], gap: f64) {
    let mut free_from = f64::NEG_INFINITY;
    for m in marks.iter_mut() {
        if m.label.is_empty() {
            continue;
        }
        if m.x < free_from {
            m.label.clear();
        } else {
            free_from = m.x + m.label.chars().count() as f64 * CHAR_W + gap;
        }
    }
}

pub fn axis(days: &[Date], ppd: f64) -> Axis {
    let mut out = Axis::default();
    let mut month_starts = Vec::new();
    let numbers = ppd >= DAY_NUMBERS_MIN_PPD;
    for (i, d) in days.iter().enumerate() {
        let x = i as f64 * ppd;
        let prev = i.checked_sub(1).and_then(|p| days.get(p));
        if prev.is_none_or(|p| p.month() != d.month()) {
            let label = if prev.is_none() || d.month() == time::Month::January {
                format!("{} {}", short_month(*d), d.year())
            } else {
                short_month(*d).to_owned()
            };
            out.months.push(AxisMark { x, label });
            month_starts.push(*d);
        }
        // A new (Monday-first) week starts at the first working day after the weekday number
        // goes back down: Monday normally, Tuesday when Monday isn't worked.
        let new_week = prev.is_none_or(|p| {
            d.weekday().number_days_from_monday() < p.weekday().number_days_from_monday()
        });
        if new_week {
            out.weeks.push(AxisMark {
                x,
                label: d.day().to_string(),
            });
        }
        if numbers {
            out.days.push(AxisMark {
                x: x + ppd / 2.0,
                label: d.day().to_string(),
            });
        }
    }
    thin_months(&mut out.months, &month_starts);
    thin(&mut out.weeks, 6.0);
    out
}

/// Month names stay only where they fit before the next month starts (a last few days of a month
/// at the start of the axis give way to the next month), and the first one shown says the year.
/// `starts` is the first day of each month mark.
fn thin_months(marks: &mut [AxisMark], starts: &[Date]) {
    let label_w = |m: &AxisMark| m.label.chars().count() as f64 * CHAR_W + 8.0;
    for i in 0..marks.len() {
        let next = marks.get(i + 1).map(|n| n.x);
        if let Some(m) = marks.get_mut(i) {
            if label_w(m) > next.map_or(f64::INFINITY, |n| n - m.x) {
                m.label.clear();
            }
        }
    }
    if let Some((m, d)) = marks
        .iter_mut()
        .zip(starts)
        .find(|(m, _)| !m.label.is_empty())
    {
        if !m.label.contains(' ') {
            m.label = format!("{} {}", m.label, d.year());
        }
    }
}

// ---------------------------------------------------------------------- words on the chart

/// `text` cut to fit `avail` pixels of chart text, with an ellipsis; none if hardly a word fits.
pub fn fit_text(text: &str, avail: f64) -> Option<String> {
    let max = (avail / CHAR_W).floor().max(0.0) as usize;
    let len = text.chars().count();
    if len <= max {
        return (len > 0).then(|| text.to_owned());
    }
    if max < 4 {
        return None;
    }
    let mut out: String = text.chars().take(max - 1).collect();
    out.truncate(out.trim_end().len());
    out.push('…');
    Some(out)
}

/// The name written after a bar that ends at `end_x` (and a `+2d` chip, if any), in what is left
/// of a chart `chart_w` wide: where it starts and what it says.
pub fn bar_name(text: &str, end_x: f64, chip: bool, chart_w: f64) -> Option<(f64, String)> {
    let x = end_x + if chip { 34.0 } else { 6.0 };
    fit_text(text, chart_w - x - 4.0).map(|t| (x, t))
}

/// How much of a summary bar `wd` wide is filled for the work done.
pub fn progress_w(wd: f64, done: u32, open: u32) -> f64 {
    let all = done + open;
    if all == 0 {
        0.0
    } else {
        wd * f64::from(done) / f64::from(all)
    }
}

/// The tasks done and open under an objective, for its summary bar.
pub fn group_progress(g: &GanttGroup) -> (u32, u32) {
    g.projects
        .iter()
        .fold((0, 0), |(d, o), p| (d + p.done_tasks, o + p.open_tasks))
}

/// Where an ongoing objective's line runs, from the end of its work (or today): a few weeks,
/// fading out, never past the chart.
pub fn fade_span(from: f64, ppd: f64, chart_w: f64) -> (f64, f64) {
    (from, (FADE_DAYS * ppd).min(chart_w - from).max(0.0))
}

// ---------------------------------------------------------------------- the label column

/// The label column kept between its limits.
pub fn clamp_label_w(w: f64) -> f64 {
    if w.is_finite() {
        w.clamp(LABEL_MIN, LABEL_MAX)
    } else {
        LABEL_W
    }
}

/// The counts column, when the label column is wide enough to spare it (else the tooltip has them).
pub fn counts_w(label_w: f64) -> f64 {
    if label_w >= 280.0 {
        COUNTS_W
    } else {
        0.0
    }
}

/// The label column wide enough for the longest name: rows as (depth, characters in the name).
pub fn fit_label_w(rows: impl IntoIterator<Item = (u8, usize)>) -> f64 {
    let widest = rows
        .into_iter()
        .map(|(depth, chars)| 6.0 + f64::from(depth) * 14.0 + 26.0 + chars as f64 * LABEL_CHAR_W)
        .fold(0.0, f64::max);
    clamp_label_w(widest + COUNTS_W + 8.0)
}

/// One of a project's counts, for the counts column: open, blocked or done.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Count {
    Open(u32),
    Blocked(u32),
    Done(u32),
}

impl Count {
    /// The short form in the column: "14", "3⊘", "5✓".
    pub fn short(self) -> String {
        match self {
            Count::Open(n) => n.to_string(),
            Count::Blocked(n) => format!("{n}\u{2298}"),
            Count::Done(n) => format!("{n}\u{2713}"),
        }
    }

    /// The same in words, for the tooltip.
    pub fn words(self) -> String {
        match self {
            Count::Open(n) => format!("{n} open"),
            Count::Blocked(n) => format!("{n} blocked"),
            Count::Done(n) => format!("{n} done"),
        }
    }
}

/// A project's counts: always the open tasks, then the blocked and done ones when there are any.
pub fn count_parts(p: &GanttProject) -> Vec<Count> {
    let mut out = vec![Count::Open(p.open_tasks)];
    if p.blocked_tasks > 0 {
        out.push(Count::Blocked(p.blocked_tasks));
    }
    if p.done_tasks > 0 {
        out.push(Count::Done(p.done_tasks));
    }
    out
}

/// What an empty row says on the chart (spec 58): an objective with no projects, a project with
/// nothing left to do.
pub fn empty_text(row: &Row) -> Option<&'static str> {
    match row.kind {
        RowKind::Group(g) if g.objective.is_some() && g.projects.is_empty() => {
            Some("No projects yet")
        }
        RowKind::Project(p) if p.open_tasks == 0 && !project_finished(p) => Some("No open tasks"),
        _ => None,
    }
}

/// The quick-add line that adds a project to an objective; the name goes at the end.
pub fn add_project_text(objective: &str) -> String {
    format!("project for:\"{}\" ", objective.replace('"', "'"))
}

/// The quick-add line that adds a task to a project; the name goes at the end.
pub fn add_task_text(slug: &str) -> String {
    format!("task #{slug} ")
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
pub fn elbow(x1: f64, y1: f64, x2: f64, y2: f64, row_h: f64) -> String {
    if (y2 - y1).abs() < EPS {
        format!("M{x1} {y1}H{x2}")
    } else if x2 >= x1 + 12.0 {
        format!("M{x1} {y1}H{}V{y2}H{x2}", x1 + 6.0)
    } else {
        // The next bar starts before this one ends (a lag of nothing, or a finished task):
        // down between the rows and in from the left.
        let ym = y1 + (y2 - y1).signum() * row_h / 2.0;
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

// -------------------------------------------------------------------- spec 58, phase 2

/// How tall the rows are: compact (the default) or comfortable, for touchpads and low vision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Density {
    Compact,
    Comfortable,
}

/// The row and bar heights of a density.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dims {
    pub row_h: f64,
    pub bar_h: f64,
}

impl Density {
    pub fn dims(self) -> Dims {
        match self {
            Density::Compact => Dims {
                row_h: ROW_H,
                bar_h: BAR_H,
            },
            Density::Comfortable => Dims {
                row_h: 36.0,
                bar_h: 16.0,
            },
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Density::Compact => "compact",
            Density::Comfortable => "comfortable",
        }
    }

    pub fn from_id(id: &str) -> Self {
        if id == "comfortable" {
            Density::Comfortable
        } else {
            Density::Compact
        }
    }
}

/// The least a target to press may measure, either way (WCAG 2.5.8).
pub const HIT: f64 = 24.0;

/// A span along the axis: x and width.
pub type Span = (f64, f64);

/// Where a press on a task bar counts as which part: the left edge, the middle, the right edge.
/// The edges are each [`HIT`] wide, reaching out past the bar and at most a quarter of it in, so
/// they never meet and the middle keeps at least half the bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TaskHits {
    pub start: Span,
    pub body: Span,
    pub end: Span,
}

pub fn task_hits(x: f64, wd: f64) -> TaskHits {
    let inner = (wd / 4.0).min(HIT / 2.0);
    TaskHits {
        start: (x - (HIT - inner), HIT),
        body: (x + inner, wd - 2.0 * inner),
        end: (x + wd - inner, HIT),
    }
}

/// The press area of a whole bar when its edges can't be grabbed: the bar, widened to [`HIT`].
pub fn body_hit(x: f64, wd: f64) -> Span {
    let pad = ((HIT - wd) / 2.0).max(0.0);
    (x - pad, wd + 2.0 * pad)
}

/// The press areas of a project's start mark and its target flag: [`HIT`] wide around each, and
/// split halfway between them when they are closer than that, so neither covers the other.
pub fn band_handles(start_x: Option<f64>, target_x: Option<f64>) -> (Option<Span>, Option<Span>) {
    let half = HIT / 2.0;
    match (start_x, target_x) {
        (Some(s), Some(t)) if t - s < HIT => {
            let mid = (s + t) / 2.0;
            (
                Some((s - half, mid - (s - half))),
                Some((mid, t + half - mid)),
            )
        }
        (s, t) => (s.map(|s| (s - half, HIT)), t.map(|t| (t - half, HIT))),
    }
}

/// The first and last working day of a band's work.
pub fn band_span(data: &GanttData, b: &Band) -> (Option<Date>, Option<Date>) {
    (
        b.work_from.map(|o| day_at(data, o.floor() as i64)),
        b.work_to.map(|o| day_at(data, o.ceil() as i64 - 1)),
    )
}

fn plural(n: u32, one: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {one}s")
    }
}

/// What a row says to a screen reader: everything its bar shows, in words.
pub fn row_label(data: &GanttData, row: &Row) -> String {
    let mut parts: Vec<String> = Vec::new();
    match row.kind {
        RowKind::Group(g) => {
            parts.push(group_title(g));
            let ongoing = g.objective.as_ref().is_some_and(|o| o.ongoing);
            parts.push(
                if ongoing {
                    "ongoing objective"
                } else {
                    "objective"
                }
                .to_owned(),
            );
            if g.objective.is_some() && g.projects.is_empty() {
                parts.push("no projects yet".to_owned());
            } else {
                parts.push(plural(g.projects.len() as u32, "project"));
            }
            if let (_, Some(end)) = band_span(data, &g.band) {
                parts.push(format!("work until {}", short_day(end)));
            }
            if let Some(t) = g.band.target_date {
                parts.push(format!("target {}", short_day(t)));
            }
            if let Some(n) = g.band.late_by_days {
                parts.push(format!("{} over", plural(n, "working day")));
            }
        }
        RowKind::Project(p) => {
            parts.push(p.title.clone());
            parts.push("project".to_owned());
            if empty_text(row).is_some() {
                parts.push("no open tasks".to_owned());
            } else {
                parts.extend(count_parts(p).into_iter().map(|c| match c {
                    Count::Open(n) => format!("{n} open task{}", if n == 1 { "" } else { "s" }),
                    other => other.words(),
                }));
            }
            if let (_, Some(end)) = band_span(data, &p.band) {
                parts.push(format!("finishes {}", short_day(end)));
            }
            if let Some(t) = p.band.target_date {
                parts.push(format!("target {}", short_day(t)));
            }
            if let Some((text, _)) = room_text(&p.zone, p.band.late_by_days) {
                parts.push(text);
            }
        }
        RowKind::Task(t) => {
            let s = &t.schedule;
            parts.push(s.title.clone());
            parts.push(
                if t.parent.is_some() {
                    "sub-task"
                } else {
                    "task"
                }
                .to_owned(),
            );
            parts.push(status_word(s.status).to_lowercase());
            if s.critical && !s.done {
                parts.push("critical".to_owned());
            }
            parts.push(format!("{} to {}", short_day(s.start), short_day(s.finish)));
            if let Some(d) = t.due_date {
                parts.push(format!("due {}", short_day(d)));
            }
            if t.overdue {
                parts.push("overdue".to_owned());
            }
            if let Some(n) = s.late_by_days.filter(|_| !s.done) {
                parts.push(format!("{} late", plural(n, "working day")));
            }
            if s.unestimated {
                parts.push("no estimate".to_owned());
            }
        }
        RowKind::Add(p) => {
            parts.push(format!(
                "Add a task to {}: drag across the chart for its dates, or use its button",
                p.title
            ));
        }
        RowKind::Person(p) => {
            parts.push(p.title.clone());
            parts.push("person".to_owned());
            parts.push(plural(
                person_tasks(data, p.id, false).len() as u32,
                "open task",
            ));
        }
    }
    parts.join(", ")
}

/// What a fold or an opening says aloud: what is now shown under the row.
pub fn fold_text(row: &Row, open: bool, show_done: bool) -> String {
    let (title, under) = match row.kind {
        RowKind::Group(g) => {
            let n = g
                .projects
                .iter()
                .filter(|p| show_done || !project_finished(p))
                .count();
            (group_title(g), plural(n as u32, "project"))
        }
        RowKind::Project(p) => {
            let n = p
                .tasks
                .iter()
                .filter(|t| show_done || !t.schedule.done)
                .count();
            (p.title.clone(), plural(n as u32, "task"))
        }
        RowKind::Person(p) => (p.title.clone(), String::from("their tasks")),
        RowKind::Task(t) => (t.schedule.title.clone(), String::new()),
        RowKind::Add(p) => (p.title.clone(), String::new()),
    };
    if open {
        format!("{title} opened: {under}")
    } else {
        format!("{title} folded")
    }
}

/// The cells of a row in the table view.
#[derive(Clone, Debug, PartialEq)]
pub struct Cells {
    pub kind: &'static str,
    pub status: String,
    pub start: Option<Date>,
    pub finish: Option<Date>,
    pub due: Option<Date>,
    /// Room or lateness, and whether it is bad news.
    pub room: Option<(String, bool)>,
    pub critical: bool,
}

pub fn cells(data: &GanttData, row: &Row) -> Cells {
    match row.kind {
        RowKind::Group(g) => {
            let (start, finish) = band_span(data, &g.band);
            let ongoing = g.objective.as_ref().is_some_and(|o| o.ongoing);
            Cells {
                kind: "Objective",
                status: match &g.objective {
                    Some(_) if ongoing => "Ongoing".to_owned(),
                    Some(o) => objective_status_label(o.status).to_owned(),
                    None => String::new(),
                },
                start,
                finish,
                due: g.band.target_date,
                room: room_chip(&Zone::None, g.band.late_by_days),
                critical: false,
            }
        }
        RowKind::Project(p) => {
            let (start, finish) = band_span(data, &p.band);
            Cells {
                kind: "Project",
                status: project_status_label(p.status).to_owned(),
                start: p.band.start_date.or(start),
                finish,
                due: p.band.target_date,
                room: room_chip(&p.zone, p.band.late_by_days),
                critical: false,
            }
        }
        RowKind::Task(t) => {
            let s = &t.schedule;
            let room = match (s.late_by_days.filter(|_| !s.done), t.overdue) {
                (Some(n), _) => Some((format!("late {n}d"), true)),
                (None, true) => Some(("overdue".to_owned(), true)),
                _ => None,
            };
            Cells {
                kind: if t.parent.is_some() {
                    "Sub-task"
                } else {
                    "Task"
                },
                status: status_word(s.status).to_owned(),
                start: Some(s.start),
                finish: Some(s.finish),
                due: t.due_date,
                room,
                critical: s.critical && !s.done,
            }
        }
        // Never in the table (no add rows, no rows by person there), but words all the same.
        RowKind::Add(_) | RowKind::Person(_) => Cells {
            kind: if matches!(row.kind, RowKind::Person(_)) {
                "Person"
            } else {
                ""
            },
            status: String::new(),
            start: None,
            finish: None,
            due: None,
            room: None,
            critical: false,
        },
    }
}

/// A column the table can sort its tasks by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortKey {
    Name,
    Status,
    Start,
    Finish,
    Due,
    Late,
}

/// Every task the table shows, flat, in the order of `key` (ties keep the plan's order).
pub fn sorted_tasks(
    data: &GanttData,
    show_done: bool,
    key: SortKey,
    descending: bool,
) -> Vec<&GanttTask> {
    let mut out: Vec<&GanttTask> = data
        .groups
        .iter()
        .flat_map(|g| g.projects.iter())
        .filter(|p| show_done || !project_finished(p))
        .flat_map(|p| p.tasks.iter())
        .filter(|t| show_done || !t.schedule.done)
        .collect();
    out.sort_by(|a, b| {
        let (x, y) = (&a.schedule, &b.schedule);
        let order = match key {
            SortKey::Name => x.title.to_lowercase().cmp(&y.title.to_lowercase()),
            SortKey::Status => status_word(x.status).cmp(status_word(y.status)),
            SortKey::Start => x.start.cmp(&y.start),
            SortKey::Finish => x.finish.cmp(&y.finish),
            // No due date sorts last either way.
            SortKey::Due => match (a.due_date, b.due_date) {
                (Some(p), Some(q)) => p.cmp(&q),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            },
            SortKey::Late => x
                .late_by_days
                .unwrap_or(0)
                .cmp(&y.late_by_days.unwrap_or(0)),
        };
        if descending && !(key == SortKey::Due && (a.due_date.is_none() || b.due_date.is_none())) {
            order.reverse()
        } else {
            order
        }
    });
    out
}

/// Why a project's or objective's date can't move a working day from the keyboard, if it can't.
/// `part` is the target (`Alt+←/→`) or the start (`Alt+Shift+←/→`, projects only).
pub fn band_step(
    data: &GanttData,
    b: &Band,
    part: BandPart,
    delta: i64,
    title: &str,
    ongoing: bool,
) -> Result<(Option<Date>, Option<Date>), String> {
    match part {
        BandPart::Target if ongoing => {
            return Err(format!("{title} is ongoing: it has no target to move."))
        }
        BandPart::Target if b.target_date.is_none() => {
            return Err(format!(
                "{title} has no target date yet. Set one in the detail pane."
            ))
        }
        BandPart::Start if b.start_date.is_none() => {
            return Err(format!(
                "{title} has no start date yet. Set one in the detail pane."
            ))
        }
        _ => {}
    }
    match band_dates(data, b, part, delta) {
        (None, None) => Err(match part {
            BandPart::Start => format!("{title}: the start can't pass the target."),
            _ => format!("{title}: the target can't pass the start."),
        }),
        dates => Ok(dates),
    }
}

/// Why a task can't move from the keyboard or the date box, if it can't (`None`: it can).
pub fn task_step_refusal(data: &GanttData, t: &GanttTask, c: &Candidate) -> Option<String> {
    let s = &t.schedule;
    if s.done {
        return Some(format!("{} is finished: reopen it to move it.", s.title));
    }
    if c.stopped && !c.changed {
        return Some(format!("{} {}.", s.title, stop_reason(data, t)));
    }
    (!c.changed).then(|| format!("{} is already there.", s.title))
}

/// The working day on or after `d` (a date picked in the date box may be a weekend).
pub fn working_day_on_or_after(data: &GanttData, d: Date) -> Option<i64> {
    let i = data.days.partition_point(|x| *x < d);
    (i < data.days.len()).then(|| data.first_offset + i as i64)
}

// -------------------------------------------------------------------- spec 58, phase 3

/// What the Gantt is narrowed to: one person's work, one objective, what needs a look.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ViewFilter {
    pub person: Option<Uuid>,
    /// An objective, or [`NO_OBJECTIVE`] for the projects that serve none.
    pub objective: Option<Uuid>,
    pub needs_a_look: bool,
}

impl ViewFilter {
    pub fn is_empty(&self) -> bool {
        *self == ViewFilter::default()
    }
}

/// What the Gantt toolbar keeps on this device between visits: chart or table, scale, rows,
/// arrows and the *Show* filters. Not the find box (typed for one look), the folds or the row
/// height (which has its own key). A value that cannot be read leaves the default.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Remembered {
    /// *Table* instead of *Chart*.
    pub table: bool,
    pub scale: Scale,
    pub show_done: bool,
    pub by_person: bool,
    pub arrows: LinkMode,
    pub filter: RememberedFilter,
}

/// [`ViewFilter`] as stored.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct RememberedFilter {
    pub person: Option<Uuid>,
    pub objective: Option<Uuid>,
    pub needs_a_look: bool,
}

impl Default for Remembered {
    fn default() -> Self {
        Self {
            table: false,
            scale: Scale::Days,
            show_done: false,
            by_person: false,
            arrows: LinkMode::Focus,
            filter: RememberedFilter::default(),
        }
    }
}

impl Remembered {
    /// The stored text, or the defaults when there is none or it cannot be read.
    pub fn from_json(json: Option<&str>) -> Self {
        json.and_then(|j| serde_json::from_str(j).ok())
            .unwrap_or_default()
    }

    pub fn to_json(&self) -> Option<String> {
        serde_json::to_string(self).ok()
    }

    pub fn view_filter(&self) -> ViewFilter {
        ViewFilter {
            person: self.filter.person,
            objective: self.filter.objective,
            needs_a_look: self.filter.needs_a_look,
        }
    }

    /// How many of the choices differ from the defaults (what *Reset* would put back).
    pub fn changed(&self) -> usize {
        let d = Remembered::default();
        [
            self.table != d.table,
            self.scale != d.scale,
            self.show_done != d.show_done,
            self.by_person != d.by_person,
            self.arrows != d.arrows,
            self.filter.person.is_some(),
            self.filter.objective.is_some(),
            self.filter.needs_a_look,
        ]
        .into_iter()
        .filter(|c| *c)
        .count()
    }
}

/// The plan narrowed by `f`: a person keeps the tasks assigned to them (and the projects and
/// objectives those sit under, counts recounted); an objective keeps its group; *needs a look*
/// keeps those projects. Links stay only between what is left.
pub fn filtered(data: &GanttData, f: &ViewFilter) -> GanttData {
    if f.is_empty() {
        return data.clone();
    }
    let mut out = data.clone();
    out.groups
        .retain(|g| f.objective.is_none_or(|o| group_key(g) == o));
    for g in &mut out.groups {
        if f.needs_a_look {
            g.projects.retain(|p| p.needs_a_look);
        }
        if let Some(person) = f.person {
            for p in &mut g.projects {
                p.tasks.retain(|t| t.assignees.contains(&person));
                let kept: HashSet<Uuid> = p.tasks.iter().map(|t| t.schedule.id).collect();
                for t in &mut p.tasks {
                    t.parent = t.parent.filter(|x| kept.contains(x));
                }
                let open: Vec<&GanttTask> = p.tasks.iter().filter(|t| !t.schedule.done).collect();
                p.open_tasks = open.len() as u32;
                p.blocked_tasks = open
                    .iter()
                    .filter(|t| t.schedule.status == TaskStatus::Blocked)
                    .count() as u32;
                p.done_tasks = (p.tasks.len() - open.len()) as u32;
            }
            g.projects.retain(|p| !p.tasks.is_empty());
        }
    }
    if f.person.is_some() || f.needs_a_look {
        out.groups.retain(|g| !g.projects.is_empty());
    }
    let present: HashSet<Uuid> = out
        .groups
        .iter()
        .flat_map(|g| g.projects.iter())
        .flat_map(|p| std::iter::once(p.id).chain(p.tasks.iter().map(|t| t.schedule.id)))
        .collect();
    out.links
        .retain(|l| present.contains(&l.from) && present.contains(&l.to));
    out
}

fn matches_query(title: &str, query: &str) -> bool {
    let hay = title.to_lowercase();
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    !words.is_empty() && words.iter().all(|w| hay.contains(w.as_str()))
}

/// Everything whose name has every word of `query`, in the order the plan draws it: objectives,
/// then their projects, then the projects' tasks.
pub fn find_matches(data: &GanttData, query: &str, show_done: bool) -> Vec<Uuid> {
    let mut out = Vec::new();
    for g in &data.groups {
        if g.objective.is_some() && matches_query(&group_title(g), query) {
            out.push(group_key(g));
        }
        for p in g
            .projects
            .iter()
            .filter(|p| show_done || !project_finished(p))
        {
            if matches_query(&p.title, query) {
                out.push(p.id);
            }
            for t in p.tasks.iter().filter(|t| show_done || !t.schedule.done) {
                if matches_query(&t.schedule.title, query) {
                    out.push(t.schedule.id);
                }
            }
        }
    }
    out
}

/// `toggled` with what is needed opened so every one of `found` is on a row: the objective of
/// a project, the objective and project of a task. Nothing is folded.
pub fn unfold_for(data: &GanttData, toggled: &HashSet<Uuid>, found: &[Uuid]) -> HashSet<Uuid> {
    let want: HashSet<Uuid> = found.iter().copied().collect();
    let mut out = toggled.clone();
    for g in &data.groups {
        let key = group_key(g);
        for p in &g.projects {
            let task_found = p.tasks.iter().any(|t| want.contains(&t.schedule.id));
            if task_found || want.contains(&p.id) {
                // Objectives start open: open is "not toggled".
                out.remove(&key);
            }
            if task_found && !is_open(p.needs_a_look, p.id, &out) {
                if p.needs_a_look {
                    out.remove(&p.id);
                } else {
                    out.insert(p.id);
                }
            }
        }
    }
    out
}

/// The steps of `Ctrl+wheel` zoom, in pixels per working day: *Weeks* first, *Days* last.
pub const ZOOM_STEPS: [f64; 5] = [WEEKS_PPD, 11.0, 15.0, 18.0, DAYS_PPD];

/// The scale one zoom step in (`dir > 0`) or out from `ppd`, stopping at *Days* and *Weeks*.
pub fn zoom(ppd: f64, dir: i32) -> Scale {
    let next = if dir > 0 {
        ZOOM_STEPS
            .iter()
            .copied()
            .find(|s| *s > ppd + EPS)
            .unwrap_or(DAYS_PPD)
    } else {
        ZOOM_STEPS
            .iter()
            .rev()
            .copied()
            .find(|s| *s < ppd - EPS)
            .unwrap_or(WEEKS_PPD)
    };
    scale_at(next)
}

/// The scale whose days are `ppd` wide: a named one, or a step between them.
pub fn scale_at(ppd: f64) -> Scale {
    if (ppd - DAYS_PPD).abs() < EPS {
        Scale::Days
    } else if (ppd - WEEKS_PPD).abs() < EPS {
        Scale::Weeks
    } else {
        let i = ZOOM_STEPS
            .iter()
            .position(|s| (s - ppd).abs() < EPS)
            .unwrap_or(2);
        Scale::Step(i as u8)
    }
}

/// Where to scroll so the day under the pointer (`at` pixels into the view) stays under it.
pub fn zoom_scroll(at: f64, scroll_left: f64, old_ppd: f64, new_ppd: f64) -> f64 {
    ((scroll_left + at) / old_ppd * new_ppd - at).max(0.0)
}

/// Where an offset falls on the overview strip, `strip_w` wide for the whole axis.
pub fn strip_x(data: &GanttData, offset: f64, strip_w: f64) -> f64 {
    let n = data.days.len().max(1) as f64;
    ((offset - data.first_offset as f64) / n * strip_w).clamp(0.0, strip_w)
}

/// The box on the overview strip for what the chart shows: x and width.
pub fn strip_box(scroll_left: f64, view_w: f64, chart_w: f64, strip_w: f64) -> (f64, f64) {
    if chart_w <= 0.0 {
        return (0.0, strip_w);
    }
    let x = (scroll_left / chart_w * strip_w).clamp(0.0, strip_w);
    let w = (view_w / chart_w * strip_w).clamp(4.0, strip_w);
    (x.min(strip_w - w), w)
}

/// Where to scroll the chart so the box is centred on `x` of the strip.
pub fn strip_scroll(x: f64, strip_w: f64, chart_w: f64, view_w: f64) -> f64 {
    if strip_w <= 0.0 {
        return 0.0;
    }
    (x / strip_w * chart_w - view_w / 2.0).clamp(0.0, (chart_w - view_w).max(0.0))
}

/// The quick-add line for a task drawn on a project's add row: the dates from the first to the
/// last working day dragged across, the estimate the working days between.
pub fn add_task_line(data: &GanttData, slug: &str, from_offset: i64, to_offset: i64) -> String {
    let (a, b) = (from_offset.min(to_offset), from_offset.max(to_offset));
    let (start, end) = (day_at(data, a), day_at(data, b));
    let days = data
        .days
        .iter()
        .filter(|d| **d >= start && **d <= end)
        .count()
        .max(1);
    format!("task #{slug} start:{start} due:{end} est:{days}d ")
}

/// The weeks a person is over their capacity, as spans of offsets on the axis with the load:
/// (first offset, offset after the last, load %). Weeks that fall off the axis are left out.
/// Weeks over capacity per person: (first offset, offset after the last, load %).
pub type OverWeeks = std::collections::HashMap<Uuid, Vec<(i64, i64, f64)>>;

pub fn over_weeks(data: &GanttData, cap: &Capacity) -> OverWeeks {
    let last = data.first_offset + data.days.len() as i64;
    let mut out = OverWeeks::default();
    for p in &cap.people {
        for w in p.weeks.iter().filter(|w| w.over) {
            let from = working_day_on_or_after(data, w.week_start);
            let to = w
                .week_start
                .checked_add(time::Duration::days(7))
                .and_then(|next| working_day_on_or_after(data, next))
                .unwrap_or(last);
            if let Some(from) = from.filter(|f| *f < to) {
                out.entry(p.person.node.id)
                    .or_default()
                    .push((from, to, w.load_pct));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembered_toolbar_round_trips_and_counts_changes() {
        assert_eq!(Remembered::default().changed(), 0);
        let now = Remembered {
            table: true,
            scale: Scale::Step(2),
            by_person: true,
            arrows: LinkMode::Critical,
            filter: RememberedFilter {
                person: Some(Uuid::from_u128(7)),
                objective: None,
                needs_a_look: true,
            },
            ..Remembered::default()
        };
        assert_eq!(now.changed(), 6);
        let back = Remembered::from_json(now.to_json().as_deref());
        assert_eq!(back, now);
        let f = back.view_filter();
        assert_eq!(f.person, Some(Uuid::from_u128(7)));
        assert!(f.needs_a_look);
    }

    #[test]
    fn remembered_toolbar_ignores_what_it_cannot_read() {
        assert_eq!(Remembered::from_json(None), Remembered::default());
        assert_eq!(Remembered::from_json(Some("{nope")), Remembered::default());
        // A missing field keeps its default.
        let partial = Remembered::from_json(Some(r#"{"show_done":true}"#));
        assert!(partial.show_done);
        assert_eq!(partial.scale, Scale::Days);
    }
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
            assignees: Vec::new(),
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
            needs_a_look: false,
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
            people: Vec::new(),
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
        // An open project ends with its add row.
        assert_eq!(kinds(&HashSet::from([id(10)])), [0, 1, 2, 2]);
        assert_eq!(kinds(&HashSet::from([id(1)])), [0]);
        assert_eq!(kinds(&all_open(&d)), [0, 1, 2, 2]);
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
        // Objective, project, its open task and its add row; a finished project gets no add row.
        assert_eq!(rows(&d, &open, false).len(), 4);
        assert_eq!(rows(&d, &open, true).len(), 6);
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
        assert_eq!(depths, [0, 1, 2, 3, 2]);
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
        assert_eq!(elbow(10.0, 13.0, 40.0, 39.0, ROW_H), "M10 13H16V39H40");
        assert_eq!(elbow(10.0, 13.0, 30.0, 13.0, ROW_H), "M10 13H30");
        let back = elbow(50.0, 13.0, 20.0, 39.0, ROW_H);
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
        assert!(can_resize(Scale::Days.ppd(40, 800.0)));
        assert!(!can_resize(Scale::Weeks.ppd(40, 800.0)));
        assert!(shows_names(DAYS_PPD) && !shows_names(WEEKS_PPD));
    }

    #[test]
    fn fit_fills_the_width_but_never_beyond_days() {
        assert_eq!(fit_ppd(100, 1000.0), 10.0);
        assert_eq!(
            fit_ppd(10, 1000.0),
            DAYS_PPD,
            "a short plan is not stretched"
        );
        assert_eq!(fit_ppd(1500, 600.0), 1.5, "a very long plan still scrolls");
        assert_eq!(fit_ppd(0, 600.0), DAYS_PPD);
        assert_eq!(fit_ppd(100, 0.0), DAYS_PPD, "not measured yet");
        assert_eq!(Scale::Fit.ppd(100, 1000.0), 10.0);
        assert!(!can_resize(10.0));
    }

    #[test]
    fn today_opens_a_sixth_of_the_way_in() {
        assert_eq!(today_scroll(1000.0, 600.0), 900.0);
        assert_eq!(today_scroll(50.0, 600.0), 0.0, "never before the axis");
        let geo = Geo {
            first: -1.0,
            ppd: 22.0,
        };
        assert_eq!(geo.day_under(0.0), -1);
        assert_eq!(geo.day_under(43.9), 0);
        assert_eq!(geo.day_under(44.0), 1);
    }

    #[test]
    fn the_header_has_months_on_top_and_days_or_weeks_below() {
        // Thu 29 Oct 2026 .. into November and December.
        let mut days = Vec::new();
        let mut d = date!(2026 - 10 - 29);
        while days.len() < 30 {
            if d.weekday().number_days_from_monday() < 5 {
                days.push(d);
            }
            d = d.next_day().unwrap();
        }
        let a = axis(&days, DAYS_PPD);
        let months: Vec<_> = a.months.iter().map(|m| m.label.as_str()).collect();
        assert_eq!(
            months,
            ["", "Nov 2026", "Dec"],
            "two days of October have no room for its name; the first name shown says the year"
        );
        let from_monday = axis(&days[2..], DAYS_PPD);
        let months: Vec<_> = from_monday
            .months
            .iter()
            .map(|m| m.label.as_str())
            .collect();
        assert_eq!(months, ["Nov 2026", "Dec"]);
        assert_eq!(
            a.months[1].x,
            2.0 * DAYS_PPD,
            "Nov starts on Mon 2nd, the third day"
        );
        assert_eq!(a.days.len(), 30);
        assert_eq!(a.days[0].label, "29");
        assert_eq!(a.days[0].x, DAYS_PPD / 2.0);
        // Week starts: the first day, then every Monday.
        let weeks: Vec<_> = a.weeks.iter().map(|w| w.label.as_str()).collect();
        assert_eq!(&weeks[..3], ["29", "2", "9"]);
        // Narrow days: no day numbers, and week labels only where they fit.
        let narrow = axis(&days, 2.0);
        assert!(narrow.days.is_empty());
        assert_eq!(narrow.weeks.len(), 7, "the lines are all there");
        assert!(narrow.weeks.iter().any(|w| w.label.is_empty()));
        assert_eq!(
            axis(&[date!(2027 - 01 - 04)], DAYS_PPD).months[0].label,
            "Jan 2027"
        );
    }

    #[test]
    fn a_name_after_a_bar_is_cut_to_the_room_left() {
        assert_eq!(
            bar_name("Design", 100.0, false, 400.0),
            Some((106.0, "Design".to_owned()))
        );
        assert_eq!(
            bar_name("Design", 100.0, true, 400.0).map(|n| n.0),
            Some(134.0),
            "after the +2d chip"
        );
        assert_eq!(
            fit_text("Provision EU network", 60.0),
            Some("Provision…".to_owned())
        );
        assert_eq!(fit_text("Provision EU network", 18.0), None);
        assert_eq!(fit_text("", 100.0), None);
        assert_eq!(bar_name("Design", 390.0, false, 400.0), None);
    }

    #[test]
    fn a_summary_bar_is_filled_for_the_work_done() {
        assert_eq!(progress_w(100.0, 5, 15), 25.0);
        assert_eq!(progress_w(100.0, 0, 0), 0.0);
        assert_eq!(progress_w(100.0, 3, 0), 100.0);
        let mut a = project(10, vec![]);
        a.done_tasks = 2;
        a.open_tasks = 3;
        let mut b = project(11, vec![]);
        b.done_tasks = 1;
        b.open_tasks = 4;
        assert_eq!(group_progress(&group(1, vec![a, b])), (3, 7));
    }

    #[test]
    fn an_ongoing_line_fades_within_a_few_weeks() {
        assert_eq!(fade_span(100.0, 22.0, 2000.0), (100.0, 330.0));
        assert_eq!(fade_span(100.0, 22.0, 200.0), (100.0, 100.0));
        assert_eq!(fade_span(300.0, 22.0, 200.0), (300.0, 0.0));
    }

    #[test]
    fn the_label_column_stays_between_its_limits_and_fits_the_longest_name() {
        assert_eq!(clamp_label_w(50.0), LABEL_MIN);
        assert_eq!(clamp_label_w(9000.0), LABEL_MAX);
        assert_eq!(clamp_label_w(f64::NAN), LABEL_W);
        assert_eq!(counts_w(340.0), COUNTS_W);
        assert_eq!(
            counts_w(220.0),
            0.0,
            "a narrow column keeps its room for names"
        );
        let w = fit_label_w([(0, 10), (2, 20)]);
        assert_eq!(w, 6.0 + 28.0 + 26.0 + 140.0 + COUNTS_W + 8.0);
        assert_eq!(fit_label_w([(0, 500)]), LABEL_MAX);
        assert_eq!(fit_label_w([]), LABEL_MIN);
    }

    #[test]
    fn the_counts_are_short_in_the_column_and_words_in_the_tooltip() {
        let mut p = project(10, vec![]);
        p.open_tasks = 14;
        assert_eq!(count_parts(&p), [Count::Open(14)]);
        p.blocked_tasks = 3;
        p.done_tasks = 5;
        let parts = count_parts(&p);
        let short: Vec<_> = parts.iter().map(|c| c.short()).collect();
        assert_eq!(short, ["14", "3\u{2298}", "5\u{2713}"]);
        assert_eq!(parts[1].words(), "3 blocked");
    }

    #[test]
    fn empty_rows_say_so_and_quick_add_is_filled_in() {
        let d = data(
            vec![
                group(1, vec![]),
                group(
                    2,
                    vec![project(10, vec![]), project(11, vec![task(100, 0.0, 1.0)])],
                ),
            ],
            vec![],
        );
        let list = rows(&d, &HashSet::new(), false);
        let said: Vec<_> = list.iter().map(empty_text).collect();
        assert_eq!(
            said,
            [Some("No projects yet"), None, Some("No open tasks"), None]
        );
        let mut done = project(12, vec![]);
        done.status = ProjectStatus::Done;
        let d = data(vec![group(3, vec![done])], vec![]);
        assert_eq!(
            rows(&d, &HashSet::new(), true)
                .iter()
                .filter_map(empty_text)
                .count(),
            0,
            "a finished project has nothing left to do, and that is fine"
        );
        assert_eq!(
            add_project_text("Launch \"EU\""),
            "project for:\"Launch 'EU'\" "
        );
        assert_eq!(add_task_text("api-launch"), "task #api-launch ");
    }

    #[test]
    fn rows_say_in_words_what_their_bars_show() {
        let mut t = task(100, 2.0, 4.0); // Wed 3 - Thu 4 Mar
        t.schedule.status = TaskStatus::Blocked;
        t.schedule.critical = true;
        t.schedule.late_by_days = Some(2);
        t.due_date = Some(date!(2027 - 03 - 04));
        let mut p = project(10, vec![t]);
        p.open_tasks = 1;
        p.blocked_tasks = 1;
        p.band = Band {
            work_from: Some(2.0),
            work_to: Some(4.0),
            target_date: Some(date!(2027 - 03 - 10)),
            target_offset: Some(8.0),
            ..Band::default()
        };
        p.zone = Zone::Inside {
            remaining: 2,
            room: 4,
            level: ZoneLevel::Wide,
        };
        let d = data(vec![group(1, vec![p]), group(2, vec![])], vec![]);
        let list = rows(&d, &all_open(&d), false);
        let said: Vec<String> = list.iter().map(|r| row_label(&d, r)).collect();
        assert_eq!(said[0], "O1, objective, 1 project");
        assert_eq!(
            said[1],
            "P10, project, 1 open task, 1 blocked, finishes Thu 4 Mar, target Wed 10 Mar, 4 working days of room"
        );
        assert_eq!(
            said[2],
            "T100, task, blocked, critical, Wed 3 Mar to Thu 4 Mar, due Thu 4 Mar, 2 working days late"
        );
        assert_eq!(
            said[3],
            "Add a task to P10: drag across the chart for its dates, or use its button"
        );
        assert_eq!(said[4], "O2, objective, no projects yet");
        assert_eq!(fold_text(&list[1], true, false), "P10 opened: 1 task");
        assert_eq!(fold_text(&list[0], false, false), "O1 folded");
    }

    #[test]
    fn the_table_has_a_row_of_cells_for_every_row() {
        let mut t = task(100, 2.0, 4.0);
        t.schedule.late_by_days = Some(3);
        t.schedule.critical = true;
        let mut child = task(101, 0.0, 1.0);
        child.parent = Some(id(100));
        child.overdue = true;
        let d = data(vec![group(1, vec![project(10, vec![t, child])])], vec![]);
        let list = rows(&d, &all_open(&d), false);
        let c = cells(&d, &list[2]);
        assert_eq!(
            (c.kind, c.status.as_str(), c.start, c.finish),
            (
                "Task",
                "To do",
                Some(date!(2027 - 03 - 03)),
                Some(date!(2027 - 03 - 04))
            )
        );
        assert_eq!(c.room, Some(("late 3d".to_owned(), true)));
        assert!(c.critical);
        let c = cells(&d, &list[3]);
        assert_eq!(
            (c.kind, c.room),
            ("Sub-task", Some(("overdue".to_owned(), true)))
        );
        assert_eq!(cells(&d, &list[1]).kind, "Project");
        assert_eq!(cells(&d, &list[0]).status, "On track");
    }

    #[test]
    fn the_table_sorts_every_task_flat() {
        let mut a = task(100, 4.0, 5.0);
        a.schedule.title = "beta".into();
        a.due_date = Some(date!(2027 - 03 - 10));
        let mut b = task(101, 0.0, 1.0);
        b.schedule.title = "Alpha".into();
        let mut c = task(102, 2.0, 3.0);
        c.schedule.title = "gamma".into();
        c.due_date = Some(date!(2027 - 03 - 05));
        let mut done = task(103, 0.0, 1.0);
        done.schedule.done = true;
        let d = data(
            vec![group(
                1,
                vec![project(10, vec![a, b]), project(11, vec![c, done])],
            )],
            vec![],
        );
        let names = |key, desc| {
            sorted_tasks(&d, false, key, desc)
                .iter()
                .map(|t| t.schedule.title.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(SortKey::Name, false), ["Alpha", "beta", "gamma"]);
        assert_eq!(names(SortKey::Start, true), ["beta", "gamma", "Alpha"]);
        assert_eq!(
            names(SortKey::Due, false),
            ["gamma", "beta", "Alpha"],
            "no due date last"
        );
        assert_eq!(
            names(SortKey::Due, true),
            ["beta", "gamma", "Alpha"],
            "still last, reversed"
        );
        assert_eq!(sorted_tasks(&d, true, SortKey::Name, false).len(), 4);
    }

    #[test]
    fn press_areas_are_big_enough_and_never_cover_each_other() {
        for wd in [4.0, 22.0, 44.0, 66.0, 300.0] {
            let h = task_hits(100.0, wd);
            assert_eq!(h.start.1, HIT, "{wd}");
            assert_eq!(h.end.1, HIT, "{wd}");
            assert!(h.start.0 + h.start.1 <= h.body.0 + 1e-9, "{wd}");
            assert!(h.body.0 + h.body.1 <= h.end.0 + 1e-9, "{wd}");
            assert!(
                h.body.1 >= wd / 2.0 - 1e-9,
                "the middle keeps half the bar: {wd}"
            );
            assert!(
                h.start.0 + h.start.1 > 100.0,
                "the start reaches into the bar"
            );
        }
        assert_eq!(body_hit(100.0, 4.0), (90.0, HIT));
        assert_eq!(body_hit(100.0, 40.0), (100.0, 40.0));
        let (s, t) = band_handles(Some(100.0), Some(300.0));
        assert_eq!((s, t), (Some((88.0, HIT)), Some((288.0, HIT))));
        let (s, t) = band_handles(Some(100.0), Some(110.0));
        let (s, t) = (s.unwrap(), t.unwrap());
        assert!(
            s.0 + s.1 <= t.0 + 1e-9,
            "close marks split the space between them"
        );
        assert_eq!(s.0 + s.1, 105.0);
        assert_eq!(band_handles(None, Some(50.0)), (None, Some((38.0, HIT))));
    }

    #[test]
    fn the_keyboard_moves_band_dates_or_says_why_not() {
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
        assert_eq!(
            band_step(&d, &b, BandPart::Target, 1, "API", false),
            Ok((None, Some(date!(2027 - 03 - 08))))
        );
        assert_eq!(
            band_step(&d, &b, BandPart::Start, -1, "API", false),
            Ok((Some(date!(2027 - 02 - 26)), None))
        );
        assert_eq!(
            band_step(&d, &b, BandPart::Target, 1, "Keep healthy", true),
            Err("Keep healthy is ongoing: it has no target to move.".to_owned())
        );
        let no_start = Band {
            start_date: None,
            start_offset: None,
            ..b
        };
        assert!(band_step(&d, &no_start, BandPart::Start, 1, "API", false)
            .unwrap_err()
            .contains("no start date"));
        let tight = Band {
            target_date: Some(date!(2027 - 03 - 01)),
            target_offset: Some(1.0),
            ..b
        };
        assert_eq!(
            band_step(&d, &tight, BandPart::Start, 1, "API", false),
            Err("API: the start can't pass the target.".to_owned())
        );
    }

    #[test]
    fn a_task_that_cannot_move_says_why() {
        let first = task(1, 0.0, 2.0);
        let mut second = task(2, 2.0, 3.0);
        second.schedule.held_by = Some(id(1));
        second.schedule.earliest_start = date!(2027 - 03 - 03);
        let d = data(
            vec![group(1, vec![project(10, vec![first, second.clone()])])],
            vec![],
        );
        let c = task_candidate(&d, &second, DragEdge::Move, -1);
        assert_eq!(
            task_step_refusal(&d, &second, &c),
            Some("T2 can't start before T1 is done (Wed 3 Mar).".to_owned())
        );
        let c = task_candidate(&d, &second, DragEdge::Move, 1);
        assert_eq!(task_step_refusal(&d, &second, &c), None);
        second.schedule.done = true;
        assert!(task_step_refusal(&d, &second, &c)
            .unwrap()
            .contains("reopen"));
        assert_eq!(
            working_day_on_or_after(&d, date!(2027 - 03 - 06)),
            Some(5),
            "a Saturday moves to Monday"
        );
        assert_eq!(working_day_on_or_after(&d, date!(2030 - 01 - 01)), None);
    }

    #[test]
    fn density_is_remembered_by_name() {
        assert_eq!(Density::from_id("comfortable"), Density::Comfortable);
        assert_eq!(Density::from_id("anything"), Density::Compact);
        assert_eq!(Density::Compact.dims().row_h, ROW_H);
        let roomy = Density::Comfortable.dims();
        assert!(roomy.row_h >= 36.0 && roomy.bar_h >= 16.0);
        assert_eq!(
            Density::from_id(Density::Comfortable.id()),
            Density::Comfortable
        );
    }

    /// Spec 58: every mark that means something on the chart and every word on it stands out
    /// against the chart in every built-in theme (3:1 for marks, 4.5:1 for text), read from the
    /// real style sheet, with no transparency to water it down. Grid lines are left out on purpose:
    /// they only decorate, the dates are in the header (WCAG 1.4.11).
    #[test]
    fn the_chart_marks_and_words_stand_out_in_every_theme() {
        let css = include_str!("../style/input.css");
        let marks = [
            ".gc .gb.todo",
            ".gc .gb.progress",
            ".gc .gb.blocked",
            ".gc .gb.focus, .gc .summary.focus",
            ".gc .crit",
            ".gc .late-tail",
            ".gc .slack",
            ".gc .due",
            ".gc .due.overdue",
            ".gc .summary",
            ".gc .progress",
            ".gc .room",
            ".gc .room.narrow",
            ".gc .room.closing",
            ".gc .hatch-line",
            ".gc .dot",
            ".gc .over",
            ".gc .flag",
            ".gc .today",
            ".gc .today-marker",
            ".gc .row-focus",
            ".gc .new-bar",
            ".gc .overload",
            ".gc .over-line",
            ".gc .strip-work",
            ".gc .strip-work.look",
            ".gc .strip-target",
            ".gc .strip-box",
        ];
        let words = [
            ".gc text",
            ".gc text.name",
            ".gc text.band-name",
            ".gc text.month",
            ".gc text.today-num",
            ".gc text.empty",
            ".gc .pushed",
        ];
        let rule = |sel: &str| -> &str {
            let head = format!("\n{sel} {{");
            let at = css
                .find(&head)
                .unwrap_or_else(|| panic!("no rule for {sel}"));
            let body = &css[at + head.len()..];
            &body[..body.find('}').unwrap()]
        };
        for (list, least) in [(&marks[..], 3.0), (&words[..], 4.5)] {
            for sel in list {
                let r = rule(sel);
                assert!(!r.contains("opacity: 0"), "{sel} is see-through: {r}");
                let used: Vec<&str> = r
                    .split("var(--")
                    .skip(1)
                    .filter_map(|v| v.split(')').next())
                    // The background a mark sits on, not the mark.
                    .filter(|v| *v != "canvas" && *v != "panel")
                    .collect();
                assert!(!used.is_empty(), "{sel} names no colour token: {r}");
                for theme in crate::themes::ALL {
                    let vars = theme.tokens.css_vars();
                    let canvas = theme.tokens.canvas;
                    for name in &used {
                        let (_, value) = vars
                            .iter()
                            .find(|(n, _)| n.trim_start_matches("--") == *name)
                            .unwrap_or_else(|| panic!("unknown token --{name} in {sel}"));
                        let c = crate::themes::contrast(value, canvas);
                        assert!(
                            c >= least,
                            "{sel} uses --{name}: {c:.2}:1 on {}, wants {least}:1",
                            theme.id
                        );
                    }
                }
            }
        }
    }

    fn person_ref(n: u128, name: &str) -> GanttRef {
        GanttRef {
            id: id(n),
            title: name.into(),
        }
    }

    #[test]
    fn projects_that_need_a_look_start_open_and_the_rest_folded() {
        let mut late = project(10, vec![task(100, 0.0, 2.0)]);
        late.needs_a_look = true;
        let calm = project(11, vec![task(101, 0.0, 1.0)]);
        let d = data(vec![group(1, vec![late, calm])], vec![]);
        let none = HashSet::new();
        let keys: Vec<_> = rows(&d, &none, false).iter().map(|r| r.key).collect();
        assert_eq!(
            keys,
            [id(1), id(10), id(100), add_key(id(10)), id(11)],
            "the late project is open, with its add row; the calm one is folded"
        );
        // Clicks are kept as what differs: folding the late one toggles it.
        let folded = with_all_projects(&d, &none, false);
        assert!(folded.contains(&id(10)) && !folded.contains(&id(11)));
        assert_eq!(tasks_shown(&d, &folded), TasksShown::None);
        assert_eq!(
            tasks_shown(&d, &with_all_projects(&d, &none, true)),
            TasksShown::All
        );
        assert_eq!(tasks_shown(&d, &all_open(&d)), TasksShown::All);
    }

    #[test]
    fn rows_by_person_put_a_task_under_its_first_assignee_and_the_rest_under_nobody() {
        let mut a = task(100, 2.0, 3.0);
        a.assignees = vec![id(50), id(51)];
        let mut b = task(101, 0.0, 1.0);
        b.assignees = vec![id(51)];
        let c = task(102, 1.0, 2.0);
        let mut d = data(vec![group(1, vec![project(10, vec![a, b, c])])], vec![]);
        // Amir (51) comes before Zoe (50) by name.
        d.people = vec![person_ref(51, "Amir"), person_ref(50, "Zoe")];
        let list = rows_by_person(&d, &HashSet::new(), false);
        let keys: Vec<_> = list.iter().map(|r| (r.key, r.depth)).collect();
        assert_eq!(
            keys,
            [
                (id(51), 0),
                (id(101), 1),
                (id(100), 1),
                (id(50), 0),
                (UNASSIGNED.id, 0),
                (id(102), 1)
            ]
        );
        assert_eq!(row_label(&d, &list[0]), "Amir, person, 2 open tasks");
        assert!(tree(&d, &HashSet::new(), false, false)
            .iter()
            .all(|r| !matches!(r.kind, RowKind::Person(_))));
    }

    #[test]
    fn filters_narrow_the_plan_and_recount() {
        let mut a = task(100, 0.0, 2.0);
        a.assignees = vec![id(50)];
        let mut b = task(101, 0.0, 1.0);
        b.schedule.status = TaskStatus::Blocked;
        let mut child = task(102, 0.0, 1.0);
        child.parent = Some(id(101));
        child.assignees = vec![id(50)];
        let mut late = project(10, vec![a, b, child]);
        late.needs_a_look = true;
        let calm = project(11, vec![task(103, 0.0, 1.0)]);
        let d = data(
            vec![group(1, vec![late]), group(2, vec![calm])],
            vec![blocks(100, 101), blocks(101, 103)],
        );
        let mine = filtered(
            &d,
            &ViewFilter {
                person: Some(id(50)),
                ..Default::default()
            },
        );
        assert_eq!(
            mine.groups.len(),
            1,
            "the calm objective has nothing of theirs"
        );
        let p = &mine.groups[0].projects[0];
        assert_eq!(p.tasks.len(), 2);
        assert_eq!((p.open_tasks, p.blocked_tasks), (2, 0));
        assert_eq!(
            p.tasks[1].parent, None,
            "its parent is not theirs, so it stands alone"
        );
        assert!(mine.links.is_empty(), "no link has both ends left");
        let look = filtered(
            &d,
            &ViewFilter {
                needs_a_look: true,
                ..Default::default()
            },
        );
        assert_eq!(look.groups.len(), 1);
        let one = filtered(
            &d,
            &ViewFilter {
                objective: Some(id(2)),
                ..Default::default()
            },
        );
        assert_eq!(one.groups.len(), 1);
        assert_eq!(one.groups[0].projects[0].id, id(11));
        assert_eq!(filtered(&d, &ViewFilter::default()), d);
    }

    #[test]
    fn find_lists_matches_in_plan_order_and_unfolds_what_hides_them() {
        let mut t = task(100, 0.0, 2.0);
        t.schedule.title = "Load test the EU stack".into();
        let mut p = project(10, vec![t, task(101, 0.0, 1.0)]);
        p.title = "EU Region".into();
        let d = data(vec![group(1, vec![p])], vec![]);
        assert_eq!(find_matches(&d, "eu", false), [id(10), id(100)]);
        assert_eq!(find_matches(&d, "load eu", false), [id(100)]);
        assert!(find_matches(&d, "  ", false).is_empty());
        // The objective was folded and the project is folded: both open for the task.
        let folded = HashSet::from([id(1)]);
        let open = unfold_for(&d, &folded, &[id(100)]);
        assert!(is_open(true, id(1), &open) && is_open(false, id(10), &open));
        // A project match opens only its objective.
        let open = unfold_for(&d, &folded, &[id(10)]);
        assert!(is_open(true, id(1), &open) && !is_open(false, id(10), &open));
    }

    #[test]
    fn zoom_steps_between_weeks_and_days_and_keeps_the_day_under_the_pointer() {
        assert_eq!(zoom(DAYS_PPD, 1), Scale::Days, "Days is the closest");
        assert_eq!(zoom(DAYS_PPD, -1), Scale::Step(3));
        assert_eq!(zoom(WEEKS_PPD, -1), Scale::Weeks);
        assert_eq!(zoom(WEEKS_PPD, 1), Scale::Step(1));
        assert_eq!(
            zoom(3.0, 1),
            Scale::Weeks,
            "from a fitted scale, the next step up"
        );
        assert_eq!(Scale::Step(2).ppd(10, 100.0), 15.0);
        assert_eq!(scale_at(18.0), Scale::Step(3));
        // The day under x = 100 of the view stays there.
        let left = zoom_scroll(100.0, 400.0, 8.0, 16.0);
        assert_eq!((left + 100.0) / 16.0, (400.0 + 100.0) / 8.0);
        assert_eq!(zoom_scroll(100.0, 0.0, 22.0, 8.0), 0.0);
    }

    #[test]
    fn the_overview_strip_maps_the_axis_and_the_view() {
        let d = data(vec![], vec![]); // 40 days from offset -1
        assert_eq!(strip_x(&d, -1.0, 400.0), 0.0);
        assert_eq!(strip_x(&d, 19.0, 400.0), 200.0);
        assert_eq!(strip_x(&d, 999.0, 400.0), 400.0);
        assert_eq!(strip_box(440.0, 220.0, 880.0, 400.0), (200.0, 100.0));
        assert_eq!(
            strip_box(860.0, 220.0, 880.0, 400.0),
            (300.0, 100.0),
            "kept on the strip"
        );
        assert_eq!(strip_scroll(200.0, 400.0, 880.0, 220.0), 330.0);
        assert_eq!(strip_scroll(0.0, 400.0, 880.0, 220.0), 0.0);
        assert_eq!(strip_scroll(400.0, 400.0, 880.0, 220.0), 660.0);
    }

    #[test]
    fn a_task_drawn_on_the_add_row_becomes_a_quick_add_line() {
        let d = data(vec![], vec![]);
        // Wed 3 to Mon 8 Mar: four working days, either way round.
        assert_eq!(
            add_task_line(&d, "api", 2, 5),
            "task #api start:2027-03-03 due:2027-03-08 est:4d "
        );
        assert_eq!(
            add_task_line(&d, "api", 5, 2),
            add_task_line(&d, "api", 2, 5)
        );
        assert_eq!(
            add_task_line(&d, "api", 0, 0),
            "task #api start:2027-03-01 due:2027-03-01 est:1d "
        );
    }

    #[test]
    fn over_capacity_weeks_become_spans_on_the_axis() {
        use minimap_types::{NodeRef, NodeSummary, NodeType, PersonCapacity, WeekLoad};
        let d = data(vec![], vec![]);
        let week = |start: Date, over: bool| WeekLoad {
            week_start: start,
            load_days: 6.0,
            capacity_days: 5.0,
            load_pct: 120.0,
            over,
            tasks: Vec::new(),
        };
        let cap = Capacity {
            today: d.today,
            current_week_start: d.today,
            weeks: Vec::new(),
            from: d.today,
            to: d.today,
            prev_from: d.today,
            next_from: d.today,
            people: vec![PersonCapacity {
                person: NodeSummary {
                    node: NodeRef::new(NodeType::Person, id(50)),
                    label: "Zoe".into(),
                    archived: false,
                },
                weekly_capacity_hours: 40.0,
                weeks: vec![
                    week(date!(2027 - 03 - 01), true),
                    week(date!(2027 - 03 - 08), false),
                    week(date!(2030 - 01 - 07), true),
                ],
                peak_pct: 120.0,
                peak_week: None,
                over_weeks: 2,
                active_tasks: 3,
                over_task_limit: false,
                unestimated_tasks: 0,
                on_my_projects: false,
            }],
            task_limit: 10,
            hours_per_day: 8.0,
            warnings: Vec::new(),
            mine: false,
        };
        let spans = over_weeks(&d, &cap);
        assert_eq!(
            spans[&id(50)],
            [(0, 5, 120.0)],
            "Mon 1 to Fri 5; off the axis left out"
        );
    }
}
