//! Sub-tasks (spec 33, ADR-0017): a task is *part of* one other task. This is organisation, not
//! sequencing: a `subtask_of` link never moves a date, never blocks anything and is invisible to
//! the schedule, the dependency graph, impact analysis and capacity (those read `blocks` only).
//!
//! The shape is a tree one level deep: a task has at most one parent, a parent is not itself a
//! sub-task, and a sub-task has no sub-tasks. Links are `(child, parent)` pairs.

use minimap_types::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubtaskError {
    /// The child is already a sub-task of this task.
    AlreadyHasParent(Uuid),
    /// The would-be parent is itself a sub-task of this task.
    ParentIsSubtask(Uuid),
    /// The would-be child has sub-tasks of its own.
    ChildHasSubtasks,
}

impl SubtaskError {
    /// The sentence for the user. `other` is the name of the task the error mentions (the
    /// existing parent, or the parent's parent).
    pub fn message(&self, child: &str, parent: &str, other: &str) -> String {
        match self {
            Self::AlreadyHasParent(_) => format!(
                "“{child}” is already part of “{other}”. A task is part of one task: remove that link first."
            ),
            Self::ParentIsSubtask(_) => format!(
                "“{parent}” is itself part of “{other}”. Sub-tasks go one level deep."
            ),
            Self::ChildHasSubtasks => format!(
                "“{child}” has sub-tasks of its own. Sub-tasks go one level deep."
            ),
        }
    }
}

/// May `child` become a sub-task of `parent`, given the existing `(child, parent)` links? A link
/// that already exists passes (the store reports the duplicate).
pub fn check_new(existing: &[(Uuid, Uuid)], child: Uuid, parent: Uuid) -> Result<(), SubtaskError> {
    if existing.contains(&(child, parent)) {
        return Ok(());
    }
    if let Some((_, grand)) = existing.iter().find(|(c, _)| *c == parent) {
        return Err(SubtaskError::ParentIsSubtask(*grand));
    }
    if existing.iter().any(|(_, p)| *p == child) {
        return Err(SubtaskError::ChildHasSubtasks);
    }
    if let Some((_, current)) = existing.iter().find(|(c, _)| *c == child) {
        return Err(SubtaskError::AlreadyHasParent(*current));
    }
    Ok(())
}

/// Sub-tasks done out of sub-tasks counted, for a parent's progress. Cancelled sub-tasks do not
/// count either way; `(done, total)`.
pub fn progress(statuses: impl IntoIterator<Item = minimap_types::TaskStatus>) -> (u32, u32) {
    use minimap_types::TaskStatus::*;
    statuses
        .into_iter()
        .fold((0, 0), |(done, total), s| match s {
            Cancelled => (done, total),
            Done => (done + 1, total + 1),
            _ => (done, total + 1),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::TaskStatus;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    #[test]
    fn a_task_can_join_a_free_parent() {
        assert_eq!(check_new(&[], id(1), id(2)), Ok(()));
        // Other families do not matter.
        assert_eq!(check_new(&[(id(3), id(4))], id(1), id(2)), Ok(()));
        // A second sub-task of the same parent is fine.
        assert_eq!(check_new(&[(id(1), id(9))], id(2), id(9)), Ok(()));
    }

    #[test]
    fn a_task_belongs_to_one_parent() {
        assert_eq!(
            check_new(&[(id(1), id(5))], id(1), id(2)),
            Err(SubtaskError::AlreadyHasParent(id(5)))
        );
        // Linking the same pair again is left to the store's duplicate check.
        assert_eq!(check_new(&[(id(1), id(2))], id(1), id(2)), Ok(()));
    }

    #[test]
    fn the_tree_is_one_level_deep() {
        // 2 is a sub-task of 5, so it cannot be a parent.
        assert_eq!(
            check_new(&[(id(2), id(5))], id(1), id(2)),
            Err(SubtaskError::ParentIsSubtask(id(5)))
        );
        // 1 is a parent, so it cannot be a sub-task.
        assert_eq!(
            check_new(&[(id(7), id(1))], id(1), id(2)),
            Err(SubtaskError::ChildHasSubtasks)
        );
        // Two tasks cannot be each other's parent either way round.
        assert!(check_new(&[(id(1), id(2))], id(2), id(1)).is_err());
    }

    #[test]
    fn no_accepted_link_ever_makes_a_loop_or_a_second_level() {
        // Greedily accept every pair from a small set; the accepted links must stay a one-level tree.
        let mut accepted: Vec<(Uuid, Uuid)> = Vec::new();
        for c in 1..=5u128 {
            for p in 1..=5u128 {
                if c != p && check_new(&accepted, id(c), id(p)).is_ok() {
                    accepted.push((id(c), id(p)));
                }
            }
        }
        for (c, p) in &accepted {
            assert!(!accepted.iter().any(|(c2, _)| c2 == p), "{c} -> {p} nests");
            assert_eq!(accepted.iter().filter(|(c2, _)| c2 == c).count(), 1);
        }
    }

    #[test]
    fn messages_name_the_tasks() {
        let m = SubtaskError::AlreadyHasParent(id(1)).message("Fix", "Epic", "Old epic");
        assert!(m.contains("“Fix”") && m.contains("“Old epic”") && m.contains("remove"));
        let m = SubtaskError::ParentIsSubtask(id(1)).message("Fix", "Epic", "Big");
        assert!(m.contains("“Epic”") && m.contains("“Big”") && m.contains("one level"));
        let m = SubtaskError::ChildHasSubtasks.message("Fix", "Epic", "");
        assert!(m.contains("“Fix”") && m.contains("one level"));
    }

    #[test]
    fn progress_counts_done_over_all_but_cancelled() {
        use TaskStatus::*;
        assert_eq!(progress([]), (0, 0));
        assert_eq!(progress([Done, Todo, InProgress, Blocked]), (1, 4));
        assert_eq!(progress([Done, Cancelled, Cancelled]), (1, 1));
        assert_eq!(progress([Cancelled]), (0, 0));
    }
}
