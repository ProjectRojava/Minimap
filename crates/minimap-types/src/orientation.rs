//! Portfolio view and My view (docs/product-design/07-orientation.md, spec 50): the same map seen
//! north-up (everything) or player-up (everything as it touches you). The view is a way of
//! looking, kept per device and never stored in the database.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Which map to show.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum View {
    /// The whole organisation.
    #[default]
    Portfolio,
    /// Everything as it touches you.
    Mine,
}

impl View {
    pub fn toggled(self) -> Self {
        match self {
            View::Portfolio => View::Mine,
            View::Mine => View::Portfolio,
        }
    }

    /// The word stored on this device and shown on the toggle.
    pub fn as_str(self) -> &'static str {
        match self {
            View::Portfolio => "portfolio",
            View::Mine => "mine",
        }
    }

    pub fn from_str_or_default(text: &str) -> Self {
        match text {
            "mine" => View::Mine,
            _ => View::Portfolio,
        }
    }
}

/// Why something is in My view. An item may have several, in this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Why {
    /// A task assigned to you.
    Assigned,
    /// A project you own.
    Owner,
    /// A task in a project you own.
    InMyProject,
    /// Upstream of your work: a task that (through up to four `blocks` links) blocks the task
    /// `via`, or a project your project `via` depends on.
    Reaches { via: Uuid },
    /// Downstream of your work, one hop: blocked by your task `via`.
    Blocks { via: Uuid },
    /// An objective your projects or tasks contribute to.
    Serves,
    /// A person who reports to you.
    Report,
    /// A person assigned to tasks in projects you own.
    OnMyProjects,
    /// You.
    Me,
}

/// Everything in My view, and why. An item that is only here because it holds in-scope work (a
/// project with one of your tasks) has an empty list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MyScope {
    pub tasks: BTreeMap<Uuid, Vec<Why>>,
    pub projects: BTreeMap<Uuid, Vec<Why>>,
    pub objectives: BTreeMap<Uuid, Vec<Why>>,
    pub people: BTreeMap<Uuid, Vec<Why>>,
    /// How many of your own tasks (assigned to you, or in a project you own) each project holds:
    /// what a project with no reason of its own says ("3 of your tasks").
    #[serde(default)]
    pub project_tasks: BTreeMap<Uuid, u32>,
}

impl MyScope {
    /// No "me", so nothing is in My view.
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
            && self.projects.is_empty()
            && self.objectives.is_empty()
            && self.people.is_empty()
    }

    /// The first reason an item is here, if it has one.
    pub fn first_why(list: &BTreeMap<Uuid, Vec<Why>>, id: Uuid) -> Option<Why> {
        list.get(&id).and_then(|w| w.first().copied())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_view_toggles_and_reads_back_from_its_word() {
        assert_eq!(View::default(), View::Portfolio);
        assert_eq!(View::Portfolio.toggled(), View::Mine);
        assert_eq!(View::Mine.toggled(), View::Portfolio);
        for v in [View::Portfolio, View::Mine] {
            assert_eq!(View::from_str_or_default(v.as_str()), v);
            assert_eq!(
                serde_json::to_string(&v).unwrap(),
                format!("\"{}\"", v.as_str())
            );
        }
        assert_eq!(View::from_str_or_default("nonsense"), View::Portfolio);
    }

    #[test]
    fn a_scope_survives_json_with_uuid_keys() {
        let a = Uuid::from_u128(1);
        let scope = MyScope {
            tasks: BTreeMap::from([(a, vec![Why::Assigned, Why::Reaches { via: a }])]),
            projects: BTreeMap::from([(a, Vec::new())]),
            ..Default::default()
        };
        let json = serde_json::to_string(&scope).unwrap();
        let back: MyScope = serde_json::from_str(&json).unwrap();
        assert_eq!(back, scope);
        assert!(json.contains("\"kind\":\"reaches\""));
        assert!(!scope.is_empty());
        assert!(MyScope::default().is_empty());
        assert_eq!(MyScope::first_why(&scope.tasks, a), Some(Why::Assigned));
        assert_eq!(MyScope::first_why(&scope.projects, a), None);
    }
}
