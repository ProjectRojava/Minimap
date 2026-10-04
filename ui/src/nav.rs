//! Navigation table and keyboard logic. Pure, so it is unit-tested natively.

use minimap_types::NodeType;

pub struct NavItem {
    pub label: &'static str,
    /// Key of the sidebar icon (`icons::paths`).
    pub icon: &'static str,
    /// Section heading in the sidebar; empty = no heading, `pinned` = the footer.
    pub group: &'static str,
    pub path: &'static str,
    /// Second key of the `g` chord (`g p` -> Projects).
    pub chord: char,
    /// Entries whose feature isn't built yet are hidden (flip when the screen lands).
    pub enabled: bool,
}

pub const NAV: &[NavItem] = &[
    NavItem {
        label: "Overview",
        icon: "overview",
        group: "",
        path: "/",
        chord: 'o',
        enabled: true,
    },
    NavItem {
        label: "Inbox",
        icon: "inbox",
        group: "",
        path: "/inbox",
        chord: 'i',
        enabled: true,
    },
    NavItem {
        label: "This week",
        icon: "week",
        group: "",
        path: "/this-week",
        chord: 'w',
        enabled: false,
    },
    NavItem {
        label: "Objectives",
        icon: "objectives",
        group: "Plan",
        path: "/objectives",
        chord: 'b',
        enabled: true,
    },
    NavItem {
        label: "Projects",
        icon: "projects",
        group: "Plan",
        path: "/projects",
        chord: 'p',
        enabled: true,
    },
    NavItem {
        label: "Tasks",
        icon: "tasks",
        group: "Plan",
        path: "/tasks",
        chord: 't',
        enabled: true,
    },
    NavItem {
        label: "What if",
        icon: "what-if",
        group: "Plan",
        path: "/what-if",
        chord: 'f',
        enabled: true,
    },
    NavItem {
        label: "People",
        icon: "people",
        group: "People",
        path: "/people",
        chord: 'e',
        enabled: true,
    },
    NavItem {
        label: "Teams",
        icon: "teams",
        group: "People",
        path: "/teams",
        chord: 'm',
        enabled: true,
    },
    NavItem {
        label: "Notes",
        icon: "notes",
        group: "Log",
        path: "/notes",
        chord: 'n',
        enabled: true,
    },
    NavItem {
        label: "Decisions",
        icon: "decisions",
        group: "Log",
        path: "/decisions",
        chord: 'd',
        enabled: true,
    },
    NavItem {
        label: "Waiting on",
        icon: "waiting",
        group: "Log",
        path: "/waiting-on",
        chord: 'a',
        enabled: true,
    },
    NavItem {
        label: "Weekly review",
        icon: "review",
        group: "Review",
        path: "/weekly-review",
        chord: 'r',
        enabled: false,
    },
    NavItem {
        label: "Settings",
        icon: "settings",
        group: "pinned",
        path: "/settings",
        chord: 's',
        enabled: true,
    },
];

/// The group name of the footer entries (Settings).
pub const PINNED: &str = "pinned";

/// Visible entries as (heading, items) in order; headings that repeat consecutively are merged
/// and the pinned (footer) entries are left out.
pub fn grouped() -> Vec<(&'static str, Vec<&'static NavItem>)> {
    let mut out: Vec<(&'static str, Vec<&'static NavItem>)> = Vec::new();
    for item in NAV.iter().filter(|n| n.enabled && n.group != PINNED) {
        match out.last_mut() {
            Some((g, items)) if *g == item.group => items.push(item),
            _ => out.push((item.group, vec![item])),
        }
    }
    out
}

/// The footer entries.
pub fn pinned() -> Vec<&'static NavItem> {
    NAV.iter()
        .filter(|n| n.enabled && n.group == PINNED)
        .collect()
}

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

/// Keys that act on the row under the list cursor: `x` toggle done, `s` next status,
/// `d` due date, `a` assignee, `1`-`5` priority. Screens that don't use them ignore them.
pub fn is_row_key(key: &str) -> bool {
    matches!(key, "x" | "s" | "d" | "a" | "1" | "2" | "3" | "4" | "5")
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
    fn every_entry_has_an_icon_and_the_groups_partition_the_visible_ones() {
        for n in NAV {
            assert!(
                !n.icon.is_empty() && crate::icons::paths(n.icon).is_some(),
                "{}",
                n.label
            );
        }
        let visible = NAV.iter().filter(|n| n.enabled).count();
        let grouped: usize = grouped().iter().map(|(_, v)| v.len()).sum();
        assert_eq!(grouped + pinned().len(), visible);
        // The first group (Overview, Inbox) has no heading; Settings is pinned.
        assert_eq!(grouped_headings()[0], "");
        assert!(pinned().iter().any(|n| n.path == "/settings"));
        // A heading appears once.
        let headings = grouped_headings();
        let unique: std::collections::HashSet<_> = headings.iter().collect();
        assert_eq!(unique.len(), headings.len());
    }

    fn grouped_headings() -> Vec<&'static str> {
        grouped().into_iter().map(|(g, _)| g).collect()
    }

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
    fn row_keys() {
        for k in ["x", "s", "d", "a", "1", "5"] {
            assert!(is_row_key(k), "{k}");
        }
        for k in ["g", "j", "k", "n", "0", "6", "Enter", "xx", ""] {
            assert!(!is_row_key(k), "{k}");
        }
    }

    #[test]
    fn typing_targets() {
        assert!(is_typing_target("input", false));
        assert!(is_typing_target("TEXTAREA", false));
        assert!(is_typing_target("div", true));
        assert!(!is_typing_target("a", false));
    }
}
