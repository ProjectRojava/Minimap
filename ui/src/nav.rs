//! Navigation table and keyboard logic. Pure, so it is unit-tested natively.

use minimap_types::NodeType;

pub struct NavItem {
    pub label: &'static str,
    pub path: &'static str,
    /// Second key of the `g` chord (`g p` -> Projects).
    pub chord: char,
    /// Entries whose feature isn't built yet are hidden (flip when the screen lands).
    pub enabled: bool,
}

pub const NAV: &[NavItem] = &[
    NavItem {
        label: "Overview",
        path: "/",
        chord: 'o',
        enabled: true,
    },
    NavItem {
        label: "Inbox",
        path: "/inbox",
        chord: 'i',
        enabled: true,
    },
    NavItem {
        label: "This week",
        path: "/this-week",
        chord: 'w',
        enabled: false,
    },
    NavItem {
        label: "Objectives",
        path: "/objectives",
        chord: 'b',
        enabled: true,
    },
    NavItem {
        label: "Projects",
        path: "/projects",
        chord: 'p',
        enabled: true,
    },
    NavItem {
        label: "Tasks",
        path: "/tasks",
        chord: 't',
        enabled: true,
    },
    NavItem {
        label: "People",
        path: "/people",
        chord: 'e',
        enabled: true,
    },
    NavItem {
        label: "Teams",
        path: "/teams",
        chord: 'm',
        enabled: true,
    },
    NavItem {
        label: "Notes",
        path: "/notes",
        chord: 'n',
        enabled: true,
    },
    NavItem {
        label: "Decisions",
        path: "/decisions",
        chord: 'd',
        enabled: true,
    },
    NavItem {
        label: "Waiting on",
        path: "/waiting-on",
        chord: 'a',
        enabled: true,
    },
    NavItem {
        label: "Weekly review",
        path: "/weekly-review",
        chord: 'r',
        enabled: false,
    },
    NavItem {
        label: "Settings",
        path: "/settings",
        chord: 's',
        enabled: true,
    },
];

/// How long after `g` the second key still counts.
pub const CHORD_WINDOW_MS: f64 = 1000.0;

pub fn chord_target(key: &str) -> Option<&'static str> {
    let mut chars = key.chars();
    let c = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    NAV.iter()
        .find(|n| n.enabled && n.chord == c.to_ascii_lowercase())
        .map(|n| n.path)
}

/// List screen a node type's deep link (`/task/<id>`) falls back to.
pub fn list_path(t: NodeType) -> &'static str {
    match t {
        NodeType::Objective => "/objectives",
        NodeType::Project => "/projects",
        NodeType::Task => "/tasks",
        NodeType::Person => "/people",
        NodeType::Team => "/teams",
        NodeType::Note => "/notes",
        NodeType::Decision => "/decisions",
        NodeType::WaitingOn => "/waiting-on",
    }
}

pub fn type_label(t: NodeType) -> &'static str {
    match t {
        NodeType::Objective => "Objective",
        NodeType::Project => "Project",
        NodeType::Task => "Task",
        NodeType::Person => "Person",
        NodeType::Team => "Team",
        NodeType::Note => "Note",
        NodeType::Decision => "Decision",
        NodeType::WaitingOn => "Waiting on",
    }
}

/// Moves a list cursor by `delta`, clamped to the list. Empty list -> `None`;
/// no cursor yet -> first row (`j`) or last row (`k`).
pub fn move_cursor(cursor: Option<usize>, len: usize, delta: isize) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let next = match cursor {
        None if delta >= 0 => 0,
        None => len - 1,
        Some(c) => (c as isize + delta).clamp(0, len as isize - 1) as usize,
    };
    Some(next)
}

/// Keys must not trigger shortcuts while the user is typing.
pub fn is_typing_target(tag: &str, editable: bool) -> bool {
    editable
        || matches!(
            tag.to_ascii_uppercase().as_str(),
            "INPUT" | "TEXTAREA" | "SELECT"
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chords_are_unique_and_resolve() {
        let mut seen = std::collections::HashSet::new();
        for n in NAV {
            assert!(seen.insert(n.chord), "duplicate chord {}", n.chord);
        }
        assert_eq!(chord_target("p"), Some("/projects"));
        assert_eq!(chord_target("T"), Some("/tasks"));
        assert_eq!(chord_target("Escape"), None);
        // Hidden entries have no chord.
        assert_eq!(chord_target("w"), None);
        assert_eq!(chord_target("r"), None);
        assert_ne!(chord_target("g"), Some("/"));
    }

    #[test]
    fn cursor_clamps() {
        assert_eq!(move_cursor(None, 0, 1), None);
        assert_eq!(move_cursor(None, 3, 1), Some(0));
        assert_eq!(move_cursor(None, 3, -1), Some(2));
        assert_eq!(move_cursor(Some(0), 3, -1), Some(0));
        assert_eq!(move_cursor(Some(2), 3, 1), Some(2));
        assert_eq!(move_cursor(Some(1), 3, 1), Some(2));
        assert_eq!(move_cursor(Some(5), 3, 1), Some(2)); // list shrank under the cursor
    }

    #[test]
    fn typing_targets() {
        assert!(is_typing_target("input", false));
        assert!(is_typing_target("TEXTAREA", false));
        assert!(is_typing_target("div", true));
        assert!(!is_typing_target("a", false));
    }
}
