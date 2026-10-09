//! The in-app help (the wiki under Help): pages written as Markdown in `docs/help/`, embedded in
//! the app, with a small search. Pure, so it is tested natively, and the tests keep the pages
//! in step with the app (every shortcut, screen and settings tab is documented, every link works).

pub mod markdown;

/// One help page.
pub struct Page {
    /// The address (`/help?page=quick-add`) and the target of `help:<id>` links.
    pub id: &'static str,
    /// The heading it is listed under.
    pub group: &'static str,
    pub title: &'static str,
    /// One line, shown under the title and in the list.
    pub summary: &'static str,
    /// Markdown (`docs/help/<id>.md`).
    pub body: &'static str,
}

macro_rules! page {
    ($id:literal, $group:literal, $title:literal, $summary:literal) => {
        Page {
            id: $id,
            group: $group,
            title: $title,
            summary: $summary,
            body: include_str!(concat!("../../../docs/help/", $id, ".md")),
        }
    };
}

/// Every page, in reading order. Pages of one group are adjacent.
pub const PAGES: &[Page] = &[
    page!(
        "welcome",
        "Start here",
        "Welcome to Minimap",
        "What it is, your first ten minutes, and a map of the screens"
    ),
    page!(
        "concepts",
        "Start here",
        "How Minimap thinks",
        "Items, links, statuses, working days, and what is computed for you"
    ),
    page!(
        "this-week",
        "Daily work",
        "This week",
        "The landing screen: what needs you now"
    ),
    page!(
        "tasks",
        "Daily work",
        "Tasks and the inbox",
        "Adding, finding and editing tasks"
    ),
    page!(
        "meetings",
        "Daily work",
        "Meetings",
        "Meetings that start and end on their own, and follow-ups"
    ),
    page!(
        "projects",
        "Daily work",
        "Projects",
        "List and board, handles, objectives, dependencies, archiving"
    ),
    page!(
        "objectives",
        "Daily work",
        "Objectives",
        "Outcomes, contributions and how health rolls up"
    ),
    page!(
        "people-teams",
        "Daily work",
        "People and teams",
        "People, capacity hours, managers, teams and 1:1s"
    ),
    page!(
        "notes",
        "Daily work",
        "Notes and @mentions",
        "Markdown notes, mentions, checklists that become tasks"
    ),
    page!(
        "decisions",
        "Daily work",
        "Decisions",
        "Record what was decided, why, and what replaced it"
    ),
    page!(
        "waiting-on",
        "Daily work",
        "Waiting on",
        "Track what you are waiting for, and chase the stale"
    ),
    page!(
        "recurring",
        "Daily work",
        "Recurring items",
        "Tasks and notes that repeat by themselves"
    ),
    page!(
        "links",
        "Daily work",
        "Links between items",
        "Blocks, depends on, assigned to and the other relations"
    ),
    page!(
        "schedule",
        "Planning and insight",
        "Schedule and critical path",
        "How finish dates, slack and the critical path are worked out"
    ),
    page!(
        "dependencies",
        "Planning and insight",
        "Dependency graph",
        "What blocks what, as a picture"
    ),
    page!(
        "what-if",
        "Planning and insight",
        "What if this slips?",
        "See what a delay would push back, then apply it"
    ),
    page!(
        "capacity",
        "Planning and insight",
        "Capacity",
        "Who is loaded when, as a heatmap"
    ),
    page!(
        "overview",
        "Planning and insight",
        "Overview and health",
        "Health, risks, and how they are decided"
    ),
    page!(
        "weekly-review",
        "Planning and insight",
        "Weekly review",
        "A guided look back that ends in a status report"
    ),
    page!(
        "quick-add",
        "Move faster",
        "Quick-add and the command palette",
        "Add things from one line of text"
    ),
    page!(
        "search",
        "Move faster",
        "Search",
        "Find anything you have written down"
    ),
    page!(
        "keyboard",
        "Move faster",
        "Keyboard shortcuts",
        "Every shortcut in one place"
    ),
    page!(
        "undo",
        "Move faster",
        "Undo and redo",
        "Take back your last changes"
    ),
    page!(
        "settings",
        "Your data",
        "Settings",
        "Every setting, tab by tab"
    ),
    page!(
        "backup-restore",
        "Your data",
        "Backup and restore",
        "Automatic and manual backups, and getting one back"
    ),
    page!(
        "encryption",
        "Your data",
        "Encryption",
        "Lock your database with the keychain or a passphrase"
    ),
    page!(
        "google-drive",
        "Your data",
        "Google Drive: backup, sync and attachments",
        "Save to your Drive and keep computers in step"
    ),
    page!(
        "export",
        "Your data",
        "Export your data",
        "Everything, in open formats"
    ),
    page!(
        "faq",
        "Help",
        "Questions and troubleshooting",
        "Common problems, and a glossary"
    ),
];

/// The page the Help screen opens on.
pub const FIRST: &str = "welcome";

pub fn find(id: &str) -> Option<&'static Page> {
    PAGES.iter().find(|p| p.id == id)
}

/// The requested page, or the first one for a missing or unknown id.
pub fn page_or_first(id: Option<&str>) -> &'static Page {
    id.and_then(find)
        .or_else(|| find(FIRST))
        .unwrap_or(&PAGES[0])
}

/// Pages under their group headings, in order.
pub fn groups() -> Vec<(&'static str, Vec<&'static Page>)> {
    let mut out: Vec<(&'static str, Vec<&'static Page>)> = Vec::new();
    for page in PAGES {
        match out.last_mut() {
            Some((g, pages)) if *g == page.group => pages.push(page),
            _ => out.push((page.group, vec![page])),
        }
    }
    out
}

/// The pages before and after `id` in reading order.
pub fn neighbours(id: &str) -> (Option<&'static Page>, Option<&'static Page>) {
    let at = PAGES.iter().position(|p| p.id == id);
    match at {
        Some(i) => (i.checked_sub(1).map(|j| &PAGES[j]), PAGES.get(i + 1)),
        None => (None, None),
    }
}

/// A page that matches a search, with a line of it to show why.
pub struct Hit {
    pub page: &'static Page,
    pub snippet: String,
}

/// The text of a Markdown line without its list or table marks and formatting.
fn plain_line(line: &str) -> String {
    let t = line.trim();
    if t.starts_with('|') {
        // A table row: its cells, joined.
        let cells: Vec<String> = t
            .split('|')
            .map(|c| markdown::plain(&markdown::inline(c.trim())))
            .filter(|c| !c.is_empty())
            .collect();
        return cells.join(" · ");
    }
    let t = t.trim_start_matches(['>', '-', '*']).trim();
    let t = t
        .split_once(". ")
        .filter(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
        .map_or(t, |(_, rest)| rest);
    markdown::plain(&markdown::inline(t))
}

fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_owned()
    } else {
        let cut: String = text.chars().take(max).collect();
        format!("{}…", cut.trim_end())
    }
}

/// Pages containing every word of `query` (any order, case-insensitive), pages whose title has
/// the words first, then reading order. An empty query matches nothing.
pub fn search(query: &str) -> Vec<Hit> {
    let words: Vec<String> = query
        .split_whitespace()
        .take(6)
        .map(str::to_lowercase)
        .collect();
    if words.is_empty() {
        return Vec::new();
    }
    let mut hits: Vec<(usize, usize, Hit)> = Vec::new();
    for (order, page) in PAGES.iter().enumerate() {
        let title = page.title.to_lowercase();
        let body = format!("{}\n{}", page.summary, page.body).to_lowercase();
        if !words
            .iter()
            .all(|w| title.contains(w.as_str()) || body.contains(w.as_str()))
        {
            continue;
        }
        let in_title = words.iter().filter(|w| title.contains(w.as_str())).count();
        let first = &words[0];
        let snippet = page
            .body
            .lines()
            .filter(|l| !l.trim_start().starts_with('#') && !l.trim_start().starts_with("```"))
            .find(|l| l.to_lowercase().contains(first.as_str()))
            .map(plain_line)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| page.summary.to_owned());
        hits.push((
            usize::MAX - in_title,
            order,
            Hit {
                page,
                snippet: clip(&snippet, 140),
            },
        ));
    }
    hits.sort_by_key(|(title_rank, order, _)| (*title_rank, *order));
    hits.into_iter().map(|(_, _, h)| h).collect()
}

#[cfg(test)]
mod tests {
    use super::markdown::{parse, plain, Block, Inline};
    use super::*;
    use crate::{nav::NAV, settings_tab::Tab};
    use std::collections::HashSet;

    fn page(id: &str) -> &'static Page {
        find(id).unwrap_or_else(|| panic!("no help page {id}"))
    }

    fn all_inlines(blocks: &[Block]) -> Vec<&Inline> {
        let mut out: Vec<&Inline> = Vec::new();
        for b in blocks {
            match b {
                Block::Paragraph(p) | Block::Note(p) => out.extend(p),
                Block::Bullets(items) | Block::Numbered(items) => {
                    out.extend(items.iter().flatten())
                }
                Block::Table { head, rows } => {
                    out.extend(head.iter().flatten());
                    out.extend(rows.iter().flatten().flatten());
                }
                _ => {}
            }
        }
        out
    }

    #[test]
    fn pages_have_unique_addresses_titles_and_content() {
        let mut ids = HashSet::new();
        let mut titles = HashSet::new();
        for p in PAGES {
            assert!(ids.insert(p.id), "duplicate id {}", p.id);
            assert!(titles.insert(p.title), "duplicate title {}", p.title);
            assert!(
                p.id.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "{} is not an address",
                p.id
            );
            assert!(
                p.body.len() > 600,
                "{} is thin ({} bytes)",
                p.id,
                p.body.len()
            );
            assert!(!p.summary.is_empty() && p.summary.len() < 100, "{}", p.id);
        }
        assert!(find(FIRST).is_some());
        assert_eq!(PAGES.len(), 29);
    }

    #[test]
    fn the_title_in_the_file_is_the_title_in_the_list() {
        for p in PAGES {
            assert_eq!(parse(p.body).title.as_deref(), Some(p.title), "{}", p.id);
        }
    }

    #[test]
    fn groups_keep_their_pages_together_and_in_order() {
        let g = groups();
        let names: Vec<&str> = g.iter().map(|(n, _)| *n).collect();
        assert_eq!(
            names,
            [
                "Start here",
                "Daily work",
                "Planning and insight",
                "Move faster",
                "Your data",
                "Help"
            ]
        );
        assert_eq!(g.iter().map(|(_, p)| p.len()).sum::<usize>(), PAGES.len());
    }

    #[test]
    fn unknown_pages_open_the_first_and_neighbours_walk_the_order() {
        assert_eq!(page_or_first(None).id, FIRST);
        assert_eq!(page_or_first(Some("nope")).id, FIRST);
        assert_eq!(page_or_first(Some("tasks")).id, "tasks");
        assert_eq!(neighbours("welcome").0.map(|p| p.id), None);
        assert_eq!(neighbours("welcome").1.map(|p| p.id), Some("concepts"));
        assert_eq!(neighbours("faq").1.map(|p| p.id), None);
        assert_eq!(neighbours("tasks").0.map(|p| p.id), Some("this-week"));
        assert_eq!(neighbours("tasks").1.map(|p| p.id), Some("meetings"));
        let (before, after) = neighbours("zzz");
        assert!(before.is_none() && after.is_none());
    }

    /// Every route a help link may point at.
    fn routes() -> HashSet<String> {
        let mut r: HashSet<String> = NAV.iter().map(|n| n.path.to_owned()).collect();
        r.extend(Tab::visible().iter().map(|t| t.path()));
        r.insert("/help".to_owned());
        r
    }

    #[test]
    fn every_link_goes_somewhere_and_none_leaves_the_app() {
        let routes = routes();
        for p in PAGES {
            let parsed = parse(p.body);
            for inline in all_inlines(&parsed.blocks) {
                if let Inline::Link { text, target } = inline {
                    assert!(!text.is_empty(), "{}: a link with no text", p.id);
                    if let Some(id) = target.strip_prefix("help:") {
                        assert!(find(id).is_some(), "{}: help:{id} does not exist", p.id);
                    } else if let Some(path) = target.strip_prefix("app:") {
                        assert!(
                            routes.contains(path),
                            "{}: app:{path} is not a screen",
                            p.id
                        );
                    } else {
                        panic!(
                            "{}: link [{text}]({target}) is neither help: nor app:",
                            p.id
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn every_page_can_be_reached_from_another() {
        let mut linked: HashSet<&str> = HashSet::new();
        for p in PAGES {
            let parsed = parse(p.body);
            for inline in all_inlines(&parsed.blocks) {
                if let Inline::Link { target, .. } = inline {
                    if let Some(id) = target.strip_prefix("help:") {
                        if id != p.id {
                            linked.insert(find(id).map_or("", |q| q.id));
                        }
                    }
                }
            }
        }
        for p in PAGES.iter().filter(|p| p.id != FIRST) {
            assert!(linked.contains(p.id), "nothing links to {}", p.id);
        }
    }

    #[test]
    fn formatting_is_understood_everywhere() {
        for p in PAGES {
            let parsed = parse(p.body);
            assert!(!parsed.blocks.is_empty(), "{}", p.id);
            for inline in all_inlines(&parsed.blocks) {
                if let Inline::Text(t) = inline {
                    // Leftover marks mean a typo in the page (an unclosed ** or `).
                    assert!(
                        !t.contains("**") && !t.contains('`'),
                        "{}: stray mark in {t:?}",
                        p.id
                    );
                    assert!(!t.contains("]("), "{}: broken link in {t:?}", p.id);
                    assert!(
                        !t.contains("<br") && !t.contains("<p>"),
                        "{}: html in {t:?}",
                        p.id
                    );
                }
            }
            for block in &parsed.blocks {
                if let Block::Table { head, rows } = block {
                    for row in rows {
                        assert_eq!(
                            row.len(),
                            head.len(),
                            "{}: a table row has the wrong width",
                            p.id
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn every_shortcut_chord_and_screen_is_documented() {
        let keyboard = page("keyboard").body;
        let welcome = page("welcome").body;
        for n in NAV.iter().filter(|n| n.enabled) {
            assert!(
                keyboard.contains(&format!("`g {}`", n.chord)),
                "the keyboard page lacks `g {}` ({})",
                n.chord,
                n.label
            );
            // Each chord is listed with the right screen on the same line.
            let line = keyboard
                .lines()
                .find(|l| l.contains(&format!("`g {}`", n.chord)))
                .unwrap_or_default();
            assert!(
                line.contains(n.label),
                "`g {}` should say {}: {line}",
                n.chord,
                n.label
            );
            assert!(
                welcome.contains(n.label) || n.group == crate::nav::PINNED,
                "the welcome page's map lacks {}",
                n.label
            );
        }
    }

    #[test]
    fn every_settings_tab_and_theme_count_is_documented() {
        let settings = page("settings").body;
        for tab in Tab::visible() {
            let heading = format!("## {}", tab.label().replace(" & backup", ""));
            assert!(
                settings.contains(&heading) || settings.contains(&format!("## {}", tab.label())),
                "the settings page lacks the {} tab",
                tab.label()
            );
        }
        assert!(settings.contains(&format!("{} themes", crate::themes::ALL.len())));
    }

    #[test]
    fn the_report_placeholders_are_all_listed() {
        let review = page("weekly-review").body;
        for (name, _) in minimap_types::REPORT_PLACEHOLDERS {
            assert!(
                review.contains(&format!("{{{{{name}}}}}")),
                "{name} is not documented"
            );
        }
    }

    #[test]
    fn the_settings_defaults_quoted_are_the_real_ones() {
        let defaults = minimap_types::Settings::default();
        let settings = page("settings").body;
        assert!(settings.contains(&format!("default {}", defaults.hours_per_day)));
        assert!(settings.contains(&format!(
            "default {} hours",
            defaults.default_weekly_capacity_hours
        )));
        assert!(settings.contains(&format!("(default {})", defaults.stale_waiting_days)));
        assert!(settings.contains(&format!("(default {})", defaults.capacity_task_limit)));
        let t = defaults.health;
        let overview = page("overview").body;
        assert!(overview.contains(&format!(
            "{} working day late | {} working days late",
            t.late_amber_days, t.late_red_days
        )));
        assert!(overview.contains(&format!("{}% | {}%", t.risky_amber_pct, t.risky_red_pct)));
        assert!(overview.contains(&format!(
            "{}% | {}%",
            t.unestimated_amber_pct, t.unestimated_red_pct
        )));
    }

    #[test]
    fn search_finds_pages_by_any_words_and_ranks_titles_first() {
        let ids = |q: &str| search(q).iter().map(|h| h.page.id).collect::<Vec<_>>();
        assert!(ids("   ").is_empty());
        assert!(ids("xyzzyplugh").is_empty());
        // A title word comes first.
        assert_eq!(ids("capacity").first(), Some(&"capacity"));
        assert_eq!(ids("undo").first(), Some(&"undo"));
        // Words in any order, case-insensitively, each anywhere in the page.
        assert!(ids("CRITICAL path").contains(&"schedule"));
        assert!(ids("recovery key").contains(&"encryption"));
        assert!(ids("recovery key").contains(&"google-drive"));
        // More words narrow it.
        assert!(ids("recovery key drive").len() <= ids("recovery key").len());
        // Hits carry a readable line, never raw marks.
        for hit in search("snooze") {
            assert!(!hit.snippet.is_empty());
            assert!(
                !hit.snippet.contains("**") && !hit.snippet.contains('`'),
                "{}",
                hit.snippet
            );
            assert!(hit.snippet.chars().count() <= 142);
        }
    }

    #[test]
    fn a_snippet_is_the_line_that_matched() {
        let hits = search("lag");
        let links = hits
            .iter()
            .find(|h| h.page.id == "links")
            .expect("links page");
        assert!(
            links.snippet.to_lowercase().contains("lag"),
            "{}",
            links.snippet
        );
        // The plain-line reader drops list, table and formatting marks.
        assert_eq!(plain_line("- **Bold** and `code`"), "Bold and code");
        assert_eq!(plain_line("1. First [step](help:tasks)"), "First step");
        assert_eq!(plain_line("| a | b |"), "a · b");
        assert_eq!(plain_line("> **Tip:** hi"), "Tip: hi");
        assert_eq!(plain(&markdown::inline("x")), "x");
    }
}
