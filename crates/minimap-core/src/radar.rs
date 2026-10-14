//! The radar's layout (docs/product-design/10-radar-window.md, ADR-0021). Pure.
//!
//! Core decides **what** goes **where** in abstract polar units; the UI turns that into pixels.
//! The centre is you; the distance from it is how soon something is due (in working days, so
//! the rings are the same ones the schedule counts); the direction is the objective it serves.
//! Nothing here is random: a blip's angle comes from a hash of its id, so it does not jump
//! between refreshes, and the result does not depend on the order the inputs arrive in.

use std::{
    collections::{HashMap, HashSet},
    f64::consts::TAU,
};

use minimap_types::{
    fmt_clock, validate_radar_rings, ApproachReport, Blip, BlipKind, BlipSize, EdgeMark, FogLevel,
    FogReport, Footer, HealthLevel, MyScope, NodeRef, NodeSummary, NodeType, Objective,
    ObjectiveStatus, PingsView, PortfolioOverview, Project, ProjectStatus, Radar, RadarMark,
    Reason, Sector, Severity, Task, TaskStatus, Uuid, View, WorkWeek, Zone, ZoneLevel,
    DEFAULT_RADAR_RINGS,
};
use time::{Date, Month, Weekday};

use crate::{health::task_health, schedule::working_index, sync::fnv1a};

/// The most blips the radar draws; beyond this the most urgent stay (`Footer::hidden_blips`).
pub const MAX_BLIPS: usize = 60;
/// Blips closer than this (in unit coordinates, where the radar's radius is 1) are nudged apart.
pub const MIN_GAP: f64 = 0.05;
/// No sector is narrower than this (20 degrees)...
pub const MIN_SWEEP: f64 = 20.0 * TAU / 360.0;
/// ...and "Other" is never wider than this (90 degrees).
pub const OTHER_MAX_SWEEP: f64 = 90.0 * TAU / 360.0;
/// A blip stays this far (4 degrees) inside the edge of its sector.
const PADDING: f64 = 4.0 * TAU / 360.0;
/// Edge marks on the same rim keep at least this far apart (radians).
const MARK_GAP: f64 = 0.06;

/// Where each stretch of time ends on the radar (0 = the centre, 1 = the outer edge). Overdue
/// work sits inside the first stretch.
const R_NOW: f64 = 0.12;
const R_RING0: f64 = 0.4;
const R_RING1: f64 = 0.65;
const R_RING2: f64 = 0.95;
/// More overdue than this many days makes no further difference to where a blip sits.
const OVERDUE_CAP_DAYS: i64 = 10;
/// Blocked work with no date, and work in focus with no date.
const R_BLOCKED_NO_DATE: f64 = 0.5;
const R_FOCUS_NO_DATE: f64 = 0.3;

/// How many passes the collision nudge makes.
const NUDGE_PASSES: usize = 12;

/// Everything the layout needs, already read from the database.
pub struct RadarInput<'a> {
    pub today: Date,
    pub work_week: WorkWeek,
    /// The view in effect: `Mine` only when there is a scope to look through.
    pub view: View,
    pub scope: Option<&'a MyScope>,
    /// Active (not archived) items.
    pub tasks: &'a [Task],
    pub projects: &'a [Project],
    pub objectives: &'a [Objective],
    /// `contributes_to` links: (a task or project, the objective it serves).
    pub contributes: &'a [(Uuid, Uuid)],
    /// Project health and safe zone, from the Overview.
    pub overview: &'a PortfolioOverview,
    pub fog: &'a FogReport,
    pub pings: &'a PingsView,
    /// What approaches your work (scope Me).
    pub approaching: &'a ApproachReport,
    /// The rings, as working days from today.
    pub rings: [u32; 3],
}

// ---------------------------------------------------------------- radius

/// How far from the centre something due in `days` working days sits (negative = overdue).
/// Piecewise linear: overdue is inside the first stretch, closer the later it is.
pub fn radius(days: i64, rings: [u32; 3]) -> f64 {
    let [a, b, c] = rings.map(i64::from);
    if days < 0 {
        let late = (-days).min(OVERDUE_CAP_DAYS) as f64;
        return R_NOW * (1.0 - late / OVERDUE_CAP_DAYS as f64);
    }
    let between = |from: i64, to: i64, r0: f64, r1: f64| {
        r0 + (r1 - r0) * ((days - from) as f64 / (to - from).max(1) as f64)
    };
    if days <= a {
        between(0, a, R_NOW, R_RING0)
    } else if days <= b {
        between(a, b, R_RING0, R_RING1)
    } else if days <= c {
        between(b, c, R_RING1, R_RING2)
    } else {
        R_RING2
    }
}

/// Working days from today until `date`; at most -1 once the date has passed (a due date on a
/// day off still counts as overdue).
fn days_until(week: WorkWeek, today: Date, date: Date) -> i64 {
    let days = working_index(week, date) - working_index(week, today);
    if date < today {
        days.min(-1)
    } else {
        days
    }
}

// ---------------------------------------------------------------- wording

fn plural(n: i64, word: &str) -> String {
    format!("{n} {word}{}", if n == 1 { "" } else { "s" })
}

fn weekday_name(d: Weekday) -> &'static str {
    match d {
        Weekday::Monday => "Mon",
        Weekday::Tuesday => "Tue",
        Weekday::Wednesday => "Wed",
        Weekday::Thursday => "Thu",
        Weekday::Friday => "Fri",
        Weekday::Saturday => "Sat",
        Weekday::Sunday => "Sun",
    }
}

fn month_name(m: Month) -> &'static str {
    match m {
        Month::January => "Jan",
        Month::February => "Feb",
        Month::March => "Mar",
        Month::April => "Apr",
        Month::May => "May",
        Month::June => "Jun",
        Month::July => "Jul",
        Month::August => "Aug",
        Month::September => "Sep",
        Month::October => "Oct",
        Month::November => "Nov",
        Month::December => "Dec",
    }
}

/// "today", "tomorrow", "Fri" (within the coming week), else "12 Mar" (with the year when it is
/// not this one).
fn when_text(date: Date, today: Date) -> String {
    match (date - today).whole_days() {
        0 => "today".to_owned(),
        1 => "tomorrow".to_owned(),
        2..=6 => weekday_name(date.weekday()).to_owned(),
        _ if date.year() == today.year() => {
            format!("{} {}", date.day(), month_name(date.month()))
        }
        _ => format!(
            "{} {} {}",
            date.day(),
            month_name(date.month()),
            date.year()
        ),
    }
}

fn join(parts: Vec<String>) -> String {
    parts.join(" · ")
}

fn reason_word(r: Reason) -> &'static str {
    match r {
        Reason::Overdue => "overdue",
        Reason::Blocked => "blocked",
        Reason::Late | Reason::ProjectLate => "running late",
        Reason::NoSlack => "out of slack",
        Reason::ProjectRed => "in trouble",
    }
}

fn size_of(priority: u8) -> BlipSize {
    match priority {
        1 => BlipSize::L,
        2 => BlipSize::M,
        _ => BlipSize::S,
    }
}

fn mark_of(level: HealthLevel) -> RadarMark {
    match level {
        HealthLevel::Red => RadarMark::Red,
        HealthLevel::Amber => RadarMark::Amber,
        HealthLevel::Green => RadarMark::Green,
        HealthLevel::Idle => RadarMark::None,
    }
}

// ---------------------------------------------------------------- sectors

/// How wide each sector is. `counts` are the blips in each objective sector, `other` the blips
/// in the "Other" sector (`None`: there is no such sector). Sweeps add up to a full turn, none is
/// below [`MIN_SWEEP`] and "Other" is at most [`OTHER_MAX_SWEEP`] (unless that cannot be
/// honoured, which is when there are too many sectors for the minimum: then they share equally).
fn sweeps(counts: &[usize], other: Option<usize>) -> Vec<f64> {
    let total = counts.len() + usize::from(other.is_some());
    if total == 0 {
        return Vec::new();
    }
    let weight = |n: usize| n.max(1) as f64;
    if total == 1 {
        return vec![TAU];
    }
    if total as f64 * MIN_SWEEP >= TAU {
        return vec![TAU / total as f64; total];
    }
    // "Other" first: its share, held between the minimum and its cap, and low enough to leave
    // every objective its minimum.
    let all: f64 = counts.iter().map(|n| weight(*n)).sum::<f64>() + other.map_or(0.0, weight);
    let other_sweep = other.map(|n| {
        let room = TAU - counts.len() as f64 * MIN_SWEEP;
        (TAU * weight(n) / all).clamp(MIN_SWEEP, OTHER_MAX_SWEEP.min(room))
    });
    let available = TAU - other_sweep.unwrap_or(0.0);
    // The objectives share the rest by weight; one that would get less than the minimum is held
    // at it and the others share what is left.
    let mut fixed: Vec<Option<f64>> = vec![None; counts.len()];
    loop {
        let held: f64 = fixed.iter().flatten().sum();
        let free_weight: f64 = counts
            .iter()
            .zip(&fixed)
            .filter(|(_, f)| f.is_none())
            .map(|(n, _)| weight(*n))
            .sum();
        if free_weight == 0.0 {
            break;
        }
        let share = |n: usize| (available - held) * weight(n) / free_weight;
        let below: Vec<usize> = (0..counts.len())
            .filter(|&i| fixed[i].is_none() && share(counts[i]) < MIN_SWEEP)
            .collect();
        if below.is_empty() {
            for i in 0..counts.len() {
                if fixed[i].is_none() {
                    fixed[i] = Some(share(counts[i]));
                }
            }
            break;
        }
        for i in below {
            fixed[i] = Some(MIN_SWEEP);
        }
    }
    let mut out: Vec<f64> = fixed.into_iter().map(|f| f.unwrap_or(MIN_SWEEP)).collect();
    out.extend(other_sweep);
    out
}

/// The room a blip may take inside a sector: the sector less the padding on each side.
fn bounds(sector: &Sector) -> (f64, f64) {
    let pad = PADDING.min(sector.sweep / 4.0);
    (sector.start + pad, sector.start + sector.sweep - pad)
}

/// A repeatable place inside the sector for `key`: the same id always lands in the same spot.
fn angle_in(sector: &Sector, key: Uuid) -> f64 {
    let (lo, hi) = bounds(sector);
    let unit = (fnv1a(key.as_bytes()) % 10_000) as f64 / 9_999.0;
    lo + unit * (hi - lo)
}

// ---------------------------------------------------------------- the build

/// One thing that may become a blip, before it is placed.
struct Candidate {
    node: NodeSummary,
    kind: BlipKind,
    r: f64,
    size: BlipSize,
    mark: RadarMark,
    fog: FogLevel,
    /// The objective whose sector it sits in; `None` = "Other".
    objective: Option<Uuid>,
    label: String,
    /// Past its due date or target (tasks and projects only): the first thing kept when there are
    /// too many.
    overdue: bool,
    date: Option<Date>,
    priority: u8,
}

impl Candidate {
    /// Most urgent first: overdue, then the date, then priority; the id settles ties.
    fn urgency(&self) -> (u8, Date, u8, Uuid) {
        (
            u8::from(!self.overdue),
            self.date.unwrap_or(Date::MAX),
            self.priority,
            self.node.node.id,
        )
    }
}

struct Placed {
    cand: Candidate,
    /// Index into the sector list.
    sector: usize,
    theta: f64,
}

struct Ctx<'a> {
    input: &'a RadarInput<'a>,
    rings: [u32; 3],
    /// Sector objectives in order, and where each sits in the list.
    objective_index: HashMap<Uuid, usize>,
    contributes: HashMap<Uuid, Vec<Uuid>>,
    project_titles: HashMap<Uuid, &'a str>,
    fog: HashMap<Uuid, (FogLevel, u32)>,
    pinged: HashSet<Uuid>,
}

impl Ctx<'_> {
    fn in_scope(&self, node: NodeRef) -> bool {
        match (self.input.view, self.input.scope) {
            (View::Mine, Some(scope)) => match node.node_type {
                NodeType::Task => scope.tasks.contains_key(&node.id),
                NodeType::Project => scope.projects.contains_key(&node.id),
                NodeType::Objective => scope.objectives.contains_key(&node.id),
                // A ping on a note or a person is yours whatever the view.
                _ => true,
            },
            _ => true,
        }
    }

    /// The highest-priority objective (the first sector) among these that has a sector.
    fn pick_objective(&self, ids: &[Uuid]) -> Option<Uuid> {
        ids.iter()
            .filter_map(|id| self.objective_index.get(id).map(|i| (*i, *id)))
            .min()
            .map(|(_, id)| id)
    }

    fn objective_of(&self, id: Uuid) -> Option<Uuid> {
        self.contributes
            .get(&id)
            .and_then(|list| self.pick_objective(list))
    }

    /// A task serves what it contributes to itself, else what its project does.
    fn objective_of_task(&self, task: &Task) -> Option<Uuid> {
        self.objective_of(task.id)
            .or_else(|| task.project_id.and_then(|p| self.objective_of(p)))
    }

    /// Where an arbitrary node sits: tasks and projects by their links, an objective in its own
    /// sector, anything else (a note, a person) in "Other".
    fn objective_of_node(&self, node: NodeRef, tasks: &HashMap<Uuid, &Task>) -> Option<Uuid> {
        match node.node_type {
            NodeType::Task => tasks
                .get(&node.id)
                .and_then(|t| self.objective_of_task(t))
                .or_else(|| self.objective_of(node.id)),
            NodeType::Project => self.objective_of(node.id),
            NodeType::Objective => self
                .objective_index
                .contains_key(&node.id)
                .then_some(node.id),
            _ => None,
        }
    }

    fn fog_of(&self, id: Uuid) -> (FogLevel, Option<u32>) {
        self.fog
            .get(&id)
            .map_or((FogLevel::Clear, None), |(l, age)| (*l, Some(*age)))
    }

    fn task_candidate(&self, t: &Task) -> Option<Candidate> {
        let input = self.input;
        let open = matches!(
            t.status,
            TaskStatus::Todo | TaskStatus::InProgress | TaskStatus::Blocked
        );
        if !open || t.is_meeting() || !self.in_scope(NodeRef::new(NodeType::Task, t.id)) {
            return None;
        }
        let blocked = t.status == TaskStatus::Blocked;
        let overdue = t.due_date.is_some_and(|d| d < input.today);
        let focus = t.focus.is_some_and(|f| f.is_active(input.today));
        let days = t
            .due_date
            .map(|d| days_until(input.work_week, input.today, d));
        let near = days.is_some_and(|d| d <= i64::from(self.rings[2]));
        if !(overdue || near || blocked || focus) {
            return None;
        }
        let r = match (days, blocked, focus) {
            (Some(d), _, _) => radius(d, self.rings),
            (None, true, _) => R_BLOCKED_NO_DATE,
            (None, false, _) => R_FOCUS_NO_DATE,
        };
        let (fog, age) = self.fog_of(t.id);
        let mut parts = vec![t.title.clone()];
        parts.extend(
            t.project_id
                .and_then(|p| self.project_titles.get(&p))
                .map(|s| (*s).to_owned()),
        );
        if blocked {
            parts.push("blocked".into());
        }
        if let Some(due) = t.due_date {
            parts.push(if overdue {
                format!(
                    "overdue {}",
                    plural((input.today - due).whole_days(), "day")
                )
            } else {
                format!("due {}", when_text(due, input.today))
            });
        }
        if focus {
            parts.push("in focus".into());
        }
        if let Some(age) = age {
            parts.push(format!("quiet for {}", plural(i64::from(age), "day")));
        }
        if self.pinged.contains(&t.id) {
            parts.push("pinged".into());
        }
        let health = task_health(t.due_date, overdue, blocked);
        Some(Candidate {
            node: NodeSummary {
                node: NodeRef::new(NodeType::Task, t.id),
                label: t.title.clone(),
                archived: false,
            },
            kind: BlipKind::Task,
            r,
            size: size_of(t.priority),
            mark: mark_of(health.level),
            fog,
            objective: self.objective_of_task(t),
            label: join(parts),
            overdue,
            date: t.due_date,
            priority: t.priority,
        })
    }

    /// A meeting today or tomorrow.
    fn meeting_candidate(&self, t: &Task) -> Option<Candidate> {
        let today = self.input.today;
        let open = matches!(t.status, TaskStatus::Todo | TaskStatus::InProgress);
        let day = t.due_date?;
        let ahead = (day - today).whole_days();
        if !t.is_meeting()
            || !open
            || !(0..=1).contains(&ahead)
            || !self.in_scope(NodeRef::new(NodeType::Task, t.id))
        {
            return None;
        }
        let when = match t.start_minute {
            Some(m) => format!("{} {}", when_text(day, today), fmt_clock(m)),
            None => when_text(day, today),
        };
        let mut parts = vec![t.title.clone(), "meeting".into(), when];
        if self.pinged.contains(&t.id) {
            parts.push("pinged".into());
        }
        Some(Candidate {
            node: NodeSummary {
                node: NodeRef::new(NodeType::Task, t.id),
                label: t.title.clone(),
                archived: false,
            },
            kind: BlipKind::Meeting,
            r: radius(ahead, self.rings),
            size: BlipSize::S,
            mark: RadarMark::None,
            fog: FogLevel::Clear,
            objective: self.objective_of_task(t),
            label: join(parts),
            overdue: false,
            date: Some(day),
            priority: t.priority,
        })
    }

    fn project_candidate(&self, p: &Project) -> Option<Candidate> {
        let input = self.input;
        if p.status != ProjectStatus::Active
            || !self.in_scope(NodeRef::new(NodeType::Project, p.id))
        {
            return None;
        }
        let row = input.overview.project_health(p.id);
        let zone = row.map_or(Zone::None, |r| r.zone);
        let tight = matches!(
            zone,
            Zone::Outside { .. }
                | Zone::Inside {
                    level: ZoneLevel::Narrow | ZoneLevel::Closing,
                    ..
                }
        );
        let days = p
            .target_date
            .map(|d| days_until(input.work_week, input.today, d));
        let near = days.is_some_and(|d| d <= i64::from(self.rings[2]));
        if !(near || tight) {
            return None;
        }
        let overdue = p.target_date.is_some_and(|d| d < input.today);
        let (fog, age) = self.fog_of(p.id);
        let mut parts = vec![p.title.clone(), "project".into()];
        if let Some(target) = p.target_date {
            parts.push(if overdue {
                format!(
                    "target {} ago",
                    plural((input.today - target).whole_days(), "day")
                )
            } else {
                format!("target {}", when_text(target, input.today))
            });
        }
        match zone {
            Zone::Outside { over, .. } => parts.push(format!(
                "{} past its target",
                plural(i64::from(over), "working day")
            )),
            Zone::Inside {
                room,
                level: ZoneLevel::Narrow | ZoneLevel::Closing,
                ..
            } => parts.push(if room == 0 {
                "no room left".to_owned()
            } else {
                format!("{} of room", plural(i64::from(room), "day"))
            }),
            _ => {}
        }
        if let Some(age) = age {
            parts.push(format!("quiet for {}", plural(i64::from(age), "day")));
        }
        if self.pinged.contains(&p.id) {
            parts.push("pinged".into());
        }
        Some(Candidate {
            node: NodeSummary {
                node: NodeRef::new(NodeType::Project, p.id),
                label: p.title.clone(),
                archived: false,
            },
            kind: BlipKind::Project,
            r: days.map_or(R_NOW, |d| radius(d, self.rings)),
            size: size_of(p.priority),
            mark: row.map_or(RadarMark::None, |r| mark_of(r.health.level)),
            fog,
            objective: self.objective_of(p.id),
            label: join(parts),
            overdue,
            date: p.target_date,
            priority: p.priority,
        })
    }
}

/// Lays the radar out. See the module comment; the rules are in the design doc.
pub fn build(input: &RadarInput) -> Radar {
    let rings = if validate_radar_rings(input.rings).is_ok() {
        input.rings
    } else {
        DEFAULT_RADAR_RINGS
    };
    let scoped = |id: Uuid| match (input.view, input.scope) {
        (View::Mine, Some(scope)) => scope.objectives.contains_key(&id),
        _ => true,
    };

    // The sectors' objectives: in scope, not done, by priority and then title.
    let mut sector_objectives: Vec<&Objective> = input
        .objectives
        .iter()
        .filter(|o| o.status != ObjectiveStatus::Done && scoped(o.id))
        .collect();
    sector_objectives.sort_by(|a, b| {
        (a.priority, a.title.to_lowercase(), a.id).cmp(&(b.priority, b.title.to_lowercase(), b.id))
    });
    // Colours are ranked among all objectives by id (ADR-0012), whatever the view.
    let mut by_id: Vec<Uuid> = input.objectives.iter().map(|o| o.id).collect();
    by_id.sort();
    by_id.dedup();
    let hue_rank: HashMap<Uuid, u32> = by_id
        .into_iter()
        .enumerate()
        .map(|(i, id)| (id, i as u32))
        .collect();

    let mut contributes: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for (from, objective) in input.contributes {
        contributes.entry(*from).or_default().push(*objective);
    }
    let ctx = Ctx {
        input,
        rings,
        objective_index: sector_objectives
            .iter()
            .enumerate()
            .map(|(i, o)| (o.id, i))
            .collect(),
        contributes,
        project_titles: input
            .projects
            .iter()
            .map(|p| (p.id, p.title.as_str()))
            .collect(),
        fog: input
            .fog
            .marks
            .iter()
            .map(|m| (m.node.id, (m.level, m.age_days)))
            .collect(),
        pinged: input
            .pings
            .due
            .iter()
            .chain(&input.pings.upcoming)
            .map(|p| p.ping.target.id)
            .collect(),
    };
    let tasks_by_id: HashMap<Uuid, &Task> = input.tasks.iter().map(|t| (t.id, t)).collect();

    // What may become a blip.
    let mut candidates: Vec<Candidate> = Vec::new();
    candidates.extend(input.tasks.iter().filter_map(|t| ctx.task_candidate(t)));
    candidates.extend(input.tasks.iter().filter_map(|t| ctx.meeting_candidate(t)));
    candidates.extend(
        input
            .projects
            .iter()
            .filter_map(|p| ctx.project_candidate(p)),
    );
    let taken: HashSet<Uuid> = candidates.iter().map(|c| c.node.node.id).collect();
    // A ping that came due on something that is not already a blip becomes one.
    let due_pings: Vec<_> = input
        .pings
        .due
        .iter()
        .filter(|row| ctx.in_scope(row.ping.target))
        .collect();
    for row in &due_pings {
        let target = row.ping.target;
        if taken.contains(&target.id) {
            continue;
        }
        let when = if row.ping.on_date < input.today {
            format!("since {}", when_text(row.ping.on_date, input.today))
        } else {
            "today".to_owned()
        };
        let mut parts = vec![row.target.label.clone()];
        parts.push(if row.ping.note.is_empty() {
            format!("ping {when}")
        } else {
            format!("ping {when}: {}", row.ping.note)
        });
        let (fog, _) = ctx.fog_of(target.id);
        candidates.push(Candidate {
            node: row.target.clone(),
            kind: BlipKind::Ping,
            r: radius(
                days_until(input.work_week, input.today, row.ping.on_date),
                rings,
            ),
            size: BlipSize::S,
            mark: RadarMark::None,
            fog,
            objective: ctx.objective_of_node(target, &tasks_by_id),
            label: join(parts),
            overdue: false,
            date: Some(row.ping.on_date),
            priority: 5,
        });
    }

    // The footer counts everything in range, before the cap.
    let mut footer = Footer {
        overdue: candidates
            .iter()
            .filter(|c| c.kind == BlipKind::Task && c.overdue)
            .count() as u32,
        due_today: candidates
            .iter()
            .filter(|c| c.kind == BlipKind::Task && c.date == Some(input.today))
            .count() as u32,
        pings_due: due_pings.len() as u32,
        approaching: 0,
        visibility: input.fog.visibility,
        hidden_blips: 0,
    };

    // Too many: keep the most urgent.
    candidates.sort_by_key(Candidate::urgency);
    if candidates.len() > MAX_BLIPS {
        footer.hidden_blips = (candidates.len() - MAX_BLIPS) as u32;
        candidates.truncate(MAX_BLIPS);
    }

    // Edge marks: trouble that hits or threatens your work.
    struct Pending<'a> {
        item: &'a minimap_types::Approaching,
        project: Option<&'a NodeSummary>,
        objective: Option<Uuid>,
    }
    let pending: Vec<Pending> = input
        .approaching
        .groups
        .iter()
        .flat_map(|g| {
            g.items
                .iter()
                .map(move |item| (g.source_project.as_ref(), item))
        })
        .filter(|(_, item)| matches!(item.severity, Severity::Hits | Severity::Threatens))
        .map(|(project, item)| Pending {
            item,
            project,
            objective: ctx.objective_of_node(item.target.node, &tasks_by_id),
        })
        .collect();

    // The sectors: every objective, and "Other" when something sits there.
    let other_blips = candidates.iter().filter(|c| c.objective.is_none()).count();
    let has_other = other_blips > 0 || pending.iter().any(|p| p.objective.is_none());
    let counts: Vec<usize> = sector_objectives
        .iter()
        .map(|o| {
            candidates
                .iter()
                .filter(|c| c.objective == Some(o.id))
                .count()
        })
        .collect();
    let widths = sweeps(&counts, has_other.then_some(other_blips));
    let mut sectors: Vec<Sector> = Vec::new();
    let mut start = 0.0;
    for (i, sweep) in widths.iter().enumerate() {
        let objective = sector_objectives.get(i).map(|o| NodeSummary {
            node: NodeRef::new(NodeType::Objective, o.id),
            label: o.title.clone(),
            archived: false,
        });
        sectors.push(Sector {
            hue_rank: sector_objectives
                .get(i)
                .and_then(|o| hue_rank.get(&o.id).copied()),
            objective,
            start,
            sweep: *sweep,
        });
        start += sweep;
    }
    let sector_of = |objective: Option<Uuid>| match objective {
        Some(id) => ctx.objective_index.get(&id).copied().unwrap_or(0),
        None => sectors.len().saturating_sub(1),
    };

    // Place the blips, then nudge any that landed on top of each other.
    let mut placed: Vec<Placed> = candidates
        .into_iter()
        .map(|cand| {
            let sector = sector_of(cand.objective);
            let theta = angle_in(&sectors[sector], cand.node.node.id);
            Placed {
                cand,
                sector,
                theta,
            }
        })
        .collect();
    placed.sort_by(|a, b| {
        a.theta
            .total_cmp(&b.theta)
            .then(a.cand.node.node.id.cmp(&b.cand.node.node.id))
    });
    let limits: Vec<(f64, f64)> = sectors.iter().map(bounds).collect();
    nudge(&mut placed, &limits);
    placed.sort_by(|a, b| {
        a.theta
            .total_cmp(&b.theta)
            .then(a.cand.node.node.id.cmp(&b.cand.node.node.id))
    });
    let blips = placed
        .into_iter()
        .map(|p| Blip {
            pinged: ctx.pinged.contains(&p.cand.node.node.id),
            node: p.cand.node,
            kind: p.cand.kind,
            r: p.cand.r.clamp(0.0, 1.0),
            theta: p.theta,
            size: p.cand.size,
            mark: p.cand.mark,
            fog: p.cand.fog,
            label: p.cand.label,
        })
        .collect();

    // Edge marks sit at their target's angle on the rim; marks that would overlap spread out.
    let mut marks: Vec<(usize, EdgeMark, Uuid)> = pending
        .iter()
        .map(|p| {
            let sector = sector_of(p.objective);
            let item = p.item;
            let source = match p.project {
                Some(project) => format!("{} ({})", item.source.label, project.label),
                None => item.source.label.clone(),
            };
            let why = item
                .reasons
                .first()
                .map_or("in trouble", |r| reason_word(*r));
            let arrives = match item.arrives_in {
                0 => "now".to_owned(),
                n => format!("in {}", plural(i64::from(n), "day")),
            };
            (
                sector,
                EdgeMark {
                    theta: angle_in(&sectors[sector], item.target.node.id),
                    severity: item.severity,
                    label: format!(
                        "{source} is {why}; reaches {} {arrives}.",
                        item.target.label
                    ),
                    source: item.source.clone(),
                },
                item.target.node.id,
            )
        })
        .collect();
    marks.sort_by(|a, b| {
        (a.1.theta, a.1.severity, a.1.source.node.id, a.2)
            .partial_cmp(&(b.1.theta, b.1.severity, b.1.source.node.id, b.2))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut previous: Option<(usize, f64)> = None;
    for (sector, mark, _) in &mut marks {
        if let Some((prev_sector, prev_theta)) = previous {
            if prev_sector == *sector && mark.theta - prev_theta < MARK_GAP {
                mark.theta = (prev_theta + MARK_GAP).min(limits[*sector].1);
            }
        }
        previous = Some((*sector, mark.theta));
    }
    let edge_marks: Vec<EdgeMark> = marks.into_iter().map(|(_, mark, _)| mark).collect();
    footer.approaching = edge_marks.len() as u32;

    Radar {
        today: input.today,
        view: input.view,
        rings,
        sectors,
        blips,
        edge_marks,
        footer,
        revision: 0,
    }
}

// ---------------------------------------------------------------- collisions

fn xy(r: f64, theta: f64) -> (f64, f64) {
    (r * theta.sin(), -r * theta.cos())
}

fn gap(a: &Placed, b: &Placed) -> f64 {
    let (ax, ay) = xy(a.cand.r, a.theta);
    let (bx, by) = xy(b.cand.r, b.theta);
    ((ax - bx).powi(2) + (ay - by).powi(2)).sqrt()
}

/// Moves blips that sit closer than [`MIN_GAP`] apart along their sector, never out of it. A few
/// deterministic passes; a sector too crowded to separate everything keeps its blips inside it
/// and as far apart as it can.
fn nudge(placed: &mut [Placed], limits: &[(f64, f64)]) {
    for _ in 0..NUDGE_PASSES {
        let mut moved = false;
        for i in 0..placed.len() {
            for j in (i + 1)..placed.len() {
                let d = gap(&placed[i], &placed[j]);
                if d >= MIN_GAP {
                    continue;
                }
                let dir = if placed[j].theta >= placed[i].theta {
                    1.0
                } else {
                    -1.0
                };
                // Close to the centre an angle moves a blip very little: cap the turn.
                let arc = placed[j].cand.r.max(0.1);
                let delta = ((MIN_GAP - d) / arc + 0.01).min(0.5);
                let (lo, hi) = limits[placed[j].sector];
                let to = (placed[j].theta + dir * delta).clamp(lo, hi);
                if (to - placed[j].theta).abs() > 1e-9 {
                    placed[j].theta = to;
                    moved = true;
                    continue;
                }
                // Against the edge of its sector: move the other one the opposite way.
                let (lo, hi) = limits[placed[i].sector];
                let to = (placed[i].theta - dir * delta).clamp(lo, hi);
                if (to - placed[i].theta).abs() > 1e-9 {
                    placed[i].theta = to;
                    moved = true;
                }
            }
        }
        if !moved {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{
        ApproachGroup, Approaching, Focus, Health, HealthThresholds, ObjectiveHealthRow,
        OverviewCounts, Ping, PingRow, ProjectHealthRow,
    };
    use proptest::prelude::*;
    use time::{macros::date, Duration, OffsetDateTime};

    // Wednesday 2027-03-03.
    const TODAY: Date = date!(2027 - 03 - 03);

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn at() -> OffsetDateTime {
        OffsetDateTime::UNIX_EPOCH
    }

    fn task(n: u128, status: TaskStatus, due: Option<Date>) -> Task {
        Task {
            id: id(n),
            title: format!("task {n}"),
            description: String::new(),
            project_id: None,
            status,
            estimate_days: None,
            start_date: None,
            due_date: due,
            completed_at: None,
            priority: 3,
            recurrence: None,
            links: Vec::new(),
            task_type: None,
            focus: None,
            start_minute: None,
            length_minutes: None,
            confirmed_on: None,
            created_at: at(),
            updated_at: at(),
            archived_at: None,
        }
    }

    fn project(n: u128, target: Option<Date>) -> Project {
        Project {
            id: id(n),
            title: format!("project {n}"),
            slug: format!("p{n}"),
            description: String::new(),
            owner_person_id: None,
            start_date: None,
            target_date: target,
            status: ProjectStatus::Active,
            priority: 3,
            confirmed_on: None,
            created_at: at(),
            updated_at: at(),
            archived_at: None,
        }
    }

    fn objective(n: u128, priority: u8) -> Objective {
        Objective {
            id: id(n),
            title: format!("objective {n}"),
            description: String::new(),
            target_date: None,
            status: ObjectiveStatus::OnTrack,
            priority,
            ongoing: false,
            review_every_days: None,
            last_reviewed_on: None,
            confirmed_on: None,
            created_at: at(),
            updated_at: at(),
            archived_at: None,
        }
    }

    fn summary(kind: NodeType, n: u128, label: &str) -> NodeSummary {
        NodeSummary {
            node: NodeRef::new(kind, id(n)),
            label: label.into(),
            archived: false,
        }
    }

    fn health(level: HealthLevel) -> Health {
        Health {
            level,
            score: 50,
            reasons: Vec::new(),
        }
    }

    fn health_row(
        n: u128,
        level: HealthLevel,
        zone: Zone,
        target: Option<Date>,
    ) -> ProjectHealthRow {
        ProjectHealthRow {
            project: summary(NodeType::Project, n, &format!("project {n}")),
            status: ProjectStatus::Active,
            priority: 3,
            health: health(level),
            projected_finish: None,
            target_date: target,
            open_tasks: 1,
            overdue_tasks: 0,
            blocked_tasks: 0,
            unestimated_tasks: 0,
            weight: None,
            zone,
        }
    }

    /// Everything a radar is built from, owned so a test can change one part.
    #[derive(Debug)]
    struct World {
        tasks: Vec<Task>,
        projects: Vec<Project>,
        objectives: Vec<Objective>,
        contributes: Vec<(Uuid, Uuid)>,
        project_rows: Vec<ProjectHealthRow>,
        fog: FogReport,
        pings: PingsView,
        approaching: ApproachReport,
        rings: [u32; 3],
    }

    impl World {
        fn new() -> Self {
            World {
                tasks: Vec::new(),
                projects: Vec::new(),
                objectives: Vec::new(),
                contributes: Vec::new(),
                project_rows: Vec::new(),
                fog: FogReport {
                    today: TODAY,
                    marks: Vec::new(),
                    visibility: Some(100),
                    in_play: 0,
                    clear: 0,
                },
                pings: PingsView {
                    due: Vec::new(),
                    upcoming: Vec::new(),
                },
                approaching: ApproachReport::default(),
                rings: DEFAULT_RADAR_RINGS,
            }
        }

        fn overview(&self) -> PortfolioOverview {
            PortfolioOverview {
                today: TODAY,
                counts: OverviewCounts {
                    red: 0,
                    amber: 0,
                    green: 0,
                    idle: 0,
                },
                objectives: Vec::<ObjectiveHealthRow>::new(),
                unlinked_projects: self.project_rows.clone(),
                risks: Vec::new(),
                more_risks: 0,
                overloaded: Vec::new(),
                stale_waiting: Vec::new(),
                warnings: Vec::new(),
                thresholds: HealthThresholds::default(),
            }
        }

        fn run_in(&self, view: View, scope: Option<&MyScope>) -> Radar {
            let overview = self.overview();
            build(&RadarInput {
                today: TODAY,
                work_week: WorkWeek::MON_FRI,
                view,
                scope,
                tasks: &self.tasks,
                projects: &self.projects,
                objectives: &self.objectives,
                contributes: &self.contributes,
                overview: &overview,
                fog: &self.fog,
                pings: &self.pings,
                approaching: &self.approaching,
                rings: self.rings,
            })
        }

        fn run(&self) -> Radar {
            self.run_in(View::Portfolio, None)
        }
    }

    fn blip(radar: &Radar, n: u128) -> &Blip {
        radar
            .blips
            .iter()
            .find(|b| b.node.node.id == id(n))
            .unwrap_or_else(|| panic!("no blip for {n}"))
    }

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    // ------------------------------------------------------------ radius

    #[test]
    fn the_radius_runs_through_each_boundary() {
        let rings = [5, 10, 60];
        assert!(approx(radius(0, rings), 0.12));
        assert!(approx(radius(5, rings), 0.4));
        assert!(approx(radius(10, rings), 0.65));
        assert!(approx(radius(60, rings), 0.95));
        // Half way through a stretch is half way between its ends.
        assert!(approx(radius(35, rings), 0.8));
        assert!(radius(2, rings) > 0.12 && radius(2, rings) < 0.4);
        // Beyond the outer ring it stays at the edge.
        assert!(approx(radius(500, rings), 0.95));
    }

    #[test]
    fn overdue_work_is_inside_the_first_stretch_and_closer_the_later_it_is() {
        let rings = [5, 10, 60];
        assert!(approx(radius(-1, rings), 0.108));
        assert!(radius(-3, rings) < radius(-1, rings));
        // Capped at ten days: after that it sits at the centre.
        assert!(approx(radius(-10, rings), 0.0));
        assert!(approx(radius(-40, rings), 0.0));
        assert!(radius(-1, rings) <= 0.12);
    }

    #[test]
    fn a_date_is_counted_in_working_days() {
        // Friday is two working days from Wednesday; the next Monday is three.
        assert_eq!(
            days_until(WorkWeek::MON_FRI, TODAY, date!(2027 - 03 - 05)),
            2
        );
        assert_eq!(
            days_until(WorkWeek::MON_FRI, TODAY, date!(2027 - 03 - 08)),
            3
        );
        // Saturday has passed on a Monday even though it counts as the next working day.
        assert_eq!(
            days_until(
                WorkWeek::MON_FRI,
                date!(2027 - 03 - 08),
                date!(2027 - 03 - 06)
            ),
            -1
        );
    }

    // ------------------------------------------------------------ what becomes a blip

    #[test]
    fn tasks_are_blips_when_overdue_near_blocked_or_in_focus() {
        let mut w = World::new();
        w.tasks = vec![
            task(1, TaskStatus::Todo, Some(TODAY - Duration::days(2))), // overdue
            task(2, TaskStatus::Todo, Some(TODAY + Duration::days(7))), // near
            task(3, TaskStatus::Blocked, None),                         // blocked
            task(4, TaskStatus::Todo, None),                            // nothing: left out
            task(5, TaskStatus::Todo, Some(TODAY + Duration::days(200))), // far: left out
            task(6, TaskStatus::Done, Some(TODAY)),                     // finished: left out
            task(7, TaskStatus::Todo, None),
        ];
        w.tasks[6].focus = Some(Focus::PINNED);
        let radar = w.run();
        let ids: Vec<u128> = radar
            .blips
            .iter()
            .map(|b| b.node.node.id.as_u128())
            .collect();
        for wanted in [1, 2, 3, 7] {
            assert!(ids.contains(&wanted), "{wanted} should be a blip: {ids:?}");
        }
        for unwanted in [4, 5, 6] {
            assert!(
                !ids.contains(&unwanted),
                "{unwanted} should not be: {ids:?}"
            );
        }
        assert!(approx(blip(&radar, 3).r, 0.5));
        assert!(approx(blip(&radar, 7).r, 0.3));
        // A focus that has ended no longer counts.
        w.tasks[6].focus = Some(Focus {
            until: Some(TODAY - Duration::days(1)),
        });
        assert!(w.run().blips.iter().all(|b| b.node.node.id != id(7)));
    }

    #[test]
    fn health_marks_and_sizes() {
        let mut w = World::new();
        w.tasks = vec![
            task(1, TaskStatus::Todo, Some(TODAY - Duration::days(1))),
            task(2, TaskStatus::Blocked, Some(TODAY + Duration::days(3))),
            task(3, TaskStatus::InProgress, Some(TODAY + Duration::days(3))),
        ];
        w.tasks[0].priority = 1;
        w.tasks[1].priority = 2;
        let radar = w.run();
        assert_eq!(blip(&radar, 1).mark, RadarMark::Red);
        assert_eq!(blip(&radar, 1).size, BlipSize::L);
        assert_eq!(blip(&radar, 2).mark, RadarMark::Amber);
        assert_eq!(blip(&radar, 2).size, BlipSize::M);
        assert_eq!(blip(&radar, 3).mark, RadarMark::Green);
        assert_eq!(blip(&radar, 3).size, BlipSize::S);
    }

    #[test]
    fn a_project_is_a_blip_when_its_target_is_near_or_its_zone_is_tight() {
        let mut w = World::new();
        let near = TODAY + Duration::days(14);
        let far = TODAY + Duration::days(400);
        w.projects = vec![
            project(1, Some(near)),
            project(2, Some(far)),
            project(3, Some(far)),
        ];
        w.projects.push(project(4, None));
        w.project_rows = vec![
            health_row(1, HealthLevel::Amber, Zone::None, Some(near)),
            health_row(
                2,
                HealthLevel::Red,
                Zone::Outside {
                    remaining: 30,
                    over: 4,
                },
                Some(far),
            ),
            health_row(
                3,
                HealthLevel::Green,
                Zone::Inside {
                    remaining: 3,
                    room: 40,
                    level: ZoneLevel::Wide,
                },
                Some(far),
            ),
        ];
        // A project that is not active is never one.
        w.projects.push(project(5, Some(near)));
        w.projects[4].status = ProjectStatus::Paused;
        let radar = w.run();
        assert_eq!(blip(&radar, 1).mark, RadarMark::Amber);
        assert_eq!(blip(&radar, 2).mark, RadarMark::Red);
        let ids: Vec<u128> = radar
            .blips
            .iter()
            .map(|b| b.node.node.id.as_u128())
            .collect();
        assert!(!ids.contains(&3), "wide zone, far target: {ids:?}");
        assert!(!ids.contains(&4) && !ids.contains(&5));
        assert!(blip(&radar, 2).label.contains("past its target"));
    }

    #[test]
    fn meetings_today_and_tomorrow_are_hollow_squares_with_no_health() {
        let mut w = World::new();
        w.tasks = vec![
            task(1, TaskStatus::Todo, Some(TODAY)),
            task(2, TaskStatus::Todo, Some(TODAY + Duration::days(1))),
            task(3, TaskStatus::Todo, Some(TODAY + Duration::days(2))),
            task(4, TaskStatus::Done, Some(TODAY)),
        ];
        for t in &mut w.tasks {
            t.task_type = Some("meeting".into());
        }
        w.tasks[0].start_minute = Some(9 * 60 + 30);
        let radar = w.run();
        assert_eq!(radar.blips.len(), 2);
        assert_eq!(blip(&radar, 1).kind, BlipKind::Meeting);
        assert_eq!(blip(&radar, 1).mark, RadarMark::None);
        assert_eq!(blip(&radar, 1).label, "task 1 · meeting · today 09:30");
        assert!(blip(&radar, 2).label.ends_with("tomorrow"));
    }

    fn ping_row(kind: NodeType, n: u128, label: &str, on: Date, note: &str) -> PingRow {
        PingRow {
            ping: Ping {
                id: id(1000 + n),
                target: NodeRef::new(kind, id(n)),
                note: note.into(),
                on_date: on,
                cleared_on: None,
                created_at: at(),
                updated_at: at(),
                archived_at: None,
            },
            target: summary(kind, n, label),
        }
    }

    #[test]
    fn a_due_ping_is_its_own_blip_unless_its_item_already_is_one() {
        let mut w = World::new();
        w.tasks = vec![task(1, TaskStatus::Todo, Some(TODAY + Duration::days(2)))];
        w.pings.due = vec![
            // On a task that is already a blip: that blip gets the pin.
            ping_row(NodeType::Task, 1, "task 1", TODAY, ""),
            ping_row(
                NodeType::Decision,
                2,
                "Postgres over Mongo",
                TODAY,
                "revisit",
            ),
        ];
        w.pings.upcoming = vec![ping_row(
            NodeType::Person,
            3,
            "Priya",
            TODAY + Duration::days(3),
            "",
        )];
        let radar = w.run();
        assert_eq!(radar.blips.len(), 2);
        assert!(blip(&radar, 1).pinged);
        assert_eq!(blip(&radar, 1).kind, BlipKind::Task);
        let ping = blip(&radar, 2);
        assert_eq!(ping.kind, BlipKind::Ping);
        assert!(ping.pinged);
        assert_eq!(ping.label, "Postgres over Mongo · ping today: revisit");
        assert_eq!(radar.footer.pings_due, 2);
    }

    // ------------------------------------------------------------ sectors

    fn two_objectives() -> World {
        let mut w = World::new();
        w.objectives = vec![objective(10, 2), objective(11, 1), objective(12, 1)];
        // Priority 1 first, then by title.
        w
    }

    #[test]
    fn sectors_follow_priority_then_title_and_other_comes_last() {
        let mut w = two_objectives();
        w.tasks = vec![task(1, TaskStatus::Blocked, None)];
        let radar = w.run();
        let labels: Vec<_> = radar
            .sectors
            .iter()
            .map(|s| s.objective.as_ref().map(|o| o.label.as_str()))
            .collect();
        assert_eq!(
            labels,
            [
                Some("objective 11"),
                Some("objective 12"),
                Some("objective 10"),
                None
            ]
        );
        assert!(approx(radar.sectors[0].start, 0.0));
        // Colours are ranked by id, not by sector order.
        assert_eq!(radar.sectors[0].hue_rank, Some(1));
        assert_eq!(radar.sectors[2].hue_rank, Some(0));
        assert_eq!(radar.sectors[3].hue_rank, None);
    }

    #[test]
    fn there_is_no_other_sector_when_nothing_belongs_there() {
        let w = two_objectives();
        let radar = w.run();
        assert_eq!(radar.sectors.len(), 3);
        assert!(radar.sectors.iter().all(|s| s.objective.is_some()));
    }

    #[test]
    fn sweeps_add_up_to_a_full_turn_and_keep_the_minimums() {
        for counts in [
            vec![],
            vec![0],
            vec![5],
            vec![0, 0, 0],
            vec![30, 1, 1, 1],
            vec![1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
            vec![3, 9, 27],
        ] {
            for other in [None, Some(0), Some(50)] {
                let out = sweeps(&counts, other);
                assert_eq!(out.len(), counts.len() + usize::from(other.is_some()));
                if out.is_empty() {
                    continue;
                }
                let total: f64 = out.iter().sum();
                assert!(approx(total, TAU), "{counts:?} {other:?}: {total}");
                if out.len() as f64 * MIN_SWEEP < TAU {
                    assert!(
                        out.iter().all(|s| *s >= MIN_SWEEP - 1e-9),
                        "{counts:?} {other:?}: {out:?}"
                    );
                    if other.is_some() && out.len() > 1 {
                        assert!(out[out.len() - 1] <= OTHER_MAX_SWEEP + 1e-9);
                    }
                }
            }
        }
    }

    #[test]
    fn a_busier_sector_is_wider() {
        let out = sweeps(&[10, 2], None);
        assert!(out[0] > out[1]);
        // "Other" never takes more than a quarter, however busy.
        let out = sweeps(&[1, 1], Some(100));
        assert!(approx(out[2], OTHER_MAX_SWEEP));
    }

    #[test]
    fn an_item_serving_two_objectives_is_in_the_first_sector_only() {
        let mut w = two_objectives();
        w.tasks = vec![task(1, TaskStatus::Blocked, None)];
        // Serves objective 10 (priority 2) and 12 (priority 1): 12 wins.
        w.contributes = vec![(id(1), id(10)), (id(1), id(12))];
        let radar = w.run();
        assert_eq!(radar.blips.len(), 1);
        let sector = &radar.sectors[1];
        assert_eq!(sector.objective.as_ref().map(|o| o.node.id), Some(id(12)));
        let theta = blip(&radar, 1).theta;
        assert!(theta >= sector.start && theta <= sector.start + sector.sweep);
    }

    #[test]
    fn a_task_serves_its_projects_objective_when_it_has_none_of_its_own() {
        let mut w = two_objectives();
        w.tasks = vec![task(1, TaskStatus::Blocked, None)];
        w.tasks[0].project_id = Some(id(20));
        w.projects = vec![project(20, None)];
        w.contributes = vec![(id(20), id(10))];
        let radar = w.run();
        let sector = &radar.sectors[2];
        assert_eq!(sector.objective.as_ref().map(|o| o.node.id), Some(id(10)));
        let theta = blip(&radar, 1).theta;
        assert!(theta >= sector.start && theta <= sector.start + sector.sweep);
        assert!(blip(&radar, 1).label.contains("project 20"));
    }

    #[test]
    fn a_finished_objective_has_no_sector() {
        let mut w = two_objectives();
        w.objectives[0].status = ObjectiveStatus::Done;
        w.tasks = vec![task(1, TaskStatus::Blocked, None)];
        w.contributes = vec![(id(1), id(10))];
        let radar = w.run();
        assert_eq!(radar.sectors.len(), 3); // two objectives and Other
        assert!(radar.sectors[2].objective.is_none());
    }

    // ------------------------------------------------------------ angles

    fn crowded() -> World {
        let mut w = World::new();
        w.objectives = vec![objective(10, 1), objective(11, 2)];
        for n in 1..=8u128 {
            let mut t = task(n, TaskStatus::Todo, Some(TODAY + Duration::days(2)));
            t.project_id = None;
            w.tasks.push(t);
            w.contributes.push((id(n), id(10)));
        }
        w
    }

    #[test]
    fn angles_are_the_same_on_every_run_and_in_any_input_order() {
        let w = crowded();
        let first = w.run();
        assert_eq!(first, w.run());
        let mut shuffled = crowded();
        shuffled.tasks.reverse();
        shuffled.contributes.reverse();
        shuffled.objectives.reverse();
        assert_eq!(first, shuffled.run());
    }

    #[test]
    fn the_nudge_keeps_blips_in_their_sector_and_apart() {
        let w = crowded();
        let radar = w.run();
        let sector = &radar.sectors[0];
        let (lo, hi) = bounds(sector);
        let inside: Vec<&Blip> = radar.blips.iter().collect();
        assert_eq!(inside.len(), 8);
        for b in &inside {
            assert!(b.theta >= lo - 1e-9 && b.theta <= hi + 1e-9, "{}", b.theta);
        }
        for (i, a) in inside.iter().enumerate() {
            for b in &inside[i + 1..] {
                let (ax, ay) = xy(a.r, a.theta);
                let (bx, by) = xy(b.r, b.theta);
                let d = ((ax - bx).powi(2) + (ay - by).powi(2)).sqrt();
                assert!(
                    d >= MIN_GAP - 1e-6,
                    "{d} between {} and {}",
                    a.label,
                    b.label
                );
            }
        }
    }

    #[test]
    fn blips_are_listed_in_angle_order() {
        let radar = crowded().run();
        assert!(radar.blips.windows(2).all(|w| w[0].theta <= w[1].theta));
    }

    // ------------------------------------------------------------ the cap

    #[test]
    fn beyond_sixty_the_most_urgent_stay() {
        let mut w = World::new();
        // 70 tasks: ten are overdue, the rest are due on later and later days.
        for n in 1..=70u128 {
            let due = if n <= 10 {
                TODAY - Duration::days(n as i64)
            } else {
                TODAY + Duration::days(n as i64 - 10)
            };
            w.tasks.push(task(n, TaskStatus::Todo, Some(due)));
        }
        let radar = w.run();
        assert_eq!(radar.blips.len(), MAX_BLIPS);
        assert_eq!(radar.footer.hidden_blips, 10);
        let kept: HashSet<u128> = radar
            .blips
            .iter()
            .map(|b| b.node.node.id.as_u128())
            .collect();
        // Every overdue one stays; the latest-due ones are the ones dropped.
        assert!((1..=10).all(|n| kept.contains(&n)));
        assert!((61..=70).all(|n| !kept.contains(&n)));
        assert!((11..=60).all(|n| kept.contains(&n)));
        // The counts are not capped.
        assert_eq!(radar.footer.overdue, 10);
    }

    #[test]
    fn at_equal_urgency_priority_decides() {
        let mut w = World::new();
        for n in 1..=61u128 {
            let mut t = task(n, TaskStatus::Todo, Some(TODAY + Duration::days(3)));
            t.priority = if n == 61 { 1 } else { 4 };
            w.tasks.push(t);
        }
        let radar = w.run();
        assert_eq!(radar.blips.len(), MAX_BLIPS);
        // The priority-1 task was last in line by id but is kept.
        assert!(radar.blips.iter().any(|b| b.node.node.id == id(61)));
    }

    // ------------------------------------------------------------ views

    #[test]
    fn my_view_shows_only_what_is_in_scope() {
        use std::collections::BTreeMap;
        let mut w = two_objectives();
        w.tasks = vec![
            task(1, TaskStatus::Blocked, None),
            task(2, TaskStatus::Blocked, None),
        ];
        let mut scope = MyScope {
            tasks: BTreeMap::new(),
            projects: BTreeMap::new(),
            objectives: BTreeMap::new(),
            people: BTreeMap::new(),
            project_tasks: BTreeMap::new(),
        };
        scope
            .tasks
            .insert(id(1), vec![minimap_types::Why::Assigned]);
        scope
            .objectives
            .insert(id(10), vec![minimap_types::Why::Serves]);
        let all = w.run_in(View::Portfolio, Some(&scope));
        assert_eq!(all.blips.len(), 2);
        assert_eq!(
            all.sectors.iter().filter(|s| s.objective.is_some()).count(),
            3
        );
        let mine = w.run_in(View::Mine, Some(&scope));
        assert_eq!(mine.view, View::Mine);
        assert_eq!(mine.blips.len(), 1);
        assert_eq!(mine.blips[0].node.node.id, id(1));
        // Only the objective that serves you has a sector (and Other for the blip).
        let objectives: Vec<_> = mine
            .sectors
            .iter()
            .filter_map(|s| s.objective.as_ref().map(|o| o.node.id))
            .collect();
        assert_eq!(objectives, vec![id(10)]);
    }

    // ------------------------------------------------------------ edge marks

    fn approaching_item(severity: Severity, source: u128, target: u128) -> Approaching {
        Approaching {
            source: summary(NodeType::Task, source, "Gateway auth"),
            target: summary(NodeType::Task, target, "Pen test"),
            path: Vec::new(),
            reasons: vec![Reason::Blocked],
            overdue_days: None,
            arrives_in: 3,
            severity,
            target_slack: None,
        }
    }

    #[test]
    fn edge_marks_exist_only_for_hits_and_threatens() {
        let mut w = two_objectives();
        w.tasks = vec![task(2, TaskStatus::Todo, Some(TODAY + Duration::days(4)))];
        w.contributes = vec![(id(2), id(12))];
        w.approaching = ApproachReport {
            groups: vec![ApproachGroup {
                source_project: Some(summary(NodeType::Project, 30, "EU Region")),
                items: vec![
                    approaching_item(Severity::Hits, 100, 2),
                    approaching_item(Severity::Threatens, 101, 2),
                    approaching_item(Severity::Watch, 102, 2),
                ],
            }],
            headline: None,
        };
        let radar = w.run();
        assert_eq!(radar.edge_marks.len(), 2);
        assert_eq!(radar.footer.approaching, 2);
        let mark = &radar.edge_marks[0];
        assert_eq!(
            mark.label,
            "Gateway auth (EU Region) is blocked; reaches Pen test in 3 days."
        );
        // At the angle of the target's sector.
        let sector = &radar.sectors[1];
        for m in &radar.edge_marks {
            assert!(m.theta >= sector.start && m.theta <= sector.start + sector.sweep);
        }
        // Two marks for one target do not sit on top of each other.
        assert!((radar.edge_marks[0].theta - radar.edge_marks[1].theta).abs() >= MARK_GAP - 1e-9);
    }

    #[test]
    fn a_mark_for_something_in_no_objective_makes_the_other_sector() {
        let mut w = two_objectives();
        w.approaching = ApproachReport {
            groups: vec![ApproachGroup {
                source_project: None,
                items: vec![approaching_item(Severity::Hits, 100, 5)],
            }],
            headline: None,
        };
        let radar = w.run();
        assert!(radar.sectors.last().is_some_and(|s| s.objective.is_none()));
        assert_eq!(radar.edge_marks.len(), 1);
        assert!(radar.edge_marks[0]
            .label
            .starts_with("Gateway auth is blocked"));
    }

    // ------------------------------------------------------------ the footer and labels

    #[test]
    fn the_footer_counts_overdue_due_today_and_visibility() {
        let mut w = World::new();
        w.tasks = vec![
            task(1, TaskStatus::Todo, Some(TODAY - Duration::days(1))),
            task(2, TaskStatus::Todo, Some(TODAY - Duration::days(5))),
            task(3, TaskStatus::Todo, Some(TODAY)),
            task(4, TaskStatus::Todo, Some(TODAY + Duration::days(1))),
        ];
        w.fog.visibility = Some(64);
        let radar = w.run();
        assert_eq!(radar.footer.overdue, 2);
        assert_eq!(radar.footer.due_today, 1);
        assert_eq!(radar.footer.visibility, Some(64));
        assert_eq!(
            radar.footer.text(),
            "2 overdue · 1 due today · visibility 64%"
        );
    }

    #[test]
    fn a_blip_says_what_it_is_in_its_hover_text() {
        let mut w = World::new();
        w.projects = vec![project(20, None)];
        w.tasks = vec![task(
            1,
            TaskStatus::Blocked,
            Some(TODAY + Duration::days(2)),
        )];
        w.tasks[0].project_id = Some(id(20));
        w.tasks[0].title = "Gateway auth".into();
        w.projects[0].title = "EU Region".into();
        let radar = w.run();
        assert_eq!(
            blip(&radar, 1).label,
            "Gateway auth · EU Region · blocked · due Fri"
        );
        // Overdue says how long, and fog and pins are named.
        w.tasks[0].due_date = Some(TODAY - Duration::days(3));
        w.fog.marks.push(minimap_types::FogMark {
            node: NodeRef::new(NodeType::Task, id(1)),
            label: "x".into(),
            level: FogLevel::Fog,
            age_days: 19,
            last_seen: TODAY - Duration::days(19),
            reason: minimap_types::FogReason::NoChange,
        });
        w.pings.upcoming = vec![ping_row(
            NodeType::Task,
            1,
            "Gateway auth",
            TODAY + Duration::days(4),
            "",
        )];
        let radar = w.run();
        let b = blip(&radar, 1);
        assert_eq!(
            b.label,
            "Gateway auth · EU Region · blocked · overdue 3 days · quiet for 19 days · pinged"
        );
        assert_eq!(b.fog, FogLevel::Fog);
        assert!(b.pinged);
    }

    #[test]
    fn a_nonsense_set_of_rings_falls_back_to_the_defaults() {
        let mut w = World::new();
        w.rings = [10, 5, 5];
        assert_eq!(w.run().rings, DEFAULT_RADAR_RINGS);
        w.rings = [2, 4, 8];
        assert_eq!(w.run().rings, [2, 4, 8]);
    }

    #[test]
    fn an_empty_world_is_an_empty_radar() {
        let radar = World::new().run();
        assert!(radar.blips.is_empty() && radar.sectors.is_empty());
        assert_eq!(radar.footer.text(), "Nothing in range this week");
    }

    // ------------------------------------------------------------ properties

    prop_compose! {
        fn world()(
            tasks in prop::collection::vec((1u128..400, 0u8..5, -30i64..120, 1u8..=5, any::<bool>()), 0..90),
            objectives in prop::collection::vec((500u128..520, 1u8..=5), 0..8),
            links in prop::collection::vec((1u128..400, 500u128..520), 0..60),
        ) -> World {
            let mut w = World::new();
            let mut seen = HashSet::new();
            for (n, status, due, priority, has_due) in tasks {
                if !seen.insert(n) { continue; }
                let status = [TaskStatus::Todo, TaskStatus::InProgress, TaskStatus::Blocked, TaskStatus::Done, TaskStatus::Cancelled][status as usize];
                let mut t = task(n, status, has_due.then(|| TODAY + Duration::days(due)));
                t.priority = priority;
                w.tasks.push(t);
            }
            let mut seen = HashSet::new();
            for (n, priority) in objectives {
                if seen.insert(n) { w.objectives.push(objective(n, priority)); }
            }
            w.contributes = links.into_iter().map(|(a, b)| (id(a), id(b))).collect();
            w
        }
    }

    proptest! {
        #[test]
        fn every_blip_is_inside_the_circle_and_its_sector(w in world()) {
            let radar = w.run();
            prop_assert!(radar.blips.len() <= MAX_BLIPS);
            let total: f64 = radar.sectors.iter().map(|s| s.sweep).sum();
            prop_assert!(radar.sectors.is_empty() || (total - TAU).abs() < 1e-6);
            for b in &radar.blips {
                prop_assert!((0.0..=1.0).contains(&b.r));
                let inside = radar.sectors.iter().any(|s| {
                    let (lo, hi) = bounds(s);
                    b.theta >= lo - 1e-9 && b.theta <= hi + 1e-9
                });
                prop_assert!(inside, "theta {} is in no sector", b.theta);
            }
        }

        #[test]
        fn the_layout_does_not_depend_on_input_order(w in world()) {
            let first = w.run();
            let mut reversed = w;
            reversed.tasks.reverse();
            reversed.objectives.reverse();
            reversed.contributes.reverse();
            prop_assert_eq!(first, reversed.run());
        }
    }
}
