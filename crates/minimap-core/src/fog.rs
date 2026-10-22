//! Fog of war (spec 43, ADR-0019): how long ago an item that is in play was last touched or
//! confirmed. Pure. Fog is a way of looking: nothing here changes health, status or the schedule.
//!
//! *In play* means the item's state matters now (an active project, a task that is under way,
//! blocked, due soon or in focus). *Last seen* is the newest of its own change, a "still accurate",
//! a change to its links, a note that mentions it, and (for projects and objectives) what vouches
//! for it: the work under it. The age is measured in calendar days against per-kind thresholds.

use std::collections::HashMap;

use minimap_types::{
    Date, FogLevel, FogMark, FogReason, FogReport, FogThresholds, NodeRef, NodeType, Objective,
    ObjectiveStatus, Project, ProjectStatus, Task, TaskStatus, Uuid,
};
use time::OffsetDateTime;

use crate::health::priority_weight;

/// A task is "due soon" (and so in play) when it is due within this many calendar days, or late.
pub const FOG_TASK_HORIZON_DAYS: i64 = 14;

/// Everything `build` reads. The caller loads active items only.
pub struct FogInput<'a> {
    pub today: Date,
    pub thresholds: FogThresholds,
    pub tasks: &'a [Task],
    pub projects: &'a [Project],
    pub objectives: &'a [Objective],
    /// `(project or task, objective)` for every active `contributes_to` link.
    pub contributes: &'a [(Uuid, Uuid)],
    /// When each node's links last changed (the activity of the link's `from` node).
    pub last_link_change: &'a HashMap<Uuid, OffsetDateTime>,
    /// The date of the newest note that mentions each node.
    pub last_mention: &'a HashMap<Uuid, Date>,
}

/// Should this task be looked at now? Open, not a meeting, not on the backburner (spec 59), and
/// under way, blocked, due soon (or late) or in focus.
pub fn task_in_play(task: &Task, today: Date) -> bool {
    if task.archived_at.is_some()
        || task.is_meeting()
        || task.is_parked()
        || !matches!(
            task.status,
            TaskStatus::Todo | TaskStatus::InProgress | TaskStatus::Blocked
        )
    {
        return false;
    }
    matches!(task.status, TaskStatus::InProgress | TaskStatus::Blocked)
        || task
            .due_date
            .is_some_and(|d| (d - today).whole_days() <= FOG_TASK_HORIZON_DAYS)
        || task.focus.is_some_and(|f| f.is_active(today))
}

/// Only active projects are in play.
pub fn project_in_play(project: &Project) -> bool {
    project.archived_at.is_none() && project.status == ProjectStatus::Active
}

/// An objective that is not done. Ongoing ones are never fogged (they have a review rhythm).
pub fn objective_in_play(objective: &Objective) -> bool {
    objective.archived_at.is_none()
        && !objective.ongoing
        && objective.status != ObjectiveStatus::Done
}

/// Clear up to and including `haze` days, hazy up to and including `fog`, fog after that.
pub fn level(age_days: u32, haze: u32, fog: u32) -> FogLevel {
    if age_days > fog {
        FogLevel::Fog
    } else if age_days > haze {
        FogLevel::Haze
    } else {
        FogLevel::Clear
    }
}

/// How much an item counts towards visibility: its priority, times 1 for a project or objective
/// and 0.6 for a task. Haze counts half (see [`visibility`]).
fn weight(node_type: NodeType, priority: u8) -> f64 {
    priority_weight(priority)
        * match node_type {
            NodeType::Task => 0.6,
            _ => 1.0,
        }
}

/// The share of what is in play that is clear, as a whole percent: each item counts by weight,
/// clear fully, hazy half, fogged not at all. `None` when nothing is in play.
pub fn visibility(items: &[(FogLevel, f64)]) -> Option<u8> {
    let total: f64 = items.iter().map(|(_, w)| w).sum();
    if items.is_empty() || total <= 0.0 {
        return None;
    }
    let seen: f64 = items
        .iter()
        .map(|(level, w)| {
            w * match level {
                FogLevel::Clear => 1.0,
                FogLevel::Haze => 0.5,
                FogLevel::Fog => 0.0,
            }
        })
        .sum();
    Some(((seen / total) * 100.0).round().clamp(0.0, 100.0) as u8)
}

/// When something was last seen and by what: the newest of its own sightings and the dates of
/// whatever vouches for it. A "still accurate" that is later than all of them is
/// [`FogReason::OnlyConfirmed`].
fn seen(
    own: Date,
    confirmed: Option<Date>,
    others: impl IntoIterator<Item = Date>,
) -> (Date, FogReason) {
    let newest = others.into_iter().fold(own, Date::max);
    match confirmed {
        Some(c) if c > newest => (c, FogReason::OnlyConfirmed),
        _ => (newest, FogReason::NoChange),
    }
}

struct Item {
    node: NodeRef,
    label: String,
    priority: u8,
    in_play: bool,
    seen: (Date, FogReason),
    haze: u32,
    fog: u32,
}

/// The fog over everything in play.
pub fn build(input: &FogInput) -> FogReport {
    let today = input.today;
    let t = &input.thresholds;
    let link_day = |id: Uuid| input.last_link_change.get(&id).map(|at| at.date());
    let mention_day = |id: Uuid| input.last_mention.get(&id).copied();
    let sightings = |id: Uuid| link_day(id).into_iter().chain(mention_day(id));

    // Tasks first: projects and objectives are kept clear by what moves under them.
    let mut task_seen: HashMap<Uuid, Date> = HashMap::new();
    let mut items: Vec<Item> = Vec::new();
    for task in input.tasks.iter().filter(|x| x.archived_at.is_none()) {
        let s = seen(
            task.updated_at.date(),
            task.confirmed_on,
            sightings(task.id),
        );
        task_seen.insert(task.id, s.0);
        items.push(Item {
            node: NodeRef::new(NodeType::Task, task.id),
            label: task.title.clone(),
            priority: task.priority,
            in_play: task_in_play(task, today),
            seen: s,
            haze: t.task_haze,
            fog: t.task_fog,
        });
    }
    let mut project_seen: HashMap<Uuid, Date> = HashMap::new();
    for project in input.projects.iter().filter(|x| x.archived_at.is_none()) {
        let tasks_moving = input
            .tasks
            .iter()
            .filter(|x| x.project_id == Some(project.id) && task_in_play(x, today))
            .filter_map(|x| task_seen.get(&x.id).copied());
        let s = seen(
            project.updated_at.date(),
            project.confirmed_on,
            sightings(project.id).chain(tasks_moving),
        );
        project_seen.insert(project.id, s.0);
        items.push(Item {
            node: NodeRef::new(NodeType::Project, project.id),
            label: project.title.clone(),
            priority: project.priority,
            in_play: project_in_play(project),
            seen: s,
            haze: t.project_haze,
            fog: t.project_fog,
        });
    }
    for objective in input.objectives.iter().filter(|x| x.archived_at.is_none()) {
        let contributors = input
            .contributes
            .iter()
            .filter(|(_, o)| *o == objective.id)
            .filter_map(|(from, _)| {
                project_seen
                    .get(from)
                    .or_else(|| task_seen.get(from))
                    .copied()
            });
        let s = seen(
            objective.updated_at.date(),
            objective.confirmed_on,
            sightings(objective.id).chain(contributors),
        );
        items.push(Item {
            node: NodeRef::new(NodeType::Objective, objective.id),
            label: objective.title.clone(),
            priority: objective.priority,
            in_play: objective_in_play(objective),
            seen: s,
            haze: t.objective_haze,
            fog: t.objective_fog,
        });
    }

    let mut marks = Vec::new();
    let mut weights = Vec::new();
    let mut clear = 0u32;
    for item in items.iter().filter(|i| i.in_play) {
        let age_days = (today - item.seen.0).whole_days().max(0) as u32;
        let lvl = level(age_days, item.haze, item.fog);
        weights.push((lvl, weight(item.node.node_type, item.priority)));
        if lvl == FogLevel::Clear {
            clear += 1;
        } else {
            marks.push(FogMark {
                node: item.node,
                label: item.label.clone(),
                level: lvl,
                age_days,
                last_seen: item.seen.0,
                reason: item.seen.1,
            });
        }
    }
    // Fog before haze, then the oldest first.
    marks.sort_by(|a, b| {
        b.level
            .cmp(&a.level)
            .then(b.age_days.cmp(&a.age_days))
            .then_with(|| a.label.cmp(&b.label))
            .then_with(|| a.node.id.cmp(&b.node.id))
    });
    FogReport {
        today,
        marks,
        visibility: visibility(&weights),
        in_play: weights.len() as u32,
        clear,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::Focus;
    use proptest::prelude::*;
    use time::{macros::date, Duration, Time};

    const TODAY: Date = date!(2027 - 03 - 03);

    fn at(day: Date) -> OffsetDateTime {
        day.with_time(Time::MIDNIGHT).assume_utc()
    }

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn task(n: u128, status: TaskStatus, due: Option<Date>, touched: Date) -> Task {
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
            backburner_since: None,
            start_minute: None,
            length_minutes: None,
            confirmed_on: None,
            created_at: at(touched),
            updated_at: at(touched),
            archived_at: None,
        }
    }

    fn project(n: u128, status: ProjectStatus, touched: Date) -> Project {
        Project {
            id: id(n),
            title: format!("project {n}"),
            slug: format!("p{n}"),
            description: String::new(),
            owner_person_id: None,
            start_date: None,
            target_date: None,
            status,
            priority: 3,
            confirmed_on: None,
            created_at: at(touched),
            updated_at: at(touched),
            archived_at: None,
        }
    }

    fn objective(n: u128, touched: Date) -> Objective {
        Objective {
            id: id(n),
            title: format!("objective {n}"),
            description: String::new(),
            target_date: None,
            status: ObjectiveStatus::OnTrack,
            priority: 3,
            ongoing: false,
            review_every_days: None,
            last_reviewed_on: None,
            confirmed_on: None,
            created_at: at(touched),
            updated_at: at(touched),
            archived_at: None,
        }
    }

    fn ago(days: i64) -> Date {
        TODAY - Duration::days(days)
    }

    fn run(
        tasks: &[Task],
        projects: &[Project],
        objectives: &[Objective],
        contributes: &[(Uuid, Uuid)],
        links: &HashMap<Uuid, OffsetDateTime>,
        mentions: &HashMap<Uuid, Date>,
    ) -> FogReport {
        build(&FogInput {
            today: TODAY,
            thresholds: FogThresholds::default(),
            tasks,
            projects,
            objectives,
            contributes,
            last_link_change: links,
            last_mention: mentions,
        })
    }

    fn simple(tasks: &[Task]) -> FogReport {
        run(tasks, &[], &[], &[], &HashMap::new(), &HashMap::new())
    }

    #[test]
    fn levels_are_strictly_greater_than_the_thresholds() {
        assert_eq!(level(7, 7, 14), FogLevel::Clear);
        assert_eq!(level(8, 7, 14), FogLevel::Haze);
        assert_eq!(level(14, 7, 14), FogLevel::Haze);
        assert_eq!(level(15, 7, 14), FogLevel::Fog);
        assert_eq!(level(0, 7, 14), FogLevel::Clear);
    }

    #[test]
    fn a_task_on_the_backburner_is_never_in_play() {
        let mut parked = task(1, TaskStatus::InProgress, Some(ago(5)), ago(0));
        assert!(task_in_play(&parked, TODAY));
        parked.backburner_since = Some(ago(10));
        assert!(!task_in_play(&parked, TODAY));
        parked.focus = Some(Focus::PINNED);
        assert!(!task_in_play(&parked, TODAY));
    }

    #[test]
    fn a_task_is_in_play_when_under_way_blocked_due_soon_late_or_in_focus() {
        let far = Some(TODAY + Duration::days(60));
        let soon = Some(TODAY + Duration::days(14));
        let too_far = Some(TODAY + Duration::days(15));
        assert!(task_in_play(
            &task(1, TaskStatus::InProgress, far, ago(0)),
            TODAY
        ));
        assert!(task_in_play(
            &task(1, TaskStatus::Blocked, None, ago(0)),
            TODAY
        ));
        assert!(task_in_play(
            &task(1, TaskStatus::Todo, soon, ago(0)),
            TODAY
        ));
        assert!(task_in_play(
            &task(1, TaskStatus::Todo, Some(ago(5)), ago(0)),
            TODAY
        ));
        assert!(!task_in_play(
            &task(1, TaskStatus::Todo, too_far, ago(0)),
            TODAY
        ));
        assert!(!task_in_play(
            &task(1, TaskStatus::Todo, None, ago(0)),
            TODAY
        ));
        let mut focused = task(1, TaskStatus::Todo, far, ago(0));
        focused.focus = Some(Focus::PINNED);
        assert!(task_in_play(&focused, TODAY));
        focused.focus = Some(Focus {
            until: Some(ago(1)),
        });
        assert!(!task_in_play(&focused, TODAY));
    }

    #[test]
    fn closed_archived_and_meeting_tasks_are_never_in_play() {
        for status in [TaskStatus::Done, TaskStatus::Cancelled] {
            assert!(!task_in_play(
                &task(1, status, Some(ago(3)), ago(30)),
                TODAY
            ));
        }
        let mut gone = task(1, TaskStatus::InProgress, None, ago(30));
        gone.archived_at = Some(at(ago(1)));
        assert!(!task_in_play(&gone, TODAY));
        let mut meeting = task(1, TaskStatus::Todo, Some(TODAY), ago(30));
        meeting.task_type = Some("meeting".into());
        meeting.start_minute = Some(600);
        assert!(!task_in_play(&meeting, TODAY));
        assert!(simple(&[meeting]).marks.is_empty());
    }

    #[test]
    fn an_untouched_task_in_play_hazes_then_fogs() {
        let r = simple(&[
            task(1, TaskStatus::InProgress, None, ago(7)),
            task(2, TaskStatus::InProgress, None, ago(8)),
            task(3, TaskStatus::InProgress, None, ago(15)),
        ]);
        assert_eq!(r.in_play, 3);
        assert_eq!(r.clear, 1);
        let levels: Vec<(u128, FogLevel, u32)> = r
            .marks
            .iter()
            .map(|m| (m.node.id.as_u128(), m.level, m.age_days))
            .collect();
        // Fog first, then haze.
        assert_eq!(levels, [(3, FogLevel::Fog, 15), (2, FogLevel::Haze, 8)]);
        assert_eq!(r.marks[0].last_seen, ago(15));
        assert_eq!(r.marks[0].reason, FogReason::NoChange);
    }

    #[test]
    fn a_far_off_task_nobody_touched_is_not_fogged() {
        let r = simple(&[task(
            1,
            TaskStatus::Todo,
            Some(TODAY + Duration::days(180)),
            ago(90),
        )]);
        assert!(r.marks.is_empty());
        assert_eq!((r.in_play, r.visibility), (0, None));
    }

    #[test]
    fn confirming_clears_and_says_so_when_it_is_all_that_happened() {
        let mut t = task(1, TaskStatus::InProgress, None, ago(20));
        t.confirmed_on = Some(ago(3));
        assert!(simple(std::slice::from_ref(&t)).marks.is_empty());
        // Once it ages again, the hover can say it was only confirmed.
        t.confirmed_on = Some(ago(10));
        let r = simple(&[t]);
        assert_eq!(r.marks[0].reason, FogReason::OnlyConfirmed);
        assert_eq!(r.marks[0].last_seen, ago(10));
    }

    #[test]
    fn a_later_change_beats_an_older_confirmation() {
        let mut t = task(1, TaskStatus::InProgress, None, ago(2));
        t.confirmed_on = Some(ago(10));
        assert!(simple(&[t]).marks.is_empty());
    }

    #[test]
    fn a_note_or_a_change_to_its_links_keeps_a_task_in_sight() {
        let t = task(1, TaskStatus::InProgress, None, ago(30));
        let mut notes = HashMap::new();
        notes.insert(id(1), ago(2));
        let r = run(
            std::slice::from_ref(&t),
            &[],
            &[],
            &[],
            &HashMap::new(),
            &notes,
        );
        assert!(r.marks.is_empty());
        let mut links = HashMap::new();
        links.insert(id(1), at(ago(4)));
        let r = run(&[t], &[], &[], &[], &links, &HashMap::new());
        assert!(r.marks.is_empty());
    }

    #[test]
    fn a_project_is_kept_clear_by_a_moving_task_and_a_note() {
        let p = project(10, ProjectStatus::Active, ago(30));
        let mut moving = task(1, TaskStatus::InProgress, None, ago(1));
        moving.project_id = Some(id(10));
        let r = run(
            std::slice::from_ref(&moving),
            std::slice::from_ref(&p),
            &[],
            &[],
            &HashMap::new(),
            &HashMap::new(),
        );
        assert!(r.marks.is_empty());
        // A closed task does not vouch for it.
        moving.status = TaskStatus::Done;
        let r = run(
            &[moving],
            std::slice::from_ref(&p),
            &[],
            &[],
            &HashMap::new(),
            &HashMap::new(),
        );
        assert_eq!(r.marks.len(), 1);
        assert_eq!(r.marks[0].level, FogLevel::Fog);
        // A note does.
        let mut notes = HashMap::new();
        notes.insert(id(10), ago(1));
        let r = run(&[], &[p], &[], &[], &HashMap::new(), &notes);
        assert!(r.marks.is_empty());
    }

    #[test]
    fn only_active_projects_are_in_play() {
        let r = run(
            &[],
            &[
                project(1, ProjectStatus::Planned, ago(60)),
                project(2, ProjectStatus::Paused, ago(60)),
                project(3, ProjectStatus::Done, ago(60)),
                project(4, ProjectStatus::Cancelled, ago(60)),
                project(5, ProjectStatus::Active, ago(60)),
            ],
            &[],
            &[],
            &HashMap::new(),
            &HashMap::new(),
        );
        assert_eq!(r.in_play, 1);
        assert_eq!(r.marks[0].node.id, id(5));
    }

    #[test]
    fn an_objective_is_kept_clear_by_a_contributing_project() {
        let o = objective(20, ago(40));
        let p = project(10, ProjectStatus::Active, ago(2));
        let contributes = [(id(10), id(20))];
        let r = run(
            &[],
            &[p],
            std::slice::from_ref(&o),
            &contributes,
            &HashMap::new(),
            &HashMap::new(),
        );
        assert!(r.marks.is_empty());
        let r = run(&[], &[], &[o], &[], &HashMap::new(), &HashMap::new());
        assert_eq!(r.marks.len(), 1);
        assert_eq!(r.marks[0].node.node_type, NodeType::Objective);
    }

    #[test]
    fn ongoing_and_done_objectives_are_not_fogged() {
        let mut ongoing = objective(1, ago(90));
        ongoing.ongoing = true;
        let mut done = objective(2, ago(90));
        done.status = ObjectiveStatus::Done;
        let r = run(
            &[],
            &[],
            &[ongoing, done],
            &[],
            &HashMap::new(),
            &HashMap::new(),
        );
        assert_eq!((r.in_play, r.marks.len()), (0, 0));
    }

    #[test]
    fn visibility_weighs_by_priority_and_counts_haze_as_half() {
        use FogLevel::*;
        assert_eq!(visibility(&[]), None);
        assert_eq!(visibility(&[(Clear, 1.0), (Clear, 2.0)]), Some(100));
        assert_eq!(visibility(&[(Fog, 1.0)]), Some(0));
        assert_eq!(visibility(&[(Haze, 1.0)]), Some(50));
        // One clear (weight 3) and one fogged (weight 1): 75%.
        assert_eq!(visibility(&[(Clear, 3.0), (Fog, 1.0)]), Some(75));
    }

    #[test]
    fn the_report_counts_what_is_in_play_and_clear() {
        let r = simple(&[
            task(1, TaskStatus::InProgress, None, ago(1)),
            task(2, TaskStatus::InProgress, None, ago(1)),
            task(3, TaskStatus::InProgress, None, ago(30)),
        ]);
        assert_eq!((r.in_play, r.clear), (3, 2));
        assert_eq!(r.visibility, Some(((2.0f64 / 3.0) * 100.0).round() as u8));
        assert_eq!(r.mark(id(3)).map(|m| m.level), Some(FogLevel::Fog));
        assert!(r.mark(id(1)).is_none());
    }

    #[test]
    fn older_comes_first_within_a_level() {
        let r = simple(&[
            task(1, TaskStatus::InProgress, None, ago(9)),
            task(2, TaskStatus::InProgress, None, ago(12)),
        ]);
        assert_eq!(r.marks[0].node.id, id(2));
    }

    fn day(offset: i64) -> Date {
        TODAY + Duration::days(offset)
    }

    proptest! {
        // Confirming any item never makes anything worse and makes that item clear.
        #[test]
        fn confirming_never_makes_a_level_worse(
            ages in proptest::collection::vec((0i64..80, 0u8..3), 1..8),
            pick in 0usize..8,
        ) {
            let tasks: Vec<Task> = ages
                .iter()
                .enumerate()
                .map(|(i, (age, kind))| {
                    let status = match kind {
                        0 => TaskStatus::InProgress,
                        1 => TaskStatus::Blocked,
                        _ => TaskStatus::Todo,
                    };
                    task(i as u128 + 1, status, Some(TODAY + Duration::days(*age % 20)), TODAY - Duration::days(*age))
                })
                .collect();
            let before = simple(&tasks);
            let mut confirmed = tasks.clone();
            let i = pick % confirmed.len();
            confirmed[i].confirmed_on = Some(TODAY);
            let after = simple(&confirmed);
            let level_of = |r: &FogReport, id: Uuid| r.mark(id).map_or(FogLevel::Clear, |m| m.level);
            for t in &tasks {
                prop_assert!(level_of(&after, t.id) <= level_of(&before, t.id));
            }
            prop_assert_eq!(level_of(&after, confirmed[i].id), FogLevel::Clear);
        }

        // Visibility stays within 0..=100, and marks hold only fogged, in-play items.
        #[test]
        fn visibility_is_a_percent_and_marks_are_fogged_items_in_play(
            ages in proptest::collection::vec((0i64..120, 0u8..4, 0i64..40), 0..10),
        ) {
            let tasks: Vec<Task> = ages
                .iter()
                .enumerate()
                .map(|(i, (age, kind, due))| {
                    let status = match kind {
                        0 => TaskStatus::InProgress,
                        1 => TaskStatus::Blocked,
                        2 => TaskStatus::Done,
                        _ => TaskStatus::Todo,
                    };
                    task(i as u128 + 1, status, Some(day(*due)), TODAY - Duration::days(*age))
                })
                .collect();
            let r = simple(&tasks);
            if let Some(v) = r.visibility {
                prop_assert!(v <= 100);
            } else {
                prop_assert_eq!(r.in_play, 0);
            }
            for m in &r.marks {
                prop_assert!(m.level != FogLevel::Clear);
                let t = tasks.iter().find(|t| t.id == m.node.id).unwrap();
                prop_assert!(task_in_play(t, TODAY));
            }
            prop_assert!(r.clear <= r.in_play);
        }
    }
}
