//! A small Markdown reader for the help pages (`docs/help/*.md`). Pure, so it is tested natively.
//!
//! It reads only what the pages use, and the pages are checked against it by tests:
//! `#`/`##`/`###` headings, paragraphs, `-`/`*` and `1.` lists (one level, a line indented two
//! spaces continues the item), `|` tables, fenced code blocks, `>` call-outs, `---` rules, and
//! inline `**bold**`, `*italic*`, `` `code` `` and `[text](target)`. A target is `help:<page>`
//! (another help page) or `app:<path>` (a screen); anything else is shown as text.

/// A piece of a line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline {
    Text(String),
    Bold(String),
    Italic(String),
    Code(String),
    Link { text: String, target: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    /// `##` is level 2, `###` level 3. `id` is for "On this page" links.
    Heading {
        level: u8,
        text: String,
        id: String,
    },
    Paragraph(Vec<Inline>),
    Bullets(Vec<Vec<Inline>>),
    Numbered(Vec<Vec<Inline>>),
    Table {
        head: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
    },
    Code(String),
    /// A `>` call-out (a tip or a warning).
    Note(Vec<Inline>),
    Rule,
}

/// The page title (the first `# ` line) and the blocks after it.
pub struct Parsed {
    /// Only the tests read it (they check the file's title against the page list).
    #[allow(dead_code)]
    pub title: Option<String>,
    pub blocks: Vec<Block>,
}

/// `Quick-add & the palette` -> `quick-add-the-palette`.
pub fn slug(text: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            if dash && !out.is_empty() {
                out.push('-');
            }
            dash = false;
            out.push(c.to_ascii_lowercase());
        } else {
            dash = true;
        }
    }
    out
}

/// Reads inline formatting from one line of text.
pub fn inline(text: &str) -> Vec<Inline> {
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<Inline> = Vec::new();
    let mut plain = String::new();
    let flush = |plain: &mut String, out: &mut Vec<Inline>| {
        if !plain.is_empty() {
            out.push(Inline::Text(std::mem::take(plain)));
        }
    };
    let find = |from: usize, pat: &[char]| -> Option<usize> {
        (from..chars.len().saturating_sub(pat.len() - 1)).find(|&i| chars[i..i + pat.len()] == *pat)
    };
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        // A backslash makes the next character plain.
        if c == '\\' && i + 1 < chars.len() {
            plain.push(chars[i + 1]);
            i += 2;
            continue;
        }
        if c == '`' {
            if let Some(end) = find(i + 1, &['`']) {
                flush(&mut plain, &mut out);
                out.push(Inline::Code(chars[i + 1..end].iter().collect()));
                i = end + 1;
                continue;
            }
        }
        // `**bold**` and `*italic*`: the opening mark is followed, and the closing one preceded,
        // by something that is not a space (so "2 * 3" stays arithmetic).
        let solid = |at: usize| chars.get(at).is_some_and(|ch| !ch.is_whitespace());
        if c == '*' && chars.get(i + 1) == Some(&'*') && solid(i + 2) {
            let close = (i + 2..chars.len().saturating_sub(1))
                .find(|&e| chars[e] == '*' && chars[e + 1] == '*' && e > i + 2 && solid(e - 1));
            if let Some(end) = close {
                flush(&mut plain, &mut out);
                out.push(Inline::Bold(chars[i + 2..end].iter().collect()));
                i = end + 2;
                continue;
            }
        }
        if c == '*' && solid(i + 1) {
            let close = (i + 1..chars.len()).find(|&e| {
                chars[e] == '*' && e > i + 1 && solid(e - 1) && chars.get(e + 1) != Some(&'*')
            });
            if let Some(end) = close {
                flush(&mut plain, &mut out);
                out.push(Inline::Italic(chars[i + 1..end].iter().collect()));
                i = end + 1;
                continue;
            }
        }
        if c == '[' {
            // [text](target)
            if let Some(close) = find(i + 1, &[']']) {
                if chars.get(close + 1) == Some(&'(') {
                    if let Some(end) = find(close + 2, &[')']) {
                        flush(&mut plain, &mut out);
                        out.push(Inline::Link {
                            text: chars[i + 1..close].iter().collect(),
                            target: chars[close + 2..end].iter().collect(),
                        });
                        i = end + 1;
                        continue;
                    }
                }
            }
        }
        plain.push(c);
        i += 1;
    }
    flush(&mut plain, &mut out);
    out
}

fn is_rule(line: &str) -> bool {
    let t = line.trim();
    t.len() >= 3 && t.chars().all(|c| c == '-')
}

fn bullet_text(line: &str) -> Option<&str> {
    line.strip_prefix("- ").or_else(|| line.strip_prefix("* "))
}

fn numbered_text(line: &str) -> Option<&str> {
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    (digits > 0)
        .then(|| line[digits..].strip_prefix(". "))
        .flatten()
}

fn cells(line: &str) -> Vec<String> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    // `\|` is a bar inside a cell.
    let mut out = vec![String::new()];
    let mut chars = t.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'|') => {
                chars.next();
                if let Some(last) = out.last_mut() {
                    last.push('|');
                }
            }
            '|' => out.push(String::new()),
            c => {
                if let Some(last) = out.last_mut() {
                    last.push(c);
                }
            }
        }
    }
    out.into_iter().map(|c| c.trim().to_owned()).collect()
}

fn is_table_separator(line: &str) -> bool {
    let c = cells(line);
    !c.is_empty()
        && c.iter()
            .all(|c| !c.is_empty() && c.chars().all(|ch| matches!(ch, '-' | ':')))
}

/// Reads a whole page.
pub fn parse(md: &str) -> Parsed {
    let lines: Vec<&str> = md.lines().collect();
    let mut blocks = Vec::new();
    let mut title = None;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_end();
        if trimmed.trim().is_empty() {
            i += 1;
            continue;
        }
        // Fenced code.
        if trimmed.trim_start().starts_with("```") {
            let mut code = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                code.push(lines[i]);
                i += 1;
            }
            i += 1;
            blocks.push(Block::Code(code.join("\n")));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("# ") {
            if title.is_none() {
                title = Some(rest.trim().to_owned());
            }
            i += 1;
            continue;
        }
        let heading = [("### ", 3u8), ("## ", 2u8)]
            .iter()
            .find_map(|(prefix, level)| trimmed.strip_prefix(prefix).map(|rest| (*level, rest)));
        if let Some((level, rest)) = heading {
            let text = rest.trim().to_owned();
            blocks.push(Block::Heading {
                level,
                id: slug(&text),
                text,
            });
            i += 1;
            continue;
        }
        if is_rule(trimmed) {
            blocks.push(Block::Rule);
            i += 1;
            continue;
        }
        // Call-out.
        if trimmed.starts_with('>') {
            let mut text = Vec::new();
            while i < lines.len() && lines[i].trim_start().starts_with('>') {
                text.push(lines[i].trim_start().trim_start_matches('>').trim());
                i += 1;
            }
            blocks.push(Block::Note(inline(&text.join(" "))));
            continue;
        }
        // Table.
        if trimmed.trim_start().starts_with('|')
            && lines.get(i + 1).is_some_and(|l| is_table_separator(l))
        {
            let head: Vec<Vec<Inline>> = cells(trimmed).iter().map(|c| inline(c)).collect();
            i += 2;
            let mut rows = Vec::new();
            while i < lines.len() && lines[i].trim_start().starts_with('|') {
                rows.push(cells(lines[i]).iter().map(|c| inline(c)).collect());
                i += 1;
            }
            blocks.push(Block::Table { head, rows });
            continue;
        }
        // Lists.
        if bullet_text(trimmed).is_some() || numbered_text(trimmed).is_some() {
            let numbered = numbered_text(trimmed).is_some();
            let mut items: Vec<String> = Vec::new();
            while i < lines.len() {
                let l = lines[i].trim_end();
                let item = if numbered {
                    numbered_text(l)
                } else {
                    bullet_text(l)
                };
                if let Some(first) = item {
                    items.push(first.trim().to_owned());
                } else if l.starts_with("  ") && !l.trim().is_empty() && !items.is_empty() {
                    if let Some(last) = items.last_mut() {
                        last.push(' ');
                        last.push_str(l.trim());
                    }
                } else {
                    break;
                }
                i += 1;
            }
            let items: Vec<Vec<Inline>> = items.iter().map(|t| inline(t)).collect();
            blocks.push(if numbered {
                Block::Numbered(items)
            } else {
                Block::Bullets(items)
            });
            continue;
        }
        // Paragraph: this line, then lines up to the next blank line or block start. (The first
        // is always taken, so every pass makes progress whatever it looks like.)
        let mut text = vec![trimmed.trim()];
        i += 1;
        while i < lines.len() {
            let l = lines[i].trim_end();
            if l.trim().is_empty()
                || l.starts_with('#')
                || l.trim_start().starts_with("```")
                || l.starts_with('>')
                || l.trim_start().starts_with('|')
                || bullet_text(l).is_some()
                || numbered_text(l).is_some()
                || is_rule(l)
            {
                break;
            }
            text.push(l.trim());
            i += 1;
        }
        blocks.push(Block::Paragraph(inline(&text.join(" "))));
    }
    Parsed { title, blocks }
}

/// The text of inline pieces without formatting (for search snippets and checks).
pub fn plain(parts: &[Inline]) -> String {
    parts
        .iter()
        .map(|p| match p {
            Inline::Text(t) | Inline::Bold(t) | Inline::Italic(t) | Inline::Code(t) => t.clone(),
            Inline::Link { text, .. } => text.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> Inline {
        Inline::Text(s.to_owned())
    }

    #[test]
    fn inline_formatting() {
        assert_eq!(
            inline("a **bold** and *italic* and `code`"),
            vec![
                text("a "),
                Inline::Bold("bold".into()),
                text(" and "),
                Inline::Italic("italic".into()),
                text(" and "),
                Inline::Code("code".into()),
            ]
        );
        assert_eq!(
            inline("see [Tasks](help:tasks) now"),
            vec![
                text("see "),
                Inline::Link {
                    text: "Tasks".into(),
                    target: "help:tasks".into()
                },
                text(" now")
            ]
        );
        // Unclosed marks are plain text; a backslash makes a mark plain; code keeps its marks.
        assert_eq!(
            inline("2 * 3 and `a*b`"),
            vec![text("2 * 3 and "), Inline::Code("a*b".into())]
        );
        assert_eq!(inline("a \\* b"), vec![text("a * b")]);
        assert_eq!(inline("[not a link]"), vec![text("[not a link]")]);
        assert_eq!(inline("**"), vec![text("**")]);
        assert_eq!(inline(""), vec![]);
        assert_eq!(plain(&inline("a **b** [c](help:x)")), "a b c");
    }

    #[test]
    fn blocks() {
        let md = "# Title\n\nFirst line\nsecond line.\n\n## Part one\n\n- one\n- two\n  continues\n\n1. a\n2. b\n\n> **Tip** be kind\n\n| a | b |\n|---|---|\n| 1 | `x\\|y` |\n\n```text\ntask x\n```\n\n---\n\n### Small\n";
        let p = parse(md);
        assert_eq!(p.title.as_deref(), Some("Title"));
        assert_eq!(
            p.blocks,
            vec![
                Block::Paragraph(vec![text("First line second line.")]),
                Block::Heading {
                    level: 2,
                    text: "Part one".into(),
                    id: "part-one".into()
                },
                Block::Bullets(vec![vec![text("one")], vec![text("two continues")]]),
                Block::Numbered(vec![vec![text("a")], vec![text("b")]]),
                Block::Note(vec![Inline::Bold("Tip".into()), text(" be kind")]),
                Block::Table {
                    head: vec![vec![text("a")], vec![text("b")]],
                    rows: vec![vec![vec![text("1")], vec![Inline::Code("x|y".into())]]],
                },
                Block::Code("task x".into()),
                Block::Rule,
                Block::Heading {
                    level: 3,
                    text: "Small".into(),
                    id: "small".into()
                },
            ]
        );
    }

    #[test]
    fn a_table_needs_its_separator_row_and_a_list_ends_at_a_blank_line() {
        let p = parse("| not | a table |\n\ntext\n\n- a\n\nafter");
        assert!(matches!(&p.blocks[0], Block::Paragraph(_)));
        assert!(matches!(&p.blocks[2], Block::Bullets(items) if items.len() == 1));
        assert!(matches!(&p.blocks[3], Block::Paragraph(_)));
    }

    #[test]
    fn slugs_are_safe_ids() {
        assert_eq!(slug("Quick-add & the palette"), "quick-add-the-palette");
        assert_eq!(slug("  Hello, World! "), "hello-world");
        assert_eq!(slug("???"), "");
    }

    #[test]
    fn parsing_is_total() {
        for nasty in [
            "", "#", "# ", "```", "| |", "|--|", ">", "- ", "1.", "**", "[](", "\\", "`",
        ] {
            let _ = parse(nasty);
        }
    }
}
