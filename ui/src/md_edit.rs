//! Pure text edits for the Markdown box's toolbar and keys (bold, lists, quote, code, link, ...).
//!
//! Every function takes the text and the selection as **byte** offsets (`start <= end`; the box
//! converts from the browser's UTF-16 offsets, see `mentions`) and returns a [`Splice`]: replace
//! `from..to` of the old text with `insert`, then select `select` (byte offsets in the *new*
//! text). Returning a splice rather than a whole new text lets the box apply it through the
//! browser's own edit command, so the text box's native undo keeps working.

/// One change to a text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Splice {
    pub from: usize,
    pub to: usize,
    pub insert: String,
    /// The selection afterwards, in the new text.
    pub select: (usize, usize),
}

impl Splice {
    /// The text after the change.
    pub fn apply(&self, text: &str) -> String {
        format!("{}{}{}", &text[..self.from], self.insert, &text[self.to..])
    }
}

/// A selection as `(start, end)` bytes.
pub type Sel = (usize, usize);

/// The kinds of line-based formatting the toolbar toggles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Block {
    Heading,
    Quote,
    Bullet,
    Numbered,
    Task,
}

fn clamp(text: &str, sel: Sel) -> Sel {
    let fix = |mut i: usize| {
        i = i.min(text.len());
        while !text.is_char_boundary(i) {
            i -= 1;
        }
        i
    };
    let (a, b) = (fix(sel.0), fix(sel.1));
    (a.min(b), a.max(b))
}

/// Wraps the selection in `before`/`after` (bold, italic, ...), or takes them off when they are
/// already there, just outside or just inside the selection. With nothing selected it inserts
/// `placeholder` between them, selected, ready to be typed over.
pub fn wrap(text: &str, sel: Sel, before: &str, after: &str, placeholder: &str) -> Splice {
    let (a, b) = clamp(text, sel);
    let selected = &text[a..b];
    if !selected.is_empty() && text[..a].ends_with(before) && text[b..].starts_with(after) {
        let from = a - before.len();
        return Splice {
            from,
            to: b + after.len(),
            insert: selected.to_owned(),
            select: (from, from + selected.len()),
        };
    }
    if selected.len() >= before.len() + after.len()
        && selected.starts_with(before)
        && selected.ends_with(after)
    {
        let inner = &selected[before.len()..selected.len() - after.len()];
        return Splice {
            from: a,
            to: b,
            insert: inner.to_owned(),
            select: (a, a + inner.len()),
        };
    }
    let body = if selected.is_empty() {
        placeholder
    } else {
        selected
    };
    Splice {
        from: a,
        to: b,
        insert: format!("{before}{body}{after}"),
        select: (a + before.len(), a + before.len() + body.len()),
    }
}

/// Code: a fenced block when the selection spans lines, otherwise inline backticks.
pub fn code(text: &str, sel: Sel) -> Splice {
    let (a, b) = clamp(text, sel);
    let selected = &text[a..b];
    if !selected.contains('\n') {
        return wrap(text, (a, b), "`", "`", "code");
    }
    if selected.starts_with("```\n") && selected.ends_with("\n```") && selected.len() >= 8 {
        let inner = &selected[4..selected.len() - 4];
        return Splice {
            from: a,
            to: b,
            insert: inner.to_owned(),
            select: (a, a + inner.len()),
        };
    }
    let lead = if a > 0 && !text[..a].ends_with('\n') {
        "\n"
    } else {
        ""
    };
    let trail = if b < text.len() && !text[b..].starts_with('\n') {
        "\n"
    } else {
        ""
    };
    let start = a + lead.len() + 4;
    Splice {
        from: a,
        to: b,
        insert: format!("{lead}```\n{selected}\n```{trail}"),
        select: (start, start + selected.len()),
    }
}

/// A link: `[text](url)` with the address selected to be typed over (or the text, when there was
/// no selection to make one of).
pub fn link(text: &str, sel: Sel) -> Splice {
    let (a, b) = clamp(text, sel);
    let selected = &text[a..b];
    if selected.is_empty() {
        let label = "link text";
        return Splice {
            from: a,
            to: b,
            insert: format!("[{label}](https://)"),
            select: (a + 1, a + 1 + label.len()),
        };
    }
    let url_at = a + 1 + selected.len() + 2;
    Splice {
        from: a,
        to: b,
        insert: format!("[{selected}](https://)"),
        select: (url_at, url_at + "https://".len()),
    }
}

/// An `@` at the caret (with a space before it when it would stick to a word), which opens the
/// mention picker.
pub fn mention(text: &str, sel: Sel) -> Splice {
    let (a, b) = clamp(text, sel);
    let sticks = text[..a]
        .chars()
        .next_back()
        .is_some_and(|c| !c.is_whitespace());
    let insert = if sticks { " @" } else { "@" };
    Splice {
        from: a,
        to: b,
        insert: insert.to_owned(),
        select: (a + insert.len(), a + insert.len()),
    }
}

/// Replaces the typed `@query` (bytes `start..caret`) with `token` and a space, caret after it.
pub fn pick_mention(text: &str, start: usize, caret: usize, token: &str) -> Splice {
    let start = start.min(text.len());
    let caret = caret.clamp(start, text.len());
    let insert = format!("{token} ");
    let end = start + insert.len();
    Splice {
        from: start,
        to: caret,
        insert,
        select: (end, end),
    }
}

/// Puts `s` on a line of its own at the caret (for an attached file's link), caret after it.
pub fn insert_on_own_line(text: &str, sel: Sel, s: &str) -> Splice {
    let (a, b) = clamp(text, sel);
    let lead = if a == 0 || text[..a].ends_with('\n') {
        ""
    } else {
        "\n"
    };
    let insert = format!("{lead}{s}\n");
    let end = a + insert.len();
    Splice {
        from: a,
        to: b,
        insert,
        select: (end, end),
    }
}

// ---- line-based blocks -------------------------------------------------------------------

fn split_indent(line: &str) -> (&str, &str) {
    let rest = line.trim_start_matches([' ', '\t']);
    (&line[..line.len() - rest.len()], rest)
}

/// The list marker at the start of `rest`: its kind and length (with the space after it).
fn list_marker(rest: &str) -> Option<(Block, usize)> {
    for m in ["- [ ] ", "- [x] ", "- [X] ", "* [ ] ", "* [x] "] {
        if rest.starts_with(m) {
            return Some((Block::Task, m.len()));
        }
    }
    for m in ["- ", "* ", "+ "] {
        if rest.starts_with(m) {
            return Some((Block::Bullet, m.len()));
        }
    }
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    (digits > 0 && rest[digits..].starts_with(". ")).then_some((Block::Numbered, digits + 2))
}

fn quote_marker(rest: &str) -> Option<usize> {
    if rest.starts_with("> ") {
        Some(2)
    } else if rest.starts_with('>') {
        Some(1)
    } else {
        None
    }
}

fn heading_marker(rest: &str) -> Option<(usize, usize)> {
    let hashes = rest.chars().take_while(|c| *c == '#').count();
    (1..=6)
        .contains(&hashes)
        .then(|| rest[hashes..].starts_with(' '))
        .filter(|ok| *ok)
        .map(|_| (hashes, hashes + 1))
}

/// Whether the line already carries the formatting of `kind` (a heading means the level the
/// button makes).
fn has(kind: Block, line: &str) -> bool {
    let (_, rest) = split_indent(line);
    match kind {
        Block::Heading => rest.starts_with("### "),
        Block::Quote => quote_marker(rest).is_some(),
        list => list_marker(rest).is_some_and(|(k, _)| k == list),
    }
}

/// The line without the marker that competes with `kind` (any list marker for a list kind, any
/// heading level for a heading, a quote for a quote).
fn stripped(kind: Block, line: &str) -> String {
    let (indent, rest) = split_indent(line);
    let cut = match kind {
        Block::Heading => heading_marker(rest).map(|(_, len)| len),
        Block::Quote => quote_marker(rest),
        _ => list_marker(rest).map(|(_, len)| len),
    };
    format!("{indent}{}", &rest[cut.unwrap_or(0)..])
}

/// The first and last byte of the lines the selection touches (a selection that ends right after
/// a newline does not touch the line after it).
fn covered_lines(text: &str, sel: Sel) -> (usize, usize) {
    let (a, b) = sel;
    let last = if b > a && text[..b].ends_with('\n') {
        b - 1
    } else {
        b
    };
    let from = text[..a].rfind('\n').map_or(0, |i| i + 1);
    let to = text[last..].find('\n').map_or(text.len(), |i| last + i);
    (from, to.max(from))
}

/// Toggles a heading, quote or list on every line the selection touches: takes it off when all
/// of them have it, otherwise puts it on (replacing a different list kind or heading level).
/// Blank lines in a selection are left alone.
pub fn block(text: &str, sel: Sel, kind: Block) -> Splice {
    let sel = clamp(text, sel);
    let (from, to) = covered_lines(text, sel);
    let lines: Vec<&str> = text[from..to].split('\n').collect();
    let several = lines.len() > 1;
    let skip = |l: &str| several && l.trim().is_empty();
    let targets: Vec<&&str> = lines.iter().filter(|l| !skip(l)).collect();
    let remove = !targets.is_empty() && targets.iter().all(|l| has(kind, l));
    let mut number = 0usize;
    let out: Vec<String> = lines
        .iter()
        .map(|line| {
            if skip(line) {
                return (*line).to_owned();
            }
            let plain = stripped(kind, line);
            if remove {
                return plain;
            }
            number += 1;
            let (indent, rest) = split_indent(&plain);
            let marker = match kind {
                Block::Heading => "### ".to_owned(),
                Block::Quote => "> ".to_owned(),
                Block::Bullet => "- ".to_owned(),
                Block::Numbered => format!("{number}. "),
                Block::Task => "- [ ] ".to_owned(),
            };
            format!("{indent}{marker}{rest}")
        })
        .collect();
    let insert = out.join("\n");
    let end = from + insert.len();
    let select = if sel.0 == sel.1 {
        (end, end)
    } else {
        (from, end)
    };
    Splice {
        from,
        to,
        insert,
        select,
    }
}

/// Enter in a list or quote: the next line starts with the same kind of marker (a task list
/// gets an unticked box, a numbered list the next number); Enter on an empty item ends the list
/// instead. `None` when the caret is not in one, so the key does what it always does.
pub fn continue_list(text: &str, caret: usize) -> Option<Splice> {
    let (caret, _) = clamp(text, (caret, caret));
    let start = text[..caret].rfind('\n').map_or(0, |i| i + 1);
    let before = &text[start..caret];
    let (indent, rest) = split_indent(before);
    let (marker, len) = if let Some((kind, len)) = list_marker(rest) {
        let next = match kind {
            Block::Task => "- [ ] ".to_owned(),
            Block::Numbered => {
                let n: usize = rest[..len - 2].parse().ok()?;
                format!("{}. ", n + 1)
            }
            _ => rest[..len].to_owned(),
        };
        (next, len)
    } else {
        ("> ".to_owned(), quote_marker(rest)?)
    };
    if rest[len..].trim().is_empty() {
        // An empty item: take the marker away and leave the caret on the empty line.
        return Some(Splice {
            from: start,
            to: caret,
            insert: String::new(),
            select: (start, start),
        });
    }
    let insert = format!("\n{indent}{marker}");
    let end = caret + insert.len();
    Some(Splice {
        from: caret,
        to: caret,
        insert,
        select: (end, end),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Applies `s` and returns the text with the selection shown as `[` and `]`.
    fn shown(text: &str, s: &Splice) -> String {
        let new = s.apply(text);
        let (a, b) = s.select;
        format!("{}[{}]{}", &new[..a], &new[a..b], &new[b..])
    }

    fn sel(text: &str, marked: &str) -> Sel {
        let a = text.find(marked).unwrap();
        (a, a + marked.len())
    }

    #[test]
    fn bold_wraps_the_selection_and_a_second_press_takes_it_off() {
        let t = "make this loud";
        let s = wrap(t, sel(t, "this"), "**", "**", "bold text");
        assert_eq!(shown(t, &s), "make **[this]** loud");
        let t2 = s.apply(t);
        // The selection is now the inner word: pressing again unwraps just outside it.
        let s2 = wrap(&t2, s.select, "**", "**", "bold text");
        assert_eq!(shown(&t2, &s2), "make [this] loud");
        // Selecting the markers too unwraps inside.
        let s3 = wrap(&t2, sel(&t2, "**this**"), "**", "**", "bold text");
        assert_eq!(shown(&t2, &s3), "make [this] loud");
    }

    #[test]
    fn with_nothing_selected_a_placeholder_is_put_in_and_selected() {
        let s = wrap("a ", (2, 2), "_", "_", "italic text");
        assert_eq!(shown("a ", &s), "a _[italic text]_");
    }

    #[test]
    fn code_is_inline_for_a_word_and_fenced_for_lines_and_toggles_back() {
        let t = "call run() now";
        assert_eq!(shown(t, &code(t, sel(t, "run()"))), "call `[run()]` now");
        let t = "a\nb\nc";
        let s = code(t, sel(t, "a\nb"));
        assert_eq!(shown(t, &s), "```\n[a\nb]\n```\nc");
        // On its own line when it sits in the middle of one.
        let t = "x a\nb";
        let s = code(t, sel(t, "a\nb"));
        assert_eq!(s.apply(t), "x \n```\na\nb\n```");
        let fenced = "```\na\nb\n```";
        assert_eq!(shown(fenced, &code(fenced, (0, fenced.len()))), "[a\nb]");
    }

    #[test]
    fn a_link_selects_the_address_or_the_label() {
        let t = "see docs here";
        assert_eq!(
            shown(t, &link(t, sel(t, "docs"))),
            "see [docs]([https://]) here"
        );
        let s = link("", (0, 0));
        assert_eq!(shown("", &s), "[[link text]](https://)");
    }

    #[test]
    fn the_mention_button_puts_an_at_sign_a_picker_can_see() {
        assert_eq!(shown("", &mention("", (0, 0))), "@[]");
        // Stuck to a word it would not open the picker, so a space goes first.
        let t = "ask";
        assert_eq!(mention(t, (3, 3)).apply(t), "ask @");
        assert_eq!(mention("ask ", (4, 4)).apply("ask "), "ask @");
    }

    #[test]
    fn picking_a_mention_replaces_the_query_and_places_the_caret() {
        let t = "ask @pr and more";
        let s = pick_mention(t, 4, 7, "@[Priya](node:1)");
        assert_eq!(s.apply(t), "ask @[Priya](node:1)  and more");
        assert_eq!(&s.apply(t)[s.select.0..], " and more");
        // At the very start, and with non-ASCII before.
        let s = pick_mention("@", 0, 1, "@[A](node:1)");
        assert_eq!(
            (s.apply("@").as_str(), s.select),
            ("@[A](node:1) ", (13, 13))
        );
        let s = pick_mention("é @x", 3, 5, "T");
        assert_eq!(s.apply("é @x"), "é T ");
    }

    #[test]
    fn an_attached_file_goes_on_its_own_line() {
        let t = "text";
        assert_eq!(
            insert_on_own_line(t, (4, 4), "![a](attachment:1)").apply(t),
            "text\n![a](attachment:1)\n"
        );
        let s = insert_on_own_line("", (0, 0), "x");
        assert_eq!(s.apply(""), "x\n");
        assert_eq!(s.select, (2, 2));
    }

    #[test]
    fn a_list_button_marks_every_touched_line_and_toggles_off() {
        let t = "one\ntwo\n\nthree";
        let s = block(t, (0, t.len()), Block::Bullet);
        assert_eq!(s.apply(t), "- one\n- two\n\n- three");
        let on = s.apply(t);
        let off = block(&on, (0, on.len()), Block::Bullet);
        assert_eq!(off.apply(&on), t);
        // Another list kind replaces the first instead of stacking.
        let numbered = block(&on, (0, on.len()), Block::Numbered);
        assert_eq!(numbered.apply(&on), "1. one\n2. two\n\n3. three");
        let tasks = block(&on, (0, on.len()), Block::Task);
        assert_eq!(tasks.apply(&on), "- [ ] one\n- [ ] two\n\n- [ ] three");
    }

    #[test]
    fn only_the_lines_the_selection_touches_change() {
        let t = "a\nb\nc";
        let s = block(t, sel(t, "b"), Block::Quote);
        assert_eq!(s.apply(t), "a\n> b\nc");
        // A selection that ends right after a newline does not take the next line.
        let s = block(t, (0, 2), Block::Quote);
        assert_eq!(s.apply(t), "> a\nb\nc");
        // The caret alone is enough, and ends up at the end of its line.
        let s = block(t, (2, 2), Block::Bullet);
        assert_eq!(shown(t, &s), "a\n- b[]\nc".to_owned());
    }

    #[test]
    fn a_heading_button_makes_a_level_three_heading_and_replaces_other_levels() {
        let t = "Title";
        let s = block(t, (0, 0), Block::Heading);
        assert_eq!(s.apply(t), "### Title");
        assert_eq!(
            block("### Title", (0, 0), Block::Heading).apply("### Title"),
            "Title"
        );
        assert_eq!(
            block("# Title", (0, 0), Block::Heading).apply("# Title"),
            "### Title"
        );
        // `#hashtag` is not a heading.
        assert_eq!(
            block("#tag", (0, 0), Block::Heading).apply("#tag"),
            "### #tag"
        );
    }

    #[test]
    fn indentation_is_kept() {
        let t = "- a\n  b";
        let s = block(t, sel(t, "  b"), Block::Bullet);
        assert_eq!(s.apply(t), "- a\n  - b");
    }

    #[test]
    fn enter_continues_a_list_and_two_enters_end_it() {
        let t = "- one";
        let s = continue_list(t, t.len()).unwrap();
        assert_eq!(s.apply(t), "- one\n- ");
        assert_eq!(s.select, (8, 8));
        // Numbered lists count on, task lists start with an empty box, quotes carry on.
        assert_eq!(
            continue_list("9. x", 4).unwrap().apply("9. x"),
            "9. x\n10. "
        );
        assert_eq!(
            continue_list("- [x] done", 10).unwrap().apply("- [x] done"),
            "- [x] done\n- [ ] "
        );
        assert_eq!(continue_list("> hi", 4).unwrap().apply("> hi"), "> hi\n> ");
        // Indented items keep their indent.
        assert_eq!(
            continue_list("  - a", 5).unwrap().apply("  - a"),
            "  - a\n  - "
        );
        // An empty item ends the list.
        let t = "- one\n- ";
        let s = continue_list(t, t.len()).unwrap();
        assert_eq!(s.apply(t), "- one\n");
        assert_eq!(s.select, (6, 6));
        // Not in a list: Enter is left alone.
        assert!(continue_list("plain", 5).is_none());
        assert!(continue_list("", 0).is_none());
        assert!(continue_list("-no space", 9).is_none());
    }

    #[test]
    fn enter_in_the_middle_of_an_item_splits_it() {
        let t = "- ab";
        let s = continue_list(t, 3).unwrap();
        assert_eq!(s.apply(t), "- a\n- b");
    }

    #[test]
    fn offsets_inside_a_character_are_moved_to_its_start() {
        let t = "é x";
        // Byte 1 is the middle of 'é'.
        let s = wrap(t, (1, 1), "*", "*", "p");
        assert_eq!(s.apply(t), "*p*é x");
    }
}
