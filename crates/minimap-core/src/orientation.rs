//! Portfolio view and My view (docs/product-design/07-orientation.md, spec 50). Pure.
//!
//! Game minimaps are north-up (the world stays fixed) or player-up (the map turns with you). The
//! screens are north-up; My view is the other one: everything as it touches **you**. [`my_scope`]
//! works out what that is, and why each thing is in it, so a screen can show a reason beside it.
//!
//! Rules, in this order (an item keeps every reason it has, in this order):
//! 1. **Tasks** that are open, or finished in the last [`RECENT_DAYS`] days, and are assigned to
//!    you (`Assigned`) or in a project you own (`InMyProject`).
//! 2. **Projects** you own (`Owner`); a project with one of those tasks is in too (no reason of its
//!    own: the screen says "3 of your tasks").
//! 3. **Reach**: from each of your open tasks, up to `MAX_HOPS` `blocks` links back through open
//!    tasks (the same walk as *Approaching*, [`crate::approach::walk_back`]) are `Reaches`, with
//!    their projects; one hop forward are `Blocks`. A project you own reaches back along
//!    `depends_on` for `MAX_PROJECT_HOPS`.
//! 4. **Objectives** your own projects and tasks contribute to (`Serves`; what only reaches you
//!    does not count).
//! 5. **People**: you (`Me`), those who report to you (`Report`) and those assigned to tasks in
//!    your projects (`OnMyProjects`).
//! 6. Without a "me" nothing is in scope.

use std::collections::{BTreeSet, HashMap, HashSet};

use minimap_types::{
    Date, Edge, EdgeType, MyScope, NodeType, Objective, Person, Project, Task, TaskStatus, Uuid,
    Why, MAX_HOPS,
};

use crate::approach::{walk_back, MAX_PROJECT_HOPS};

/// A task finished this recently is still in My view, so the board's Done column is not empty.
pub const RECENT_DAYS: i64 = 14;

/// Everything `my_scope` reads.
pub struct ScopeInput<'a> {
    /// The "me" person; without one the scope is empty.
    pub self_id: Option<Uuid>,
    pub today: Date,
    pub tasks: &'a [Task],
    pub projects: &'a [Project],
    pub objectives: &'a [Objective],
    pub people: &'a [Person],
    /// The active links (of every type; the ones that matter are picked out).
    pub edges: &'a [Edge],
}

fn add(list: &mut std::collections::BTreeMap<Uuid, Vec<Why>>, id: Uuid, why: Why) {
    let reasons = list.entry(id).or_default();
    if !reasons.contains(&why) {
        reasons.push(why);
    }
}

fn is_open(t: &Task) -> bool {
    t.archived_at.is_none() && !matches!(t.status, TaskStatus::Done | TaskStatus::Cancelled)
}

/// What is in My view and why.
pub fn my_scope(input: &ScopeInput) -> MyScope {
    let mut scope = MyScope::default();
    let Some(me) = input.self_id else {
        return scope;
    };
    let edges = || input.edges.iter().filter(|e| e.archived_at.is_none());
    let task_by_id: HashMap<Uuid, &Task> = input
        .tasks
        .iter()
        .filter(|t| t.archived_at.is_none())
        .map(|t| (t.id, t))
        .collect();
    let project_by_id: HashMap<Uuid, &Project> = input
        .projects
        .iter()
        .filter(|p| p.archived_at.is_none())
        .map(|p| (p.id, p))
        .collect();
    let recent_from = input.today - time::Duration::days(RECENT_DAYS);
    let in_window = |t: &Task| {
        is_open(t)
            || (t.status == TaskStatus::Done
                && t.completed_at.is_some_and(|at| at.date() >= recent_from))
    };

    // ------------------------------------------------------------------ 1. tasks
    let assigned: HashSet<Uuid> = edges()
        .filter(|e| {
            e.edge_type == EdgeType::AssignedTo && e.from_type == NodeType::Task && e.to_id == me
        })
        .map(|e| e.from_id)
        .collect();
    let my_projects: BTreeSet<Uuid> = project_by_id
        .values()
        .filter(|p| p.owner_person_id == Some(me))
        .map(|p| p.id)
        .collect();
    let mut by_id: Vec<&Task> = task_by_id.values().copied().collect();
    by_id.sort_by_key(|t| t.id);
    for t in by_id.iter().filter(|t| in_window(t)) {
        if assigned.contains(&t.id) {
            add(&mut scope.tasks, t.id, Why::Assigned);
        }
        if t.project_id.is_some_and(|p| my_projects.contains(&p)) {
            add(&mut scope.tasks, t.id, Why::InMyProject);
        }
    }

    // ---------------------------------------------------------------- 2. projects
    for p in &my_projects {
        add(&mut scope.projects, *p, Why::Owner);
    }
    for id in scope.tasks.keys() {
        if let Some(p) = task_by_id
            .get(id)
            .and_then(|t| t.project_id)
            .filter(|p| project_by_id.contains_key(p))
        {
            scope.projects.entry(p).or_default();
        }
    }
    for id in scope.tasks.keys() {
        if let Some(p) = task_by_id.get(id).and_then(|t| t.project_id) {
            if project_by_id.contains_key(&p) {
                *scope.project_tasks.entry(p).or_default() += 1;
            }
        }
    }
    // What is yours, before anything that only reaches you is added.
    let own_tasks: BTreeSet<Uuid> = scope.tasks.keys().copied().collect();
    let own_projects: BTreeSet<Uuid> = scope.projects.keys().copied().collect();

    // ----------------------------------------------------------------- 3. reach
    let mut blockers: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    let mut blocked: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    let mut needs: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for e in edges() {
        match e.edge_type {
            EdgeType::Blocks => {
                blockers.entry(e.to_id).or_default().push(e.from_id);
                blocked.entry(e.from_id).or_default().push(e.to_id);
            }
            EdgeType::DependsOn => needs.entry(e.from_id).or_default().push(e.to_id),
            _ => {}
        }
    }
    for list in blockers
        .values_mut()
        .chain(blocked.values_mut())
        .chain(needs.values_mut())
    {
        list.sort();
        list.dedup();
    }
    let starts: Vec<Uuid> = own_tasks
        .iter()
        .copied()
        .filter(|id| task_by_id.get(id).is_some_and(|t| is_open(t)))
        .collect();
    let reach = |scope: &mut MyScope, id: Uuid, why: Why| {
        add(&mut scope.tasks, id, why);
        if let Some(p) = task_by_id
            .get(&id)
            .and_then(|t| t.project_id)
            .filter(|p| project_by_id.contains_key(p) && !own_projects.contains(p))
        {
            add(&mut scope.projects, p, why);
        }
    };
    // Upstream first, for every task of yours, then one hop downstream.
    for &start in &starts {
        walk_back(start, &blockers, MAX_HOPS, |pred, _| {
            let Some(task) = task_by_id.get(&pred).filter(|t| is_open(t)) else {
                return false; // finished work breaks the chain
            };
            if !own_tasks.contains(&task.id) {
                reach(&mut scope, pred, Why::Reaches { via: start });
            }
            true
        });
    }
    for &start in &starts {
        for &after in blocked.get(&start).into_iter().flatten() {
            if task_by_id.get(&after).is_some_and(|t| is_open(t)) && !own_tasks.contains(&after) {
                reach(&mut scope, after, Why::Blocks { via: start });
            }
        }
    }
    for &owned in &my_projects {
        walk_back(owned, &needs, MAX_PROJECT_HOPS, |q, _| {
            if !project_by_id.contains_key(&q) {
                return false;
            }
            if !own_projects.contains(&q) {
                add(&mut scope.projects, q, Why::Reaches { via: owned });
            }
            true
        });
    }

    // ------------------------------------------------------------- 4. objectives
    let live_objectives: HashSet<Uuid> = input
        .objectives
        .iter()
        .filter(|o| o.archived_at.is_none())
        .map(|o| o.id)
        .collect();
    for e in edges().filter(|e| {
        e.edge_type == EdgeType::ContributesTo
            && e.to_type == NodeType::Objective
            && live_objectives.contains(&e.to_id)
    }) {
        let mine = match e.from_type {
            NodeType::Project => own_projects.contains(&e.from_id),
            NodeType::Task => own_tasks.contains(&e.from_id),
            _ => false,
        };
        if mine {
            add(&mut scope.objectives, e.to_id, Why::Serves);
        }
    }

    // ----------------------------------------------------------------- 5. people
    let live_people: HashSet<Uuid> = input
        .people
        .iter()
        .filter(|p| p.archived_at.is_none())
        .map(|p| p.id)
        .collect();
    add(&mut scope.people, me, Why::Me);
    for e in edges().filter(|e| {
        e.edge_type == EdgeType::ReportsTo
            && e.from_type == NodeType::Person
            && e.to_id == me
            && live_people.contains(&e.from_id)
    }) {
        add(&mut scope.people, e.from_id, Why::Report);
    }
    for e in edges().filter(|e| {
        e.edge_type == EdgeType::AssignedTo
            && e.from_type == NodeType::Task
            && e.to_id != me
            && live_people.contains(&e.to_id)
    }) {
        if scope
            .tasks
            .get(&e.from_id)
            .is_some_and(|why| why.contains(&Why::InMyProject))
        {
            add(&mut scope.people, e.to_id, Why::OnMyProjects);
        }
    }
    scope
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{ObjectiveStatus, ProjectStatus};
    use proptest::prelude::*;
    use time::{macros::date, Duration, OffsetDateTime, Time};

    // Wednesday 2027-03-03.
    const TODAY: Date = date!(2027 - 03 - 03);
    const ME: u128 = 1;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn at(d: Date) -> OffsetDateTime {
        OffsetDateTime::new_utc(d, Time::MIDNIGHT)
    }

    fn task(n: u128, status: TaskStatus, project: Option<u128>) -> Task {
        Task {
            id: id(n),
            title: format!("T{n}"),
            description: String::new(),
            project_id: project.map(id),
            status,
            estimate_days: Some(1.0),
            start_date: None,
            due_date: None,
            completed_at: None,
            priority: 3,
            recurrence: None,
            links: Vec::new(),
            task_type: None,
            focus: None,
            start_minute: None,
            length_minutes: None,
            confirmed_on: None,
            created_at: at(TODAY),
            updated_at: at(TODAY),
            archived_at: None,
        }
    }

    fn open(n: u128, project: Option<u128>) -> Task {
        task(n, TaskStatus::Todo, project)
    }

    fn project(n: u128, owner: Option<u128>) -> Project {
        Project {
            id: id(n),
            title: format!("P{n}"),
            slug: format!("p{n}"),
            description: String::new(),
            owner_person_id: owner.map(id),
            start_date: None,
            target_date: None,
            status: ProjectStatus::Active,
            priority: 3,
            confirmed_on: None,
            created_at: at(TODAY),
            updated_at: at(TODAY),
            archived_at: None,
        }
    }

    fn objective(n: u128) -> Objective {
        Objective {
            id: id(n),
            title: format!("O{n}"),
            description: String::new(),
            target_date: None,
            status: ObjectiveStatus::OnTrack,
            priority: 3,
            ongoing: false,
            review_every_days: None,
            last_reviewed_on: None,
            confirmed_on: None,
            created_at: at(TODAY),
            updated_at: at(TODAY),
            archived_at: None,
        }
    }

    fn person(n: u128) -> Person {
        Person {
            id: id(n),
            name: format!("Person {n}"),
            role_title: String::new(),
            email: None,
            weekly_capacity_hours: 40.0,
            is_self: n == ME,
            notes: String::new(),
            created_at: at(TODAY),
            updated_at: at(TODAY),
            archived_at: None,
        }
    }

    fn edge(edge_type: EdgeType, from: (NodeType, u128), to: (NodeType, u128)) -> Edge {
        Edge {
            id: Uuid::from_u128(10_000 + from.1 * 1000 + to.1),
            edge_type,
            from_type: from.0,
            from_id: id(from.1),
            to_type: to.0,
            to_id: id(to.1),
            attrs: serde_json::json!({}),
            created_at: at(TODAY),
            archived_at: None,
        }
    }

    fn blocks(from: u128, to: u128) -> Edge {
        edge(
            EdgeType::Blocks,
            (NodeType::Task, from),
            (NodeType::Task, to),
        )
    }

    fn assign(task: u128, who: u128) -> Edge {
        edge(
            EdgeType::AssignedTo,
            (NodeType::Task, task),
            (NodeType::Person, who),
        )
    }

    struct World {
        tasks: Vec<Task>,
        projects: Vec<Project>,
        objectives: Vec<Objective>,
        people: Vec<Person>,
        edges: Vec<Edge>,
    }

    impl World {
        fn new() -> Self {
            World {
                tasks: Vec::new(),
                projects: Vec::new(),
                objectives: Vec::new(),
                people: vec![person(ME), person(2), person(3), person(4)],
                edges: Vec::new(),
            }
        }

        fn scope(&self) -> MyScope {
            self.scope_for(Some(id(ME)))
        }

        fn scope_for(&self, me: Option<Uuid>) -> MyScope {
            my_scope(&ScopeInput {
                self_id: me,
                today: TODAY,
                tasks: &self.tasks,
                projects: &self.projects,
                objectives: &self.objectives,
                people: &self.people,
                edges: &self.edges,
            })
        }
    }

    fn why(list: &std::collections::BTreeMap<Uuid, Vec<Why>>, n: u128) -> Option<Vec<Why>> {
        list.get(&id(n)).cloned()
    }

    #[test]
    fn without_a_me_nothing_is_in_scope() {
        let mut w = World::new();
        w.tasks.push(open(10, None));
        w.edges.push(assign(10, ME));
        assert!(w.scope_for(None).is_empty());
        assert!(!w.scope().is_empty());
    }

    #[test]
    fn my_tasks_are_assigned_ones_and_those_in_projects_i_own_with_every_reason_in_order() {
        let mut w = World::new();
        w.projects.push(project(100, Some(ME)));
        w.projects.push(project(101, Some(2)));
        w.tasks.push(open(10, Some(101))); // assigned to me, in someone else's project
        w.tasks.push(open(11, Some(100))); // in my project, assigned to Person 2
        w.tasks.push(open(12, Some(100))); // both
        w.tasks.push(open(13, Some(101))); // neither
        w.edges
            .extend([assign(10, ME), assign(11, 2), assign(12, ME)]);
        let s = w.scope();
        assert_eq!(why(&s.tasks, 10), Some(vec![Why::Assigned]));
        assert_eq!(why(&s.tasks, 11), Some(vec![Why::InMyProject]));
        assert_eq!(
            why(&s.tasks, 12),
            Some(vec![Why::Assigned, Why::InMyProject])
        );
        assert_eq!(why(&s.tasks, 13), None);
    }

    #[test]
    fn only_open_work_and_work_finished_in_the_last_14_days_counts() {
        let mut w = World::new();
        let mut recent = task(10, TaskStatus::Done, None);
        recent.completed_at = Some(at(TODAY - Duration::days(14)));
        let mut old = task(11, TaskStatus::Done, None);
        old.completed_at = Some(at(TODAY - Duration::days(15)));
        let cancelled = task(12, TaskStatus::Cancelled, None);
        let mut archived = open(13, None);
        archived.archived_at = Some(at(TODAY));
        w.tasks
            .extend([recent, old, cancelled, archived, open(14, None)]);
        w.edges.extend([
            assign(10, ME),
            assign(11, ME),
            assign(12, ME),
            assign(13, ME),
            assign(14, ME),
        ]);
        let s = w.scope();
        let ids: Vec<u128> = s.tasks.keys().map(|k| k.as_u128()).collect();
        assert_eq!(ids, [10, 14]);
    }

    #[test]
    fn a_project_is_in_when_i_own_it_or_it_holds_one_of_my_tasks() {
        let mut w = World::new();
        w.projects.extend([
            project(100, Some(ME)),
            project(101, None),
            project(102, None),
        ]);
        w.tasks.push(open(10, Some(101)));
        w.edges.push(assign(10, ME));
        let s = w.scope();
        assert_eq!(why(&s.projects, 100), Some(vec![Why::Owner]));
        assert_eq!(
            why(&s.projects, 101),
            Some(Vec::new()),
            "in, with no reason of its own"
        );
        assert_eq!(why(&s.projects, 102), None);
        // Archived projects are never in.
        w.projects[0].archived_at = Some(at(TODAY));
        assert_eq!(why(&w.scope().projects, 100), None);
    }

    #[test]
    fn what_blocks_my_work_reaches_me_up_to_four_links_back_and_their_projects_come_too() {
        let mut w = World::new();
        w.projects.extend([project(100, None), project(101, None)]);
        // 20 -> 19 -> 18 -> 17 -> 16 -> 10 (mine): 16 is one link back, 20 is five.
        w.tasks.push(open(10, Some(100)));
        for n in 16..=20 {
            w.tasks.push(open(n, Some(101)));
        }
        w.edges.push(assign(10, ME));
        w.edges.extend([
            blocks(16, 10),
            blocks(17, 16),
            blocks(18, 17),
            blocks(19, 18),
            blocks(20, 19),
        ]);
        let s = w.scope();
        for n in 16..=19 {
            assert_eq!(
                why(&s.tasks, n),
                Some(vec![Why::Reaches { via: id(10) }]),
                "{n}"
            );
        }
        assert_eq!(why(&s.tasks, 20), None, "five links back is too far");
        assert_eq!(
            why(&s.projects, 101),
            Some(vec![Why::Reaches { via: id(10) }])
        );
    }

    #[test]
    fn reach_stops_at_finished_work_and_does_not_start_from_finished_work() {
        let mut w = World::new();
        w.tasks.push(open(10, None));
        w.tasks.push(task(11, TaskStatus::Done, None)); // finished blocker
        w.tasks.push(open(12, None)); // behind the finished one
        w.edges.push(assign(10, ME));
        w.edges.extend([blocks(11, 10), blocks(12, 11)]);
        let s = w.scope();
        assert_eq!(why(&s.tasks, 11), None);
        assert_eq!(why(&s.tasks, 12), None, "finished work breaks the chain");
        // My finished task reaches nothing (it is in, being recent, but starts no walk).
        let mut w = World::new();
        let mut done = task(10, TaskStatus::Done, None);
        done.completed_at = Some(at(TODAY));
        w.tasks.extend([done, open(11, None)]);
        w.edges.extend([assign(10, ME), blocks(11, 10)]);
        let s = w.scope();
        assert!(why(&s.tasks, 10).is_some());
        assert_eq!(why(&s.tasks, 11), None);
    }

    #[test]
    fn what_my_work_blocks_is_one_hop_down_only() {
        let mut w = World::new();
        w.projects.push(project(101, None));
        w.tasks
            .extend([open(10, None), open(11, Some(101)), open(12, None)]);
        w.edges.push(assign(10, ME));
        w.edges.extend([blocks(10, 11), blocks(11, 12)]);
        let s = w.scope();
        assert_eq!(why(&s.tasks, 11), Some(vec![Why::Blocks { via: id(10) }]));
        assert_eq!(why(&s.tasks, 12), None, "two hops down is not in");
        assert_eq!(
            why(&s.projects, 101),
            Some(vec![Why::Blocks { via: id(10) }])
        );
    }

    #[test]
    fn a_task_can_reach_me_and_block_me_and_keeps_both_in_that_order() {
        let mut w = World::new();
        // 11 blocks my 10 and is blocked by my 12.
        w.tasks
            .extend([open(10, None), open(11, None), open(12, None)]);
        w.edges.extend([
            assign(10, ME),
            assign(12, ME),
            blocks(11, 10),
            blocks(12, 11),
        ]);
        let s = w.scope();
        assert_eq!(
            why(&s.tasks, 11),
            Some(vec![
                Why::Reaches { via: id(10) },
                Why::Blocks { via: id(12) }
            ])
        );
    }

    #[test]
    fn projects_my_projects_depend_on_reach_me_up_to_three_links_back() {
        let mut w = World::new();
        w.projects.extend([
            project(100, Some(ME)),
            project(101, None),
            project(102, None),
            project(103, None),
            project(104, None),
        ]);
        let depends = |a: u128, b: u128| {
            edge(
                EdgeType::DependsOn,
                (NodeType::Project, a),
                (NodeType::Project, b),
            )
        };
        w.edges.extend([
            depends(100, 101),
            depends(101, 102),
            depends(102, 103),
            depends(103, 104),
        ]);
        let s = w.scope();
        for n in 101..=103 {
            assert_eq!(
                why(&s.projects, n),
                Some(vec![Why::Reaches { via: id(100) }]),
                "{n}"
            );
        }
        assert_eq!(why(&s.projects, 104), None);
    }

    #[test]
    fn an_objective_is_served_only_by_my_own_projects_and_tasks() {
        let mut w = World::new();
        w.objectives
            .extend([objective(200), objective(201), objective(202)]);
        w.projects
            .extend([project(100, Some(ME)), project(101, None)]);
        w.tasks.extend([open(10, None), open(11, Some(101))]);
        let serves = |from: (NodeType, u128), o: u128| {
            edge(EdgeType::ContributesTo, from, (NodeType::Objective, o))
        };
        w.edges.extend([
            assign(10, ME),
            serves((NodeType::Project, 100), 200), // a project I own
            serves((NodeType::Task, 10), 201),     // one of my tasks
            // Someone else's project that only reaches me.
            blocks(11, 10),
            serves((NodeType::Project, 101), 202),
        ]);
        let s = w.scope();
        assert_eq!(why(&s.objectives, 200), Some(vec![Why::Serves]));
        assert_eq!(why(&s.objectives, 201), Some(vec![Why::Serves]));
        assert_eq!(why(&s.objectives, 202), None, "reaching me is not serving");
        assert!(why(&s.projects, 101).is_some(), "but the project is in");
    }

    #[test]
    fn people_are_me_my_reports_and_those_on_my_projects_in_that_order() {
        let mut w = World::new();
        w.projects.push(project(100, Some(ME)));
        w.tasks
            .extend([open(10, Some(100)), open(11, Some(100)), open(12, None)]);
        let reports_to = |a: u128, b: u128| {
            edge(
                EdgeType::ReportsTo,
                (NodeType::Person, a),
                (NodeType::Person, b),
            )
        };
        w.edges.extend([
            reports_to(2, ME),
            reports_to(3, 2), // reports to my report, not to me
            assign(10, 2),    // my report, also working on my project
            assign(11, 3),
            assign(12, 4), // a task outside my projects
        ]);
        let s = w.scope();
        assert_eq!(why(&s.people, ME), Some(vec![Why::Me]));
        assert_eq!(
            why(&s.people, 2),
            Some(vec![Why::Report, Why::OnMyProjects])
        );
        assert_eq!(why(&s.people, 3), Some(vec![Why::OnMyProjects]));
        assert_eq!(why(&s.people, 4), None);
    }

    #[test]
    fn a_meeting_assigned_to_me_is_in_like_any_task_and_feeds_no_reach() {
        let mut w = World::new();
        let mut meeting = open(10, None);
        meeting.task_type = Some(minimap_types::MEETING_TYPE.to_owned());
        meeting.due_date = Some(TODAY);
        meeting.start_minute = Some(600);
        meeting.length_minutes = Some(30);
        w.tasks.extend([meeting, open(11, None)]);
        w.edges.push(assign(10, ME));
        let s = w.scope();
        assert_eq!(why(&s.tasks, 10), Some(vec![Why::Assigned]));
        assert_eq!(why(&s.tasks, 11), None);
    }

    /// Everything `smaller` has, `bigger` has too. (Membership, not reasons: a task that was in
    /// only because my work blocked it is, once it is mine, in for being assigned instead.)
    fn contains_all(bigger: &MyScope, smaller: &MyScope) -> bool {
        let covers = |big: &std::collections::BTreeMap<Uuid, Vec<Why>>,
                      small: &std::collections::BTreeMap<Uuid, Vec<Why>>| {
            small.keys().all(|k| big.contains_key(k))
        };
        covers(&bigger.tasks, &smaller.tasks)
            && covers(&bigger.projects, &smaller.projects)
            && covers(&bigger.objectives, &smaller.objectives)
            && covers(&bigger.people, &smaller.people)
    }

    fn random_world(links: &[(u8, u8)], owners: &[u8], assigned: &[u8]) -> World {
        let mut w = World::new();
        w.projects.extend([project(100, None), project(101, None)]);
        w.objectives.push(objective(200));
        for n in 10..18u128 {
            w.tasks.push(open(n, Some(100 + (n % 2))));
        }
        for (a, b) in links {
            let (a, b) = (10 + u128::from(*a % 8), 10 + u128::from(*b % 8));
            if a != b {
                w.edges.push(blocks(a, b));
            }
        }
        for o in owners {
            if *o % 2 == 0 {
                w.projects[usize::from(*o % 2)].owner_person_id = Some(id(ME));
            }
            w.edges.push(edge(
                EdgeType::ContributesTo,
                (NodeType::Project, 100 + u128::from(*o % 2)),
                (NodeType::Objective, 200),
            ));
        }
        for a in assigned {
            w.edges.push(assign(
                10 + u128::from(*a % 8),
                if a % 3 == 0 { 2 } else { 3 },
            ));
        }
        w
    }

    proptest! {
        /// Assigning one more task to me never takes anything out of My view.
        #[test]
        fn assigning_a_task_to_me_never_removes_anything(
            links in proptest::collection::vec((0u8..8, 0u8..8), 0..12),
            owners in proptest::collection::vec(0u8..4, 0..3),
            assigned in proptest::collection::vec(0u8..8, 0..6),
            extra in 0u8..8,
        ) {
            let mut w = random_world(&links, &owners, &assigned);
            let before = w.scope();
            w.edges.push(assign(10 + u128::from(extra), ME));
            let after = w.scope();
            prop_assert!(contains_all(&after, &before), "{before:?} -> {after:?}");
        }
    }
}
