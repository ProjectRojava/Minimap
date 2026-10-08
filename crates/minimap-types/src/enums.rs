//! String-backed enums. `as_str()` is the canonical form stored in SQLite and
//! always matches the serde (snake_case) representation.

use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseEnumError {
    pub kind: &'static str,
    pub value: String,
}

impl std::fmt::Display for ParseEnumError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid {}: {:?}", self.kind, self.value)
    }
}

impl std::error::Error for ParseEnumError {}

macro_rules! str_enum {
    ($(#[$m:meta])* $name:ident { $($var:ident => $s:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($var),+ }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$var),+];

            pub fn as_str(self) -> &'static str {
                match self { $($name::$var => $s),+ }
            }
        }

        impl FromStr for $name {
            type Err = ParseEnumError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    $($s => Ok($name::$var),)+
                    _ => Err(ParseEnumError { kind: stringify!($name), value: s.to_owned() }),
                }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

str_enum!(NodeType {
    Objective => "objective",
    Project => "project",
    Task => "task",
    Person => "person",
    Team => "team",
    Note => "note",
    Decision => "decision",
    WaitingOn => "waiting_on",
});

// `SubtaskOf` (child -> parent) is the organisational "part of" link between two tasks (spec 33,
// ADR-0017). It was withdrawn once (ADR-0015, when it also drove the schedule) and came back as
// a plain tree: it has no effect on dates, blocking or the schedule.
str_enum!(EdgeType {
    Blocks => "blocks",
    DependsOn => "depends_on",
    ContributesTo => "contributes_to",
    AssignedTo => "assigned_to",
    MemberOf => "member_of",
    ReportsTo => "reports_to",
    RelatesTo => "relates_to",
    Mentions => "mentions",
    Affects => "affects",
    About => "about",
    Supersedes => "supersedes",
    SubtaskOf => "subtask_of",
});

str_enum!(ObjectiveStatus {
    OnTrack => "on_track",
    AtRisk => "at_risk",
    OffTrack => "off_track",
    Done => "done",
});

str_enum!(ProjectStatus {
    Planned => "planned",
    Active => "active",
    Paused => "paused",
    Done => "done",
    Cancelled => "cancelled",
});

str_enum!(TaskStatus {
    Todo => "todo",
    InProgress => "in_progress",
    Blocked => "blocked",
    Done => "done",
    Cancelled => "cancelled",
});

str_enum!(NoteKind {
    OneOnOne => "one_on_one",
    Meeting => "meeting",
    General => "general",
});

str_enum!(DecisionStatus {
    Proposed => "proposed",
    Decided => "decided",
    Superseded => "superseded",
});

str_enum!(ActivityAction {
    Created => "created",
    Updated => "updated",
    Archived => "archived",
    Unarchived => "unarchived",
    Deleted => "deleted",
    EdgeAdded => "edge_added",
    EdgeRemoved => "edge_removed",
});

#[cfg(test)]
mod tests {
    use super::*;

    fn check<T>(all: &[T], as_str: fn(T) -> &'static str)
    where
        T: Copy + Serialize + FromStr + PartialEq + std::fmt::Debug,
        T::Err: std::fmt::Debug,
    {
        for &v in all {
            let json = serde_json::to_string(&v).unwrap();
            assert_eq!(json, format!("\"{}\"", as_str(v)));
            assert_eq!(as_str(v).parse::<T>().unwrap(), v);
        }
    }

    #[test]
    fn serde_matches_as_str() {
        check(NodeType::ALL, NodeType::as_str);
        check(EdgeType::ALL, EdgeType::as_str);
        check(ObjectiveStatus::ALL, ObjectiveStatus::as_str);
        check(ProjectStatus::ALL, ProjectStatus::as_str);
        check(TaskStatus::ALL, TaskStatus::as_str);
        check(NoteKind::ALL, NoteKind::as_str);
        check(DecisionStatus::ALL, DecisionStatus::as_str);
        check(ActivityAction::ALL, ActivityAction::as_str);
    }

    #[test]
    fn rejects_unknown() {
        assert!("nope".parse::<TaskStatus>().is_err());
    }
}
