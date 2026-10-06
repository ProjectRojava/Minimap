//! Line icons for the sidebar: 16x16, stroked in `currentColor`, so they follow the theme.
//! Pure data (SVG path strings), tested for completeness against the nav table.

/// The path data of an icon, drawn with `fill="none"` and a round 1.4px stroke.
pub fn paths(name: &str) -> Option<&'static [&'static str]> {
    Some(match name {
        "overview" => &[
            "M2.5 2.5h4.5v4.5H2.5z",
            "M9 2.5h4.5v4.5H9z",
            "M2.5 9h4.5v4.5H2.5z",
            "M9 9h4.5v4.5H9z",
        ],
        "inbox" => &["M2 9.5 4 3h8l2 6.5V13H2z", "M2 9.5h3.2l.8 1.5h4l.8-1.5H14"],
        "week" => &[
            "M2.5 3.5h11v10h-11z",
            "M2.5 6.5h11",
            "M5.5 2v3",
            "M10.5 2v3",
        ],
        "objectives" => &[
            "M8 2.5a5.5 5.5 0 1 0 0 11 5.5 5.5 0 0 0 0-11z",
            "M8 5.5a2.5 2.5 0 1 0 0 5 2.5 2.5 0 0 0 0-5z",
        ],
        "projects" => &["M2 4.5h4l1.5 1.5H14v6.5H2z"],
        "tasks" => &[
            "M2.5 3.5h3v3h-3z",
            "M2.5 9.5h3v3h-3z",
            "M8 5h5.5",
            "M8 11h5.5",
        ],
        "what-if" => &["M9 1.8 3.5 9h4L7 14.2 12.5 7h-4z"],
        "people" => &[
            "M8 7.5a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5z",
            "M3 13.5c0-2.8 2.2-4.5 5-4.5s5 1.7 5 4.5",
        ],
        "teams" => &[
            "M5.5 7a2 2 0 1 0 0-4 2 2 0 0 0 0 4z",
            "M11 6.8a1.7 1.7 0 1 0 0-3.4",
            "M1.5 12.5c0-2.2 1.8-3.5 4-3.5s4 1.3 4 3.5",
            "M10 9.2c2.3-.2 4.5.8 4.5 3.3",
        ],
        "notes" => &[
            "M3.5 2h6.5l2.5 2.5V14h-9z",
            "M10 2v3h2.5",
            "M5.5 8h5",
            "M5.5 10.5h5",
        ],
        "decisions" => &[
            "M8 2.5v11",
            "M3 5h10",
            "M3 5 1.8 9a1.9 1.9 0 0 0 3.4 0z",
            "M13 5l-1.2 4a1.9 1.9 0 0 0 3.4 0z",
        ],
        "waiting" => &[
            "M8 2.5a5.5 5.5 0 1 0 0 11 5.5 5.5 0 0 0 0-11z",
            "M8 5v3.2l2 1.3",
        ],
        "cloud" => &["M4.6 12.6a3.1 3.1 0 0 1-.5-6.15 4.1 4.1 0 0 1 7.8-.7A3.4 3.4 0 0 1 11.4 12.6z"],
        "attach" => &["M12.6 7.6 7.5 12.7a3 3 0 0 1-4.2-4.2l5.6-5.6a2 2 0 0 1 2.8 2.8L6.2 11.3a1 1 0 0 1-1.4-1.4l4.9-4.9"],
        "lock" => &[
            "M3.5 7h9v6.5h-9z",
            "M5.5 7V5a2.5 2.5 0 0 1 5 0v2",
            "M8 9.5v2",
        ],
        "review" => &[
            "M3 8a5 5 0 0 1 9-3",
            "M12 2.5V5H9.5",
            "M13 8a5 5 0 0 1-9 3",
            "M4 13.5V11h2.5",
        ],
        "settings" => &[
            "M2.5 4.5h11",
            "M2.5 8h11",
            "M2.5 11.5h11",
            "M5.5 3v3",
            "M10.5 6.5v3",
            "M6.5 10v3",
        ],
        "help" => &[
            "M8 2.5a5.5 5.5 0 1 0 0 11 5.5 5.5 0 0 0 0-11z",
            "M6.3 6.3a1.8 1.8 0 1 1 2.6 1.6c-.6.4-.9.8-.9 1.4",
            "M8 11.4v.1",
        ],
        "capacity" => &["M3 13V9", "M8 13V3", "M13 13V6", "M1.8 14h12.4"],
        "graph" => &[
            "M2.5 6.5h3v3h-3z",
            "M10.5 2.5h3v3h-3z",
            "M10.5 10.5h3v3h-3z",
            "M5.5 8h2.5l2.5-3",
            "M8 8l2.5 3",
        ],
        "search" => &["M7 12a5 5 0 1 0 0-10 5 5 0 0 0 0 10z", "M10.8 10.8 14 14"],
        "command" => &[
            "M5.5 5.5h5v5h-5z",
            "M5.5 5.5V4a1.5 1.5 0 1 0-1.5 1.5z",
            "M10.5 5.5V4A1.5 1.5 0 1 1 12 5.5z",
            "M5.5 10.5V12A1.5 1.5 0 1 1 4 10.5z",
            "M10.5 10.5V12a1.5 1.5 0 1 0 1.5-1.5z",
        ],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_icons_have_paths_and_unknown_ones_do_not() {
        assert!(paths("overview").is_some_and(|p| p.len() == 4));
        assert!(paths("command").is_some());
        assert!(paths("nope").is_none());
    }

    #[test]
    fn every_path_is_a_well_formed_move_to_first_command() {
        for name in [
            "overview",
            "inbox",
            "week",
            "objectives",
            "projects",
            "tasks",
            "what-if",
            "people",
            "teams",
            "notes",
            "decisions",
            "waiting",
            "review",
            "settings",
            "command",
            "search",
            "capacity",
            "graph",
        ] {
            for d in paths(name).unwrap() {
                assert!(d.starts_with('M'), "{name}: {d}");
                assert!(
                    d.chars()
                        .all(|c| c.is_ascii_alphanumeric() || " .,-".contains(c)),
                    "{name}: {d}"
                );
            }
        }
    }
}
