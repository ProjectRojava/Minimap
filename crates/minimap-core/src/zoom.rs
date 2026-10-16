//! Zoom (spec 52, docs/product-design/09-semantic-zoom.md). Pure.
//!
//! A game's minimap scrolls out to the world map and back in to where you stand. Minimap's
//! hierarchy is objective, project, task; [`path_of`] gives the way from the whole portfolio down
//! to any item, so a screen can move one step up or down it without losing the place.
//!
//! The path of:
//! - a **task** (a meeting too) is `World`, its objective, its project, itself. The project is the
//!   task's own; the objective is the project's, or, for an inbox task, the one it contributes to
//!   directly. A level that is missing is skipped.
//! - a **project** is `World`, its objective, itself.
//! - an **objective** is `World`, itself.
//! - a **note, decision or waiting-on** is `World`, then the path *above* the first objective,
//!   project or task it mentions / affects / is about (that item's own project or objective is
//!   included, the task itself is not, so the levels keep deepening), then itself.
//! - a **person or team** is `World`, itself.
//!
//! A project that serves several objectives zooms out to one of them: the highest priority (1
//! first), then the heaviest weight, then the earliest target date (none last), then the title.
//! The others are the step's `alternatives`, in that order. Archived items and links are ignored.

use std::cmp::Ordering;

use minimap_types::{
    Edge, EdgeType, Level, NodeRef, NodeSummary, NodeType, Objective, Project, Task, Uuid, ZoomStep,
};

/// What [`path_of`] reads: the live items and the active links.
pub struct ZoomInput<'a> {
    pub tasks: &'a [Task],
    pub projects: &'a [Project],
    pub objectives: &'a [Objective],
    /// The active links (of every type; the ones that matter are picked out).
    pub edges: &'a [Edge],
    /// Names of items of other kinds that may be asked about (the note, person... itself).
    pub others: &'a [NodeSummary],
}

fn weight_of(e: &Edge) -> f64 {
    e.attrs
        .get("weight")
        .and_then(|w| w.as_f64())
        .unwrap_or(1.0)
}

fn objective_summary(o: &Objective) -> NodeSummary {
    NodeSummary {
        node: NodeRef::new(NodeType::Objective, o.id),
        label: o.title.clone(),
        archived: o.archived_at.is_some(),
    }
}

fn project_summary(p: &Project) -> NodeSummary {
    NodeSummary {
        node: NodeRef::new(NodeType::Project, p.id),
        label: p.title.clone(),
        archived: p.archived_at.is_some(),
    }
}

fn task_summary(t: &Task) -> NodeSummary {
    NodeSummary {
        node: NodeRef::new(NodeType::Task, t.id),
        label: t.title.clone(),
        archived: t.archived_at.is_some(),
    }
}

impl<'a> ZoomInput<'a> {
    fn live_edges(&self) -> impl Iterator<Item = &'a Edge> {
        self.edges.iter().filter(|e| e.archived_at.is_none())
    }

    fn live_objective(&self, id: Uuid) -> Option<&'a Objective> {
        self.objectives
            .iter()
            .find(|o| o.id == id && o.archived_at.is_none())
    }

    fn live_project(&self, id: Uuid) -> Option<&'a Project> {
        self.projects
            .iter()
            .find(|p| p.id == id && p.archived_at.is_none())
    }

    fn live_task(&self, id: Uuid) -> Option<&'a Task> {
        self.tasks
            .iter()
            .find(|t| t.id == id && t.archived_at.is_none())
    }

    /// The objectives `from` contributes to, best first: the chosen one and the alternatives.
    fn objectives_of(&self, from: NodeRef) -> Vec<NodeSummary> {
        let mut found: Vec<(&Objective, f64)> = self
            .live_edges()
            .filter(|e| e.edge_type == EdgeType::ContributesTo && e.from() == from)
            .filter_map(|e| Some((self.live_objective(e.to_id)?, weight_of(e))))
            .collect();
        found.sort_by(|(a, wa), (b, wb)| {
            a.priority
                .cmp(&b.priority)
                .then_with(|| wb.total_cmp(wa))
                .then_with(|| match (a.target_date, b.target_date) {
                    (Some(x), Some(y)) => x.cmp(&y),
                    (Some(_), None) => Ordering::Less,
                    (None, Some(_)) => Ordering::Greater,
                    (None, None) => Ordering::Equal,
                })
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
                .then_with(|| a.id.cmp(&b.id))
        });
        found.dedup_by_key(|(o, _)| o.id);
        found
            .into_iter()
            .map(|(o, _)| objective_summary(o))
            .collect()
    }

    /// The objective step above `from`, if it serves any.
    fn objective_step(&self, from: NodeRef) -> Option<ZoomStep> {
        let mut all = self.objectives_of(from).into_iter();
        let best = all.next()?;
        Some(ZoomStep {
            level: Level::Objective,
            node: Some(best),
            alternatives: all.collect(),
        })
    }

    /// The steps above a project (its objective), then the project itself.
    fn project_steps(&self, project: &Project) -> Vec<ZoomStep> {
        let mut steps = Vec::new();
        steps.extend(self.objective_step(NodeRef::new(NodeType::Project, project.id)));
        steps.push(ZoomStep::of(Level::Project, project_summary(project)));
        steps
    }

    /// The steps above a task: its project's path, or for an inbox task its own objective.
    fn task_ancestors(&self, task: &Task) -> Vec<ZoomStep> {
        match task.project_id.and_then(|p| self.live_project(p)) {
            Some(project) => self.project_steps(project),
            None => self
                .objective_step(NodeRef::new(NodeType::Task, task.id))
                .into_iter()
                .collect(),
        }
    }

    /// The first objective, project or task a note mentions, a decision affects or a waiting-on
    /// is about (the oldest link first), as long as it is still active.
    fn first_linked(&self, node: NodeRef) -> Option<NodeRef> {
        let wanted = match node.node_type {
            NodeType::Note => EdgeType::Mentions,
            NodeType::Decision => EdgeType::Affects,
            NodeType::WaitingOn => EdgeType::About,
            _ => return None,
        };
        self.live_edges()
            .filter(|e| e.edge_type == wanted && e.from() == node)
            .filter(|e| match e.to_type {
                NodeType::Objective => self.live_objective(e.to_id).is_some(),
                NodeType::Project => self.live_project(e.to_id).is_some(),
                NodeType::Task => self.live_task(e.to_id).is_some(),
                _ => false,
            })
            .min_by_key(|e| (e.created_at, e.id))
            .map(|e| e.to())
    }

    /// The name of any item: the live and archived ones this input holds, else `others`.
    fn summary_of(&self, node: NodeRef) -> NodeSummary {
        let found = match node.node_type {
            NodeType::Objective => self
                .objectives
                .iter()
                .find(|o| o.id == node.id)
                .map(objective_summary),
            NodeType::Project => self
                .projects
                .iter()
                .find(|p| p.id == node.id)
                .map(project_summary),
            NodeType::Task => self
                .tasks
                .iter()
                .find(|t| t.id == node.id)
                .map(task_summary),
            _ => None,
        };
        found
            .or_else(|| self.others.iter().find(|s| s.node == node).cloned())
            .unwrap_or(NodeSummary {
                node,
                label: String::new(),
                archived: false,
            })
    }
}

/// The way from the whole portfolio down to `node`: `World` first, `node` last, every step one
/// level deeper than the one before.
pub fn path_of(node: NodeRef, input: &ZoomInput) -> Vec<ZoomStep> {
    let mut path = vec![ZoomStep::world()];
    let own = input.summary_of(node);
    match node.node_type {
        NodeType::Objective => path.push(ZoomStep::of(Level::Objective, own)),
        NodeType::Project => {
            path.extend(input.objective_step(node));
            path.push(ZoomStep::of(Level::Project, own));
        }
        NodeType::Task => {
            if let Some(task) = input.tasks.iter().find(|t| t.id == node.id) {
                path.extend(input.task_ancestors(task));
            }
            path.push(ZoomStep::of(Level::Item, own));
        }
        NodeType::Note | NodeType::Decision | NodeType::WaitingOn => {
            if let Some(linked) = input.first_linked(node) {
                match linked.node_type {
                    NodeType::Objective => {
                        path.push(ZoomStep::of(Level::Objective, input.summary_of(linked)));
                    }
                    NodeType::Project => {
                        if let Some(project) = input.live_project(linked.id) {
                            path.extend(input.project_steps(project));
                        }
                    }
                    NodeType::Task => {
                        if let Some(task) = input.live_task(linked.id) {
                            path.extend(input.task_ancestors(task));
                        }
                    }
                    _ => {}
                }
            }
            path.push(ZoomStep::of(Level::Item, own));
        }
        NodeType::Person | NodeType::Team => path.push(ZoomStep::of(Level::Item, own)),
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{ObjectiveStatus, ProjectStatus, TaskStatus};
    use proptest::prelude::*;
    use time::{macros::date, Date, Duration, OffsetDateTime, Time};

    const DAY: Date = date!(2027 - 03 - 03);

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn at(offset_days: i64) -> OffsetDateTime {
        OffsetDateTime::new_utc(DAY + Duration::days(offset_days), Time::MIDNIGHT)
    }

    fn task(n: u128, project: Option<u128>) -> Task {
        Task {
            id: id(n),
            title: format!("T{n}"),
            description: String::new(),
            project_id: project.map(id),
            status: TaskStatus::Todo,
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
            created_at: at(0),
            updated_at: at(0),
            archived_at: None,
        }
    }

    fn project(n: u128) -> Project {
        Project {
            id: id(n),
            title: format!("P{n}"),
            slug: format!("p{n}"),
            description: String::new(),
            owner_person_id: None,
            start_date: None,
            target_date: None,
            status: ProjectStatus::Active,
            priority: 3,
            confirmed_on: None,
            created_at: at(0),
            updated_at: at(0),
            archived_at: None,
        }
    }

    fn objective(n: u128, priority: u8) -> Objective {
        Objective {
            id: id(n),
            title: format!("O{n}"),
            description: String::new(),
            target_date: None,
            status: ObjectiveStatus::OnTrack,
            priority,
            ongoing: false,
            review_every_days: None,
            last_reviewed_on: None,
            confirmed_on: None,
            created_at: at(0),
            updated_at: at(0),
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
            created_at: at(0),
            archived_at: None,
        }
    }

    fn serves(from: (NodeType, u128), objective: u128, weight: f64) -> Edge {
        let mut e = edge(
            EdgeType::ContributesTo,
            from,
            (NodeType::Objective, objective),
        );
        e.attrs = serde_json::json!({ "weight": weight });
        e
    }

    fn pr(n: u128) -> (NodeType, u128) {
        (NodeType::Project, n)
    }

    fn tk(n: u128) -> (NodeType, u128) {
        (NodeType::Task, n)
    }

    fn node(t: (NodeType, u128)) -> NodeRef {
        NodeRef::new(t.0, id(t.1))
    }

    /// Objectives 1 (priority 1) and 2 (priority 3); project 10 serves 1; project 11 serves
    /// nothing; task 100 is in project 10, task 101 in 11, task 102 is in the inbox.
    struct World {
        tasks: Vec<Task>,
        projects: Vec<Project>,
        objectives: Vec<Objective>,
        edges: Vec<Edge>,
        others: Vec<NodeSummary>,
    }

    impl World {
        fn new() -> Self {
            Self {
                tasks: vec![task(100, Some(10)), task(101, Some(11)), task(102, None)],
                projects: vec![project(10), project(11)],
                objectives: vec![objective(1, 1), objective(2, 3)],
                edges: vec![serves(pr(10), 1, 1.0)],
                others: Vec::new(),
            }
        }

        fn path(&self, n: (NodeType, u128)) -> Vec<(Level, String)> {
            let input = ZoomInput {
                tasks: &self.tasks,
                projects: &self.projects,
                objectives: &self.objectives,
                edges: &self.edges,
                others: &self.others,
            };
            path_of(node(n), &input)
                .into_iter()
                .map(|s| (s.level, s.label().to_owned()))
                .collect()
        }

        fn steps(&self, n: (NodeType, u128)) -> Vec<ZoomStep> {
            let input = ZoomInput {
                tasks: &self.tasks,
                projects: &self.projects,
                objectives: &self.objectives,
                edges: &self.edges,
                others: &self.others,
            };
            path_of(node(n), &input)
        }
    }

    fn l(level: Level, label: &str) -> (Level, String) {
        (level, label.to_owned())
    }

    #[test]
    fn a_task_zooms_out_to_its_project_its_objective_and_the_world() {
        let w = World::new();
        assert_eq!(
            w.path(tk(100)),
            vec![
                l(Level::World, "World"),
                l(Level::Objective, "O1"),
                l(Level::Project, "P10"),
                l(Level::Item, "T100"),
            ]
        );
    }

    #[test]
    fn a_project_without_an_objective_skips_that_level() {
        let w = World::new();
        assert_eq!(
            w.path(tk(101)),
            vec![
                l(Level::World, "World"),
                l(Level::Project, "P11"),
                l(Level::Item, "T101"),
            ]
        );
        assert_eq!(
            w.path(pr(11)),
            vec![l(Level::World, "World"), l(Level::Project, "P11")]
        );
    }

    #[test]
    fn an_inbox_task_goes_straight_to_the_world_or_to_its_own_objective() {
        let mut w = World::new();
        assert_eq!(
            w.path(tk(102)),
            vec![l(Level::World, "World"), l(Level::Item, "T102")]
        );
        w.edges.push(serves(tk(102), 2, 1.0));
        assert_eq!(
            w.path(tk(102)),
            vec![
                l(Level::World, "World"),
                l(Level::Objective, "O2"),
                l(Level::Item, "T102"),
            ]
        );
    }

    #[test]
    fn a_task_in_an_archived_project_is_treated_as_an_inbox_task() {
        let mut w = World::new();
        w.projects[0].archived_at = Some(at(1));
        assert_eq!(
            w.path(tk(100)),
            vec![l(Level::World, "World"), l(Level::Item, "T100")]
        );
    }

    #[test]
    fn an_objective_zooms_out_to_the_world() {
        let w = World::new();
        assert_eq!(
            w.path((NodeType::Objective, 2)),
            vec![l(Level::World, "World"), l(Level::Objective, "O2")]
        );
    }

    #[test]
    fn the_objective_with_the_highest_priority_wins_and_the_rest_are_alternatives() {
        let mut w = World::new();
        w.objectives.push(objective(3, 2));
        w.edges.push(serves(pr(10), 2, 1.0));
        w.edges.push(serves(pr(10), 3, 1.0));
        let steps = w.steps(pr(10));
        assert_eq!(steps[1].label(), "O1");
        let alts: Vec<_> = steps[1].alternatives.iter().map(|a| &a.label[..]).collect();
        // Priority 2 before priority 3.
        assert_eq!(alts, ["O3", "O2"]);
        // Only the objective step carries alternatives.
        assert!(steps[0].alternatives.is_empty() && steps[2].alternatives.is_empty());
    }

    #[test]
    fn ties_go_to_the_heavier_weight_then_the_earlier_target_then_the_title() {
        let mut w = World::new();
        w.objectives = vec![
            objective(1, 2),
            objective(2, 2),
            objective(3, 2),
            objective(4, 2),
        ];
        w.edges = vec![
            serves(pr(10), 1, 0.4),
            serves(pr(10), 2, 0.9),
            serves(pr(10), 3, 0.4),
            serves(pr(10), 4, 0.4),
        ];
        // Same priority: the heaviest weight (2) first.
        assert_eq!(w.steps(pr(10))[1].label(), "O2");
        // The rest tie on weight: a target date beats none, earlier beats later.
        w.objectives[2].target_date = Some(date!(2027 - 06 - 01));
        w.objectives[3].target_date = Some(date!(2027 - 05 - 01));
        let steps = w.steps(pr(10));
        let alts: Vec<_> = steps[1].alternatives.iter().map(|a| &a.label[..]).collect();
        assert_eq!(alts, ["O4", "O3", "O1"]);
        // With nothing else to tell them apart, the title decides (ignoring case).
        w.objectives[2].target_date = None;
        w.objectives[3].target_date = None;
        w.objectives[0].title = "alpha".into();
        w.objectives[2].title = "Beta".into();
        w.objectives[3].title = "Gamma".into();
        let steps = w.steps(pr(10));
        let alts: Vec<_> = steps[1].alternatives.iter().map(|a| &a.label[..]).collect();
        assert_eq!(alts, ["alpha", "Beta", "Gamma"]);
    }

    #[test]
    fn an_archived_objective_or_link_is_skipped() {
        let mut w = World::new();
        w.edges.push(serves(pr(10), 2, 1.0));
        w.objectives[0].archived_at = Some(at(1));
        let steps = w.steps(pr(10));
        assert_eq!(steps[1].label(), "O2");
        assert!(steps[1].alternatives.is_empty());
        // An archived link is as good as none.
        w.edges[1].archived_at = Some(at(1));
        assert_eq!(
            w.path(pr(10)),
            vec![l(Level::World, "World"), l(Level::Project, "P10")]
        );
    }

    #[test]
    fn a_note_a_decision_and_a_waiting_on_follow_the_first_thing_they_refer_to() {
        let mut w = World::new();
        let named = |t: NodeType, n: u128, label: &str| NodeSummary {
            node: NodeRef::new(t, id(n)),
            label: label.into(),
            archived: false,
        };
        w.others = vec![
            named(NodeType::Note, 200, "1:1"),
            named(NodeType::Decision, 201, "Use Postgres"),
            named(NodeType::WaitingOn, 202, "Sign-off"),
            named(NodeType::Person, 203, "Priya"),
            named(NodeType::Team, 204, "Platform"),
        ];
        // The note mentions a person first (ignored) and then a task, then a project.
        let mut mention_project = edge(EdgeType::Mentions, (NodeType::Note, 200), pr(11));
        mention_project.created_at = at(5);
        let mut mention_task = edge(EdgeType::Mentions, (NodeType::Note, 200), tk(100));
        mention_task.created_at = at(2);
        let mention_person = edge(
            EdgeType::Mentions,
            (NodeType::Note, 200),
            (NodeType::Person, 203),
        );
        w.edges
            .extend([mention_project, mention_task, mention_person]);
        // The task's own step is not repeated: the note sits below the task's project.
        assert_eq!(
            w.path((NodeType::Note, 200)),
            vec![
                l(Level::World, "World"),
                l(Level::Objective, "O1"),
                l(Level::Project, "P10"),
                l(Level::Item, "1:1"),
            ]
        );
        w.edges.push(edge(
            EdgeType::Affects,
            (NodeType::Decision, 201),
            (NodeType::Objective, 2),
        ));
        assert_eq!(
            w.path((NodeType::Decision, 201)),
            vec![
                l(Level::World, "World"),
                l(Level::Objective, "O2"),
                l(Level::Item, "Use Postgres"),
            ]
        );
        w.edges
            .push(edge(EdgeType::About, (NodeType::WaitingOn, 202), pr(11)));
        assert_eq!(
            w.path((NodeType::WaitingOn, 202)),
            vec![
                l(Level::World, "World"),
                l(Level::Project, "P11"),
                l(Level::Item, "Sign-off"),
            ]
        );
    }

    #[test]
    fn an_unlinked_note_and_a_person_or_team_go_straight_to_the_world() {
        let mut w = World::new();
        w.others = vec![NodeSummary {
            node: NodeRef::new(NodeType::Note, id(200)),
            label: "Idea".into(),
            archived: false,
        }];
        assert_eq!(
            w.path((NodeType::Note, 200)),
            vec![l(Level::World, "World"), l(Level::Item, "Idea")]
        );
        assert_eq!(
            w.path((NodeType::Person, 9)).len(),
            2,
            "a person is World then itself"
        );
        // A note whose only mention is archived has no place in the tree either.
        w.edges
            .push(edge(EdgeType::Mentions, (NodeType::Note, 200), pr(10)));
        w.projects[0].archived_at = Some(at(1));
        assert_eq!(w.path((NodeType::Note, 200)).len(), 2);
    }

    #[test]
    fn a_meeting_behaves_as_a_task() {
        let mut w = World::new();
        w.tasks[0].task_type = Some("meeting".into());
        assert_eq!(w.path(tk(100)).len(), 4);
    }

    fn build_world(
        tasks: &[Option<usize>],
        serving: &[(usize, usize, u8)],
        task_serving: &[(usize, usize)],
    ) -> World {
        // Three projects and three objectives; `tasks[i]` is the project of task i.
        let mut w = World {
            tasks: tasks
                .iter()
                .enumerate()
                .map(|(i, p)| task(100 + i as u128, p.map(|p| 10 + p as u128)))
                .collect(),
            projects: (0..3).map(|i| project(10 + i)).collect(),
            objectives: (0..3)
                .map(|i| objective(1 + i, 1 + (i % 5) as u8))
                .collect(),
            edges: Vec::new(),
            others: Vec::new(),
        };
        for &(p, o, weight) in serving {
            w.edges.push(serves(
                pr(10 + p as u128),
                1 + o as u128,
                f64::from(weight) / 10.0,
            ));
        }
        for &(t, o) in task_serving {
            w.edges
                .push(serves(tk(100 + t as u128), 1 + o as u128, 1.0));
        }
        w
    }

    proptest! {
        #[test]
        fn a_path_starts_at_the_world_ends_at_the_item_and_only_deepens(
            tasks in prop::collection::vec(prop::option::of(0usize..3), 1..8),
            serving in prop::collection::vec((0usize..3, 0usize..3, 0u8..=10), 0..8),
            task_serving in prop::collection::vec((0usize..8, 0usize..3), 0..4),
            archive_project in prop::option::of(0usize..3),
        ) {
            let mut w = build_world(&tasks, &serving, &task_serving);
            if let Some(p) = archive_project {
                w.projects[p].archived_at = Some(at(1));
            }
            let mut asked: Vec<NodeRef> = Vec::new();
            asked.extend(w.tasks.iter().map(|t| NodeRef::new(NodeType::Task, t.id)));
            asked.extend(w.projects.iter().map(|p| NodeRef::new(NodeType::Project, p.id)));
            asked.extend(w.objectives.iter().map(|o| NodeRef::new(NodeType::Objective, o.id)));
            let input = ZoomInput {
                tasks: &w.tasks,
                projects: &w.projects,
                objectives: &w.objectives,
                edges: &w.edges,
                others: &w.others,
            };
            for n in asked {
                let path = path_of(n, &input);
                prop_assert_eq!(path[0].level, Level::World);
                prop_assert!(path[0].node.is_none());
                prop_assert_eq!(path.last().and_then(|s| s.node.as_ref()).map(|s| s.node), Some(n));
                for pair in path.windows(2) {
                    prop_assert!(pair[0].level < pair[1].level);
                }
            }
        }
    }
}
