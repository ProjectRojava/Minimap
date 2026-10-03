//! The text that stands for a mention in a note body: `@[Name](node:<id>)`.

use uuid::Uuid;

/// Brackets and newlines can't appear in a label, so they become spaces (then whitespace is
/// collapsed); an empty label becomes "link".
pub fn mention_token(label: &str, id: Uuid) -> String {
    let clean: String = label
        .chars()
        .map(|c| {
            if matches!(c, '[' | ']' | '\n' | '\r') {
                ' '
            } else {
                c
            }
        })
        .collect();
    let clean = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    let clean = if clean.is_empty() {
        "link".to_owned()
    } else {
        clean
    };
    format!("@[{clean}](node:{id})")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_clean_their_labels() {
        let id = Uuid::from_u128(1);
        assert_eq!(mention_token("Priya", id), format!("@[Priya](node:{id})"));
        assert_eq!(
            mention_token("A [b]\nc", id),
            format!("@[A b c](node:{id})")
        );
        assert_eq!(mention_token("  ", id), format!("@[link](node:{id})"));
    }
}
