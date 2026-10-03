//! Notes: `@[Name](node:id)` mentions, `[ ]` checklist items, list arrangement and safe
//! Markdown rendering. Pure: no IO.

use std::cmp::Ordering;
use std::collections::HashSet;

use minimap_types::{ChecklistItem, MentionRef, NodeType, NoteFilter, NoteItem, NoteRow, Uuid};
use pulldown_cmark::{html, CowStr, Event, Options, Parser, Tag, TagEnd};

/// A mention located in a body, with its byte range (`@` to the closing `)`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mention {
    pub label: String,
    pub id: Uuid,
    pub start: usize,
    pub end: usize,
}

const OPEN: &str = "@[";
const MID: &str = "](node:";

pub use minimap_types::mention_token;

/// Every well-formed `@[label](node:uuid)` in the body, in order.
pub fn parse_mentions(body: &str) -> Vec<Mention> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(rel) = body[from..].find(OPEN) {
        let start = from + rel;
        let label_start = start + OPEN.len();
        let parsed = body[label_start..].find(MID).and_then(|m| {
            let label = &body[label_start..label_start + m];
            if label.contains('\n') || label.contains('[') || label.contains(']') {
                return None;
            }
            let id_start = label_start + m + MID.len();
            let close = body[id_start..].find(')')?;
            let id = Uuid::parse_str(&body[id_start..id_start + close]).ok()?;
            Some((label.to_owned(), id, id_start + close + 1))
        });
        match parsed {
            Some((label, id, end)) => {
                out.push(Mention {
                    label,
                    id,
                    start,
                    end,
                });
                from = end;
            }
            None => from = start + OPEN.len(),
        }
    }
    out
}

/// The distinct ids mentioned, in first-seen order.
pub fn mention_ids(body: &str) -> Vec<Uuid> {
    let mut seen = HashSet::new();
    parse_mentions(body)
        .into_iter()
        .filter(|m| seen.insert(m.id))
        .map(|m| m.id)
        .collect()
}

/// `text` with every mention replaced by `f(mention)`.
fn replace_mentions(text: &str, f: impl Fn(&Mention) -> String) -> String {
    let mut out = String::new();
    let mut last = 0;
    for m in parse_mentions(text) {
        out.push_str(&text[last..m.start]);
        out.push_str(&f(&m));
        last = m.end;
    }
    out.push_str(&text[last..]);
    out
}

/// Strips a list bullet (`-`, `*`, `+`) and returns what follows, if the line is `[ ] text`.
fn unchecked_text(line: &str) -> Option<&str> {
    let mut s = line.trim_start();
    for bullet in ["- ", "* ", "+ "] {
        if let Some(rest) = s.strip_prefix(bullet) {
            s = rest.trim_start();
            break;
        }
    }
    let rest = s.strip_prefix("[ ]")?;
    let text = rest
        .strip_prefix(' ')
        .or_else(|| rest.strip_prefix('\t'))?
        .trim();
    (!text.is_empty()).then_some(text)
}

/// The unchecked `[ ]` lines (with or without a list bullet) as convertible items.
pub fn checklist(body: &str) -> Vec<ChecklistItem> {
    body.split('\n')
        .enumerate()
        .filter_map(|(i, line)| {
            let text = unchecked_text(line)?;
            let mentions = parse_mentions(text)
                .into_iter()
                .map(|m| MentionRef {
                    label: m.label,
                    id: m.id,
                })
                .collect();
            Some(ChecklistItem {
                line: u32::try_from(i).ok()?,
                text: replace_mentions(text, |m| m.label.clone()),
                mentions,
            })
        })
        .collect()
}

/// Replaces an unchecked line with `- [x] @[label](node:id)` (keeping its indentation), so the
/// converted task is mentioned and the box is ticked. `None` if the line isn't an unchecked item.
pub fn convert_line(body: &str, line: usize, label: &str, id: Uuid) -> Option<String> {
    let mut lines: Vec<String> = body.split('\n').map(str::to_owned).collect();
    let original = lines.get(line)?;
    unchecked_text(original)?;
    let indent: String = original.chars().take_while(|c| c.is_whitespace()).collect();
    lines[line] = format!("{indent}- [x] {}", mention_token(label, id));
    Some(lines.join("\n"))
}

/// One readable line of the note for lists: markup stripped, mentions as `@Name`.
pub fn excerpt(body: &str, max_chars: usize) -> String {
    let line = body
        .lines()
        .map(|l| replace_mentions(l.trim(), |m| format!("@{}", m.label)))
        .map(|l| {
            let l = l.trim_start_matches('#').trim();
            let l = ["- [ ] ", "- [x] ", "[ ] ", "[x] ", "- ", "* ", "+ ", "> "]
                .iter()
                .find_map(|p| l.strip_prefix(p))
                .unwrap_or(l);
            l.trim().to_owned()
        })
        .find(|l| !l.is_empty())
        .unwrap_or_default();
    if line.chars().count() <= max_chars {
        return line;
    }
    let cut: String = line.chars().take(max_chars).collect();
    format!("{}…", cut.trim_end())
}

/// Filters notes and orders them newest first (note date, then creation).
pub fn arrange(items: Vec<NoteItem>, filter: &NoteFilter) -> Vec<NoteRow> {
    let terms: Vec<String> = filter
        .text
        .as_deref()
        .unwrap_or("")
        .split_whitespace()
        .map(str::to_lowercase)
        .collect();
    let mut kept: Vec<NoteItem> = items
        .into_iter()
        .filter(|i| filter.kind.is_none_or(|k| i.note.kind == k))
        .filter(|i| {
            filter
                .mentions_id
                .is_none_or(|id| i.mentions.iter().any(|m| m.node.id == id))
        })
        .filter(|i| filter.date_from.is_none_or(|d| i.note.note_date >= d))
        .filter(|i| filter.date_to.is_none_or(|d| i.note.note_date <= d))
        .filter(|i| {
            if terms.is_empty() {
                return true;
            }
            let hay = format!("{} {}", i.note.title, i.note.body).to_lowercase();
            terms.iter().all(|t| hay.contains(t))
        })
        .collect();
    kept.sort_by(|a, b| -> Ordering {
        b.note
            .note_date
            .cmp(&a.note.note_date)
            .then_with(|| b.note.created_at.cmp(&a.note.created_at))
            .then_with(|| b.note.id.cmp(&a.note.id))
    });
    kept.into_iter()
        .map(|i| NoteRow {
            excerpt: excerpt(&i.note.body, 120),
            id: i.note.id,
            title: i.note.title,
            note_date: i.note.note_date,
            kind: i.note.kind,
            mentions: i.mentions,
        })
        .collect()
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

fn node_type_attr(t: NodeType) -> &'static str {
    t.as_str()
}

/// Renders Markdown to HTML that is safe to insert as-is:
/// - raw HTML in the note is shown as text, never interpreted;
/// - `@[Name](node:id)` mentions become `<a class="mention" data-node-type=… data-node-id=…>` with
///   the node's *current* name (a missing node renders as plain `@Name`);
/// - other links lose their `href` (the app never navigates away; the address stays in `title`);
/// - images become their alt text (nothing is fetched).
///
/// `resolve` looks a node up by id (type, current label), if it exists and is active.
pub fn render(body: &str, resolve: &dyn Fn(Uuid) -> Option<(NodeType, String)>) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_TASKLISTS);

    #[derive(Clone, Copy, PartialEq)]
    enum Link {
        None,
        /// A mention of a node that exists: the anchor is already written; drop the stored label.
        Found,
        /// A mention of a node that is gone: keep the stored label, wrapped in a span.
        Missing,
        /// An ordinary link: show its text, never navigate.
        Plain,
    }

    let mut events: Vec<Event> = Vec::new();
    let mut link = Link::None;
    for event in Parser::new_ext(body, options) {
        match event {
            // Raw HTML is text. Images are just their alt text (the children are text events).
            Event::Html(h) | Event::InlineHtml(h) => events.push(Event::Text(h)),
            Event::Start(Tag::Image { .. }) | Event::End(TagEnd::Image) => {}
            Event::Start(Tag::Link { dest_url, .. }) => {
                let mention = dest_url
                    .strip_prefix("node:")
                    .and_then(|i| Uuid::parse_str(i).ok());
                match mention {
                    Some(id) => match resolve(id) {
                        Some((t, label)) => {
                            link = Link::Found;
                            events.push(Event::Html(CowStr::from(format!(
                                "<a class=\"mention\" data-node-type=\"{}\" data-node-id=\"{id}\">@{}</a>",
                                node_type_attr(t),
                                escape(&label)
                            ))));
                        }
                        None => {
                            link = Link::Missing;
                            events.push(Event::Html(CowStr::from(
                                "<span class=\"mention missing\">@",
                            )));
                        }
                    },
                    None => {
                        link = Link::Plain;
                        events.push(Event::Html(CowStr::from(format!(
                            "<span class=\"link\" title=\"{}\">",
                            escape(&dest_url)
                        ))));
                    }
                }
            }
            Event::End(TagEnd::Link) => {
                if matches!(link, Link::Missing | Link::Plain) {
                    events.push(Event::Html(CowStr::from("</span>")));
                }
                link = Link::None;
            }
            _ if link == Link::Found => {}
            e => events.push(e),
        }
    }
    let mut out = String::new();
    html::push_html(&mut out, events.into_iter());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{NodeSummary, Note, NoteKind};
    use proptest::prelude::*;
    use time::{macros::date, Date, OffsetDateTime};

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    #[test]
    fn mentions_are_found_with_their_ranges() {
        let body = format!(
            "Met {} about {}.",
            mention_token("Priya", id(1)),
            mention_token("Fix login", id(2))
        );
        let found = parse_mentions(&body);
        assert_eq!(found.len(), 2);
        assert_eq!((found[0].label.as_str(), found[0].id), ("Priya", id(1)));
        assert_eq!(
            &body[found[1].start..found[1].end],
            mention_token("Fix login", id(2))
        );
        assert_eq!(
            mention_ids(&format!(
                "{0} {0} {1}",
                mention_token("A", id(1)),
                mention_token("B", id(2))
            )),
            vec![id(1), id(2)]
        );
    }

    #[test]
    fn malformed_mentions_are_ignored() {
        for body in [
            "@priya",
            "@[Priya]",
            "@[Priya](node:)",
            "@[Priya](node:not-a-uuid)",
            "@[Pri\nya](node:00000000-0000-0000-0000-000000000001)",
            "@[Pri[ya](node:00000000-0000-0000-0000-000000000001)",
            "[Priya](node:00000000-0000-0000-0000-000000000001)",
            "@[Priya](http://x.io)",
        ] {
            assert!(parse_mentions(body).is_empty(), "{body:?}");
        }
        // A bad one doesn't hide a good one after it.
        let body = format!("@[x](node:zzz) then {}", mention_token("Ok", id(7)));
        assert_eq!(mention_ids(&body), vec![id(7)]);
    }

    #[test]
    fn tokens_clean_their_labels() {
        assert_eq!(
            mention_token("A [b]\nc", id(1)),
            format!("@[A b c](node:{})", id(1))
        );
        assert_eq!(
            mention_token("  ", id(1)),
            format!("@[link](node:{})", id(1))
        );
    }

    #[test]
    fn checklist_finds_unchecked_lines() {
        let body = format!(
            "# 1:1\n[ ] Send budget to {}\n- [ ] Book room\n  * [ ]  Nested item \n- [x] Done thing\n[ ]\n[ ]nospace\ntext [ ] inside",
            mention_token("Priya", id(1))
        );
        let items = checklist(&body);
        let shown: Vec<(u32, &str)> = items.iter().map(|i| (i.line, i.text.as_str())).collect();
        assert_eq!(
            shown,
            vec![
                (1, "Send budget to Priya"),
                (2, "Book room"),
                (3, "Nested item")
            ]
        );
        assert_eq!(
            items[0].mentions,
            vec![MentionRef {
                label: "Priya".into(),
                id: id(1)
            }]
        );
    }

    #[test]
    fn converting_a_line_ticks_it_and_links_the_task() {
        let body = "intro\n  - [ ] Send budget\nother";
        let out = convert_line(body, 1, "Send budget", id(9)).unwrap();
        assert_eq!(
            out,
            format!("intro\n  - [x] @[Send budget](node:{})\nother", id(9))
        );
        assert!(
            checklist(&out).is_empty(),
            "a converted line is no longer an open item"
        );
        assert_eq!(mention_ids(&out), vec![id(9)]);
        // Lines that aren't unchecked items, or don't exist, are refused.
        assert!(convert_line(body, 0, "x", id(1)).is_none());
        assert!(convert_line(body, 7, "x", id(1)).is_none());
        assert!(convert_line(&out, 1, "x", id(1)).is_none());
    }

    #[test]
    fn excerpts_are_one_clean_line() {
        let body = format!(
            "\n\n## Weekly sync with {}\n- more",
            mention_token("Priya", id(1))
        );
        assert_eq!(excerpt(&body, 120), "Weekly sync with @Priya");
        assert_eq!(excerpt("- [ ] do the thing", 120), "do the thing");
        assert_eq!(excerpt("", 10), "");
        assert_eq!(excerpt("abcdefghijkl", 5), "abcde…");
    }

    fn note(n: u128, title: &str, body: &str, kind: NoteKind, d: Date, created: i64) -> NoteItem {
        NoteItem {
            note: Note {
                id: id(n),
                title: title.into(),
                body: body.into(),
                note_date: d,
                kind,
                created_at: OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(created),
                updated_at: OffsetDateTime::UNIX_EPOCH,
                archived_at: None,
            },
            mentions: if n % 2 == 1 {
                vec![NodeSummary {
                    node: minimap_types::NodeRef::new(NodeType::Person, id(500)),
                    label: "Priya".into(),
                    archived: false,
                }]
            } else {
                vec![]
            },
        }
    }

    #[test]
    fn notes_are_newest_first_and_filterable() {
        let items = vec![
            note(
                1,
                "1:1 Priya",
                "budget talk",
                NoteKind::OneOnOne,
                date!(2027 - 03 - 01),
                1,
            ),
            note(
                2,
                "Board prep",
                "slides",
                NoteKind::Meeting,
                date!(2027 - 03 - 10),
                2,
            ),
            note(
                3,
                "Ideas",
                "Budget ideas",
                NoteKind::General,
                date!(2027 - 03 - 10),
                3,
            ),
            note(5, "Old", "x", NoteKind::OneOnOne, date!(2026 - 12 - 31), 4),
        ];
        let titles = |f: &NoteFilter| -> Vec<String> {
            arrange(items.clone(), f)
                .into_iter()
                .map(|r| r.title)
                .collect()
        };
        // Same date: the later-created note first.
        assert_eq!(
            titles(&NoteFilter::default()),
            vec!["Ideas", "Board prep", "1:1 Priya", "Old"]
        );
        assert_eq!(
            titles(&NoteFilter {
                kind: Some(NoteKind::OneOnOne),
                ..Default::default()
            }),
            vec!["1:1 Priya", "Old"]
        );
        assert_eq!(
            titles(&NoteFilter {
                mentions_id: Some(id(500)),
                ..Default::default()
            }),
            vec!["Ideas", "1:1 Priya", "Old"]
        );
        let range = NoteFilter {
            date_from: Some(date!(2027 - 03 - 01)),
            date_to: Some(date!(2027 - 03 - 10)),
            ..Default::default()
        };
        assert_eq!(titles(&range), vec!["Ideas", "Board prep", "1:1 Priya"]);
        assert_eq!(
            titles(&NoteFilter {
                text: Some("BUDGET".into()),
                ..Default::default()
            }),
            vec!["Ideas", "1:1 Priya"]
        );
        assert_eq!(
            titles(&NoteFilter {
                text: Some("budget priya".into()),
                ..Default::default()
            }),
            vec!["1:1 Priya"]
        );
    }

    fn resolver<'a>(
        found: &'a [(u128, NodeType, &'static str)],
    ) -> impl Fn(Uuid) -> Option<(NodeType, String)> + 'a {
        move |u| {
            found
                .iter()
                .find(|(n, _, _)| id(*n) == u)
                .map(|(_, t, l)| (*t, (*l).to_owned()))
        }
    }

    #[test]
    fn rendering_is_markdown_without_raw_html() {
        let none = |_: Uuid| None;
        let html = render(
            "# Title\n\n- one\n- **two**\n\n[ ] open\n\n- [x] done\n\n`code`",
            &none,
        );
        assert!(
            html.contains("<h1>Title</h1>")
                && html.contains("<strong>two</strong>")
                && html.contains("<code>code</code>")
        );
        assert!(html.contains("type=\"checkbox\"") && html.contains("disabled"));
        // Raw HTML and script are text.
        let evil = render(
            "<script>alert(1)</script> <img src=x onerror=alert(1)> <b>x</b>",
            &none,
        );
        assert!(
            !evil.contains("<script") && !evil.contains("<img") && !evil.contains("<b>"),
            "{evil}"
        );
        assert!(evil.contains("&lt;script&gt;"));
    }

    #[test]
    fn rendering_never_produces_navigable_links_or_images() {
        let none = |_: Uuid| None;
        let html = render(
            "[click](javascript:alert(1)) [site](https://example.com) ![logo](https://evil/x.png)",
            &none,
        );
        assert!(!html.contains("href"), "{html}");
        assert!(!html.contains("<img") && !html.contains("src="), "{html}");
        assert!(html.contains("click") && html.contains("site") && html.contains("logo"));
        assert!(html.contains("title=\"https://example.com\""));
    }

    #[test]
    fn mentions_render_with_the_current_name() {
        let body = format!(
            "Ask {} about {}",
            mention_token("Old Name", id(1)),
            mention_token("Gone", id(2))
        );
        let found = [(1u128, NodeType::Person, "Priya <Shah>")];
        let html = render(&body, &resolver(&found));
        assert!(html.contains("class=\"mention\""), "{html}");
        assert!(html.contains("data-node-type=\"person\""));
        assert!(html.contains(&format!("data-node-id=\"{}\"", id(1))));
        assert!(
            html.contains("@Priya &lt;Shah&gt;"),
            "renamed and escaped: {html}"
        );
        assert!(!html.contains("Old Name"));
        // A node that no longer exists keeps its stored label as plain text.
        assert!(
            html.contains("mention missing") && html.contains("Gone"),
            "{html}"
        );
        assert!(!html.contains(&format!("data-node-id=\"{}\"", id(2))));
    }

    proptest! {
        #[test]
        fn parsing_never_panics_and_ranges_are_valid(body in "(?s).{0,200}") {
            for m in parse_mentions(&body) {
                prop_assert!(m.start < m.end && m.end <= body.len());
                prop_assert!(body.is_char_boundary(m.start) && body.is_char_boundary(m.end));
                prop_assert!(body[m.start..m.end].starts_with("@["));
            }
            let _ = checklist(&body);
            let _ = excerpt(&body, 40);
        }

        /// A token always parses back to the same id and a cleaned label.
        #[test]
        fn tokens_round_trip(label in "[^\n\r]{0,30}", n in any::<u128>()) {
            let token = mention_token(&label, id(n));
            let found = parse_mentions(&format!("x {token} y"));
            prop_assert_eq!(found.len(), 1);
            prop_assert_eq!(found[0].id, id(n));
            prop_assert!(!found[0].label.contains('[') && !found[0].label.contains(']'));
        }

        /// Rendering any text never lets raw markup through.
        #[test]
        fn rendering_never_emits_script_tags(body in "(?s).{0,200}") {
            let html = render(&body, &|_| None);
            prop_assert!(!html.contains("<script"));
        }
    }
}
