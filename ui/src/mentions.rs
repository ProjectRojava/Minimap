//! Pure helpers for the note editor's `@` picker. Browsers report caret positions in UTF-16
//! code units, Rust strings are UTF-8 bytes: these convert between them and find the
//! `@query` being typed.

/// A mention being typed: the `@` starts at byte `start`, the caret is at byte `caret`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MentionQuery {
    pub start: usize,
    pub caret: usize,
    /// What follows the `@` up to the caret.
    pub query: String,
}

/// UTF-16 offset (as reported by `selectionStart`) to a byte offset in `text`.
pub fn utf16_to_byte(text: &str, utf16: usize) -> usize {
    let mut units = 0;
    for (i, c) in text.char_indices() {
        if units >= utf16 {
            return i;
        }
        units += c.len_utf16();
    }
    text.len()
}

/// Byte offset to UTF-16 offset (for `setSelectionRange`).
pub fn byte_to_utf16(text: &str, byte: usize) -> usize {
    let mut end = byte.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].encode_utf16().count()
}

/// The mention being typed at `caret`, if any: an `@` at the start of the text or after
/// whitespace, followed by word characters (no spaces or brackets) up to the caret.
pub fn mention_query(text: &str, caret: usize) -> Option<MentionQuery> {
    let caret = caret.min(text.len());
    if !text.is_char_boundary(caret) {
        return None;
    }
    let before = &text[..caret];
    let at = before.rfind('@')?;
    let preceded_ok = before[..at]
        .chars()
        .next_back()
        .is_none_or(char::is_whitespace);
    let query = &before[at + 1..];
    let clean = !query
        .chars()
        .any(|c| c.is_whitespace() || matches!(c, '[' | ']' | '(' | ')' | '@'));
    (preceded_ok && clean).then(|| MentionQuery {
        start: at,
        caret,
        query: query.to_owned(),
    })
}

/// Replaces the typed `@query` with `token` and a trailing space. Returns the new text and
/// where the caret goes (a byte offset, just after the space).
pub fn insert_mention(text: &str, start: usize, caret: usize, token: &str) -> (String, usize) {
    let start = start.min(text.len());
    let caret = caret.clamp(start, text.len());
    let new = format!("{}{} {}", &text[..start], token, &text[caret..]);
    (new, start + token.len() + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caret_offsets_convert_both_ways() {
        let t = "a😀b é";
        // 😀 is 2 UTF-16 units but 4 bytes.
        assert_eq!(utf16_to_byte(t, 0), 0);
        assert_eq!(utf16_to_byte(t, 1), 1);
        assert_eq!(utf16_to_byte(t, 3), 5);
        assert_eq!(utf16_to_byte(t, 999), t.len());
        for (byte, _) in t.char_indices() {
            assert_eq!(utf16_to_byte(t, byte_to_utf16(t, byte)), byte);
        }
        assert_eq!(byte_to_utf16(t, t.len()), t.encode_utf16().count());
    }

    #[test]
    fn finds_the_mention_being_typed() {
        let q = |t: &str| mention_query(t, t.len());
        assert_eq!(
            q("@"),
            Some(MentionQuery {
                start: 0,
                caret: 1,
                query: String::new()
            })
        );
        assert_eq!(q("hello @pri").unwrap().query, "pri");
        assert_eq!(q("hello @pri").unwrap().start, 6);
        assert_eq!(q("line one\n@x").unwrap().query, "x");
        assert_eq!(q("héllo @é").unwrap().query, "é");
        // Not a mention in progress.
        for t in [
            "hello",
            "a@b",
            "@pri ",
            "@pri x",
            "see @[Priya](node:1)",
            "mail me@x.io",
            "",
        ] {
            assert_eq!(q(t), None, "{t:?}");
        }
        // The caret may be in the middle of the text; only what is before it counts.
        let t = "ask @pr and more";
        assert_eq!(mention_query(t, 7).unwrap().query, "pr");
        assert_eq!(mention_query(t, 3), None);
    }

    #[test]
    fn inserting_replaces_the_query_and_places_the_caret() {
        let t = "ask @pr and more";
        let m = mention_query(t, 7).unwrap();
        let (new, caret) = insert_mention(t, m.start, m.caret, "@[Priya](node:1)");
        assert_eq!(new, "ask @[Priya](node:1)  and more");
        assert_eq!(&new[caret..], " and more");
        // Works at the very start and end, and with non-ASCII before.
        let (new, caret) = insert_mention("@", 0, 1, "@[A](node:1)");
        assert_eq!((new.as_str(), caret), ("@[A](node:1) ", 13));
        let (new, caret) = insert_mention("é @x", 3, 5, "T");
        assert_eq!((new.as_str(), &new[caret..]), ("é T ", ""));
    }
}
