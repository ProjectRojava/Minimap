//! How subtasks and `blocks` links work together (spec 29, ADR-0013). Pure.
//!
//! A task with subtasks is a **summary**: it groups work and is never scheduled itself. Only the
//! leaf tasks (those with no live subtasks) are, and a `blocks` link on a summary is read as
//! applying to every leaf under it:
//!
//! - `P blocks X`: X waits for every leaf of P;
//! - `X blocks P`: every leaf of P waits for X;
//! - a link between a task and one of its own ancestors or descendants says nothing useful
//!   (the group would wait for part of itself), so it is refused when it is made and ignored by
//!   the schedule if sync brings one in.
//!
//! The same functions serve the schedule, impact analysis, capacity and health, and the checks
//! that refuse a link.

use std::collections::{HashMap, HashSet};

use minimap_types::{Edge, EdgeType, Task, TaskStatus, Uuid};
use petgraph::{algo::toposort, graphmap::DiGraphMap};

use crate::cycles::find_cycle;

/// Parent and children among the live tasks (not cancelled, not archived).
#[derive(Debug, Clone, Default)]
pub struct Hierarchy {
    parent: HashMap<Uuid, Uuid>,
    children: HashMap<Uuid, Vec<Uuid>>,
}

fn is_live(t: &Task) -> bool {
    t.status != TaskStatus::Cancelled && t.archived_at.is_none()
}

impl Hierarchy {
    /// From the tasks and every active edge (only active `subtask_of` edges between live tasks
    /// count). A task with two parents (two devices set different ones) keeps the one with the
    /// smaller id, so every device agrees; a link that would close a loop is ignored.
    pub fn new(tasks: &[Task], edges: &[Edge]) -> Self {
        let live: HashSet<Uuid> = tasks.iter().filter(|t| is_live(t)).map(|t| t.id).collect();
        let mut pairs: Vec<(Uuid, Uuid)> = edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::SubtaskOf && e.archived_at.is_none())
            .filter(|e| live.contains(&e.from_id) && live.contains(&e.to_id))
            .map(|e| (e.from_id, e.to_id))
            .collect();
        pairs.sort();
        Self::from_pairs(pairs)
    }

    /// From `(child, parent)` pairs, in the order given (the first parent of a child wins).
    pub fn from_pairs(pairs: impl IntoIterator<Item = (Uuid, Uuid)>) -> Self {
        let mut h = Hierarchy::default();
        for (child, parent) in pairs {
            if child == parent || h.parent.contains_key(&child) || h.is_ancestor(child, parent) {
                continue;
            }
            h.parent.insert(child, parent);
            h.children.entry(parent).or_default().push(child);
        }
        h
    }

    pub fn parent(&self, id: Uuid) -> Option<Uuid> {
        self.parent.get(&id).copied()
    }

    /// Has live subtasks, so it groups work instead of being scheduled.
    pub fn is_summary(&self, id: Uuid) -> bool {
        self.children.contains_key(&id)
    }

    /// `ancestor` is above `id` (its parent, that one's parent, ...).
    pub fn is_ancestor(&self, ancestor: Uuid, id: Uuid) -> bool {
        let mut at = id;
        let mut seen = HashSet::new();
        while let Some(p) = self.parent(at) {
            if p == ancestor {
                return true;
            }
            if !seen.insert(p) {
                break;
            }
            at = p;
        }
        false
    }

    /// One is the other's ancestor.
    pub fn related(&self, a: Uuid, b: Uuid) -> bool {
        self.is_ancestor(a, b) || self.is_ancestor(b, a)
    }

    /// The tasks that carry the work under `id`: its leaf descendants, or `id` itself when it
    /// has no subtasks.
    pub fn leaves_under(&self, id: Uuid) -> Vec<Uuid> {
        let mut out = Vec::new();
        let mut stack = vec![id];
        let mut seen = HashSet::new();
        while let Some(t) = stack.pop() {
            if !seen.insert(t) {
                continue;
            }
            match self.children.get(&t) {
                Some(kids) => stack.extend(kids.iter().rev().copied()),
                None => out.push(t),
            }
        }
        out
    }

    /// Every summary task.
    pub fn summaries(&self) -> impl Iterator<Item = Uuid> + '_ {
        self.children.keys().copied()
    }
}

/// `blocks` links `(from, to, lag)` read on the leaf tasks: each end becomes the leaves under
/// it. Links between a task and its own ancestor or descendant are dropped, and so are pairs
/// that would link a leaf to itself. Duplicates keep the longest lag.
pub fn expand_blocks(h: &Hierarchy, blocks: &[(Uuid, Uuid, f64)]) -> Vec<(Uuid, Uuid, f64)> {
    let mut best: HashMap<(Uuid, Uuid), f64> = HashMap::new();
    let mut order: Vec<(Uuid, Uuid)> = Vec::new();
    for &(from, to, lag) in blocks {
        if from == to || h.related(from, to) {
            continue;
        }
        for f in h.leaves_under(from) {
            for t in h.leaves_under(to) {
                if f == t {
                    continue;
                }
                match best.get_mut(&(f, t)) {
                    Some(l) => *l = l.max(lag),
                    None => {
                        best.insert((f, t), lag);
                        order.push((f, t));
                    }
                }
            }
        }
    }
    order.into_iter().map(|k| (k.0, k.1, best[&k])).collect()
}

/// The first `blocks` link between a task and its own ancestor or descendant, if any.
pub fn relative_block(h: &Hierarchy, blocks: &[(Uuid, Uuid)]) -> Option<(Uuid, Uuid)> {
    blocks.iter().copied().find(|&(a, b)| h.related(a, b))
}

/// The subtasks of one group in the order the work goes: `blocks` links between them put one
/// after another, and subtasks with no order between them keep the order they were `children`
/// given in (oldest first). A loop among them (which the checks prevent) leaves the rest in
/// the given order.
pub fn sequence(children: &[Uuid], blocks: &[(Uuid, Uuid)]) -> Vec<Uuid> {
    let in_group: HashSet<Uuid> = children.iter().copied().collect();
    let mut waits_for: HashMap<Uuid, HashSet<Uuid>> = HashMap::new();
    for &(a, b) in blocks {
        if a != b && in_group.contains(&a) && in_group.contains(&b) {
            waits_for.entry(b).or_default().insert(a);
        }
    }
    let mut out: Vec<Uuid> = Vec::with_capacity(children.len());
    let mut placed: HashSet<Uuid> = HashSet::new();
    while out.len() < children.len() {
        // The first one (in the given order) with nothing left to wait for; none means a loop.
        let next = children.iter().copied().find(|c| {
            !placed.contains(c)
                && waits_for
                    .get(c)
                    .is_none_or(|w| w.iter().all(|p| placed.contains(p)))
        });
        let Some(next) = next else {
            out.extend(children.iter().copied().filter(|c| !placed.contains(c)));
            break;
        };
        placed.insert(next);
        out.push(next);
    }
    out
}

/// The next step of a group: the first subtask in `order` that is still open and not waiting
/// for anything.
pub fn next_step(
    order: &[Uuid],
    open: impl Fn(Uuid) -> bool,
    waiting: impl Fn(Uuid) -> bool,
) -> Option<Uuid> {
    order.iter().copied().find(|&c| open(c) && !waiting(c))
}

/// A suggestion for the status of a group, from how its direct subtasks stand. Only ever a
/// suggestion: the user decides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusHint {
    pub status: TaskStatus,
    pub text: &'static str,
}

/// What the group's status could be, given its own status and its subtasks' (cancelled ones
/// don't count). `None` when it already fits or has nothing to go on.
pub fn status_hint(own: TaskStatus, subtasks: &[TaskStatus]) -> Option<StatusHint> {
    use TaskStatus::*;
    if matches!(own, Done | Cancelled) {
        return None;
    }
    let live: Vec<TaskStatus> = subtasks
        .iter()
        .copied()
        .filter(|s| *s != Cancelled)
        .collect();
    if live.is_empty() {
        return None;
    }
    let open: Vec<TaskStatus> = live.iter().copied().filter(|s| *s != Done).collect();
    if open.is_empty() {
        return Some(StatusHint {
            status: Done,
            text: "All subtasks are done.",
        });
    }
    if open.iter().all(|s| *s == Blocked) {
        return (own != Blocked).then_some(StatusHint {
            status: Blocked,
            text: "Every open subtask is blocked.",
        });
    }
    let started = live.iter().any(|s| matches!(s, InProgress | Done));
    (own == Todo && started).then_some(StatusHint {
        status: InProgress,
        text: "Work has started on its subtasks.",
    })
}

/// Why a `blocks` link or a new parent is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// A task and one of its own ancestors or descendants (first = the one that would block).
    Relatives(Uuid, Uuid),
    /// The work would wait for itself; the path reads `[a, b, ..., a]` on the leaf tasks.
    /// Empty when only the fact of the loop is known (a parent change).
    Loop(Vec<Uuid>),
}

/// Checks a new `blocks` link `new = (from, to)` against the existing links `blocks`, with the
/// subtasks in `h`: not between relatives, and no loop once groups are read as their leaves.
pub fn check_new_block(
    h: &Hierarchy,
    blocks: &[(Uuid, Uuid)],
    new: (Uuid, Uuid),
) -> Result<(), Refusal> {
    if new.0 != new.1 && h.related(new.0, new.1) {
        return Err(Refusal::Relatives(new.0, new.1));
    }
    let existing = leaf_pairs(h, blocks);
    for (f, t, _) in expand_blocks(h, &[(new.0, new.1, 0.0)]) {
        if let Some(path) = find_cycle(&existing, f, t) {
            return Err(Refusal::Loop(path));
        }
    }
    Ok(())
}

/// Checks the subtasks in `h` (with a new or changed parent in them) against the `blocks`
/// links: none between a task and its own ancestor or descendant, and no loop among the leaves.
pub fn check_new_parent(h: &Hierarchy, blocks: &[(Uuid, Uuid)]) -> Result<(), Refusal> {
    if let Some((a, b)) = relative_block(h, blocks) {
        return Err(Refusal::Relatives(a, b));
    }
    let pairs = leaf_pairs(h, blocks);
    let graph: DiGraphMap<Uuid, ()> = pairs.iter().copied().collect();
    if toposort(&graph, None).is_err() {
        return Err(Refusal::Loop(Vec::new()));
    }
    Ok(())
}

fn leaf_pairs(h: &Hierarchy, blocks: &[(Uuid, Uuid)]) -> Vec<(Uuid, Uuid)> {
    let with_lag: Vec<(Uuid, Uuid, f64)> = blocks.iter().map(|&(a, b)| (a, b, 0.0)).collect();
    expand_blocks(h, &with_lag)
        .into_iter()
        .map(|(a, b, _)| (a, b))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    /// 1 is the parent of 2, 3 and 4; 4 is the parent of 5 and 6.
    fn tree() -> Hierarchy {
        Hierarchy::from_pairs([
            (id(2), id(1)),
            (id(3), id(1)),
            (id(4), id(1)),
            (id(5), id(4)),
            (id(6), id(4)),
        ])
    }

    #[test]
    fn leaves_are_the_tasks_with_no_subtasks_under_a_summary() {
        let h = tree();
        assert!(h.is_summary(id(1)) && h.is_summary(id(4)));
        assert!(!h.is_summary(id(2)) && !h.is_summary(id(9)));
        assert_eq!(h.leaves_under(id(1)), vec![id(2), id(3), id(5), id(6)]);
        assert_eq!(h.leaves_under(id(4)), vec![id(5), id(6)]);
        assert_eq!(h.leaves_under(id(2)), vec![id(2)]);
        assert_eq!(h.leaves_under(id(9)), vec![id(9)]);
    }

    #[test]
    fn ancestry_is_transitive_and_not_symmetric() {
        let h = tree();
        assert!(h.is_ancestor(id(1), id(5)));
        assert!(!h.is_ancestor(id(5), id(1)));
        assert!(h.related(id(5), id(1)) && h.related(id(1), id(5)));
        assert!(!h.related(id(2), id(3)));
    }

    #[test]
    fn a_link_that_would_close_a_loop_or_give_a_second_parent_is_ignored() {
        let h = Hierarchy::from_pairs([
            (id(2), id(1)),
            (id(1), id(2)),
            (id(3), id(1)),
            (id(3), id(2)),
            (id(4), id(4)),
        ]);
        assert_eq!(h.parent(id(2)), Some(id(1)));
        assert_eq!(h.parent(id(1)), None);
        assert_eq!(h.parent(id(3)), Some(id(1)));
        assert_eq!(h.parent(id(4)), None);
    }

    #[test]
    fn a_block_on_a_summary_applies_to_every_leaf_under_it() {
        let h = tree();
        // 7 blocks the group: it holds up every leaf of 1.
        let into = expand_blocks(&h, &[(id(7), id(1), 0.0)]);
        let to: Vec<Uuid> = into.iter().map(|e| e.1).collect();
        assert_eq!(to, vec![id(2), id(3), id(5), id(6)]);
        assert!(into.iter().all(|e| e.0 == id(7)));
        // The group blocks 8: 8 waits for every leaf.
        let out = expand_blocks(&h, &[(id(1), id(8), 2.0)]);
        assert_eq!(out.len(), 4);
        assert!(out.iter().all(|e| e.1 == id(8) && e.2 == 2.0));
        // A block on a nested summary only reaches its own leaves.
        assert_eq!(expand_blocks(&h, &[(id(7), id(4), 0.0)]).len(), 2);
    }

    #[test]
    fn plain_blocks_between_leaves_are_unchanged_and_relatives_are_dropped() {
        let h = tree();
        assert_eq!(
            expand_blocks(&h, &[(id(2), id(3), 1.0)]),
            vec![(id(2), id(3), 1.0)]
        );
        // A group against its own part, either way round, or against itself.
        assert!(expand_blocks(
            &h,
            &[
                (id(1), id(5), 0.0),
                (id(5), id(1), 0.0),
                (id(1), id(1), 0.0)
            ]
        )
        .is_empty());
        assert_eq!(
            relative_block(&h, &[(id(2), id(3)), (id(6), id(1))]),
            Some((id(6), id(1)))
        );
        assert_eq!(relative_block(&h, &[(id(2), id(3))]), None);
    }

    #[test]
    fn a_new_block_is_refused_between_relatives_and_when_it_closes_a_loop_through_a_group() {
        let h = tree();
        assert_eq!(
            check_new_block(&h, &[], (id(1), id(5))),
            Err(Refusal::Relatives(id(1), id(5)))
        );
        assert_eq!(
            check_new_block(&h, &[], (id(6), id(1))),
            Err(Refusal::Relatives(id(6), id(1)))
        );
        assert_eq!(check_new_block(&h, &[], (id(2), id(3))), Ok(()));
        // 7 blocks the group; a part of the group may not block 7.
        let existing = [(id(7), id(1))];
        match check_new_block(&h, &existing, (id(2), id(7))) {
            Err(Refusal::Loop(path)) => {
                assert_eq!(path.first(), path.last());
                assert!(path.contains(&id(7)) && path.contains(&id(2)));
            }
            other => panic!("{other:?}"),
        }
        // A loop between plain leaves is found too.
        assert!(matches!(
            check_new_block(&h, &[(id(7), id(8))], (id(8), id(7))),
            Err(Refusal::Loop(_))
        ));
    }

    #[test]
    fn a_new_parent_is_refused_when_it_puts_a_block_between_relatives_or_makes_a_loop() {
        // 2 blocks 3, so neither may become the parent of the other.
        let h = Hierarchy::from_pairs([(id(3), id(2))]);
        assert_eq!(
            check_new_parent(&h, &[(id(2), id(3))]),
            Err(Refusal::Relatives(id(2), id(3)))
        );
        // 9 blocks 1 (the group), 2 (a part) blocks 9: a loop once 2 is under 1.
        let h = Hierarchy::from_pairs([(id(2), id(1))]);
        assert_eq!(
            check_new_parent(&h, &[(id(9), id(1)), (id(2), id(9))]),
            Err(Refusal::Loop(Vec::new()))
        );
        assert_eq!(check_new_parent(&h, &[(id(9), id(1))]), Ok(()));
        assert_eq!(check_new_parent(&Hierarchy::default(), &[]), Ok(()));
    }

    #[test]
    fn subtasks_come_in_the_order_the_work_goes() {
        let kids = [id(10), id(11), id(12), id(13)];
        // No links: as given.
        assert_eq!(sequence(&kids, &[]), kids);
        // 12 must come first, then 10; 11 and 13 are free and keep their place after what they
        // don't depend on.
        let order = sequence(&kids, &[(id(12), id(10))]);
        assert_eq!(order, [id(11), id(12), id(10), id(13)]);
        // A chain, and links to tasks outside the group are not part of the order.
        let order = sequence(
            &kids,
            &[(id(13), id(12)), (id(12), id(11)), (id(99), id(10))],
        );
        assert_eq!(order, [id(10), id(13), id(12), id(11)]);
        // A loop doesn't lose anything.
        let order = sequence(&kids, &[(id(10), id(11)), (id(11), id(10))]);
        assert_eq!(order.len(), 4);
    }

    #[test]
    fn the_next_step_is_the_first_open_one_that_is_not_waiting() {
        let order = [id(1), id(2), id(3)];
        let open = |c: Uuid| c != id(1);
        assert_eq!(next_step(&order, open, |c| c == id(2)), Some(id(3)));
        assert_eq!(next_step(&order, open, |_| false), Some(id(2)));
        assert_eq!(next_step(&order, |_| false, |_| false), None);
        assert_eq!(next_step(&order, open, |_| true), None);
    }

    #[test]
    fn a_group_gets_a_status_suggestion_from_its_subtasks() {
        use TaskStatus::*;
        let hint = |own, subs: &[TaskStatus]| status_hint(own, subs).map(|h| h.status);
        assert_eq!(hint(Todo, &[Done, Done]), Some(Done));
        assert_eq!(hint(InProgress, &[Done, Cancelled]), Some(Done));
        assert_eq!(hint(Todo, &[Todo, InProgress]), Some(InProgress));
        assert_eq!(hint(Todo, &[Done, Todo]), Some(InProgress));
        assert_eq!(hint(InProgress, &[Todo, InProgress]), None);
        assert_eq!(hint(Todo, &[Todo, Todo]), None);
        assert_eq!(hint(Todo, &[Blocked, Done]), Some(Blocked));
        assert_eq!(hint(Blocked, &[Blocked, Done]), None);
        // Nothing to go on, or the group is already closed.
        assert_eq!(hint(Todo, &[]), None);
        assert_eq!(hint(Todo, &[Cancelled]), None);
        assert_eq!(hint(Done, &[Todo]), None);
        assert_eq!(hint(Cancelled, &[Done]), None);
    }

    #[test]
    fn two_links_to_the_same_pair_keep_the_longest_lag() {
        let h = tree();
        let both = expand_blocks(&h, &[(id(7), id(1), 1.0), (id(7), id(2), 3.0)]);
        let lag = both.iter().find(|e| e.1 == id(2)).map(|e| e.2);
        assert_eq!(lag, Some(3.0));
        assert_eq!(both.len(), 4);
    }
}
