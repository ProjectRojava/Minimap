//! Search query planning: words, prefix matching and typo tolerance. Pure; the store runs the
//! expressions this builds against its FTS5 index.
//!
//! Every query word is a prefix (`fix log` finds "Fix login timeout"). A word that matches
//! nothing exactly can also match indexed terms a typo away (`loign` finds "login").

/// Words beyond this many are ignored.
pub const MAX_WORDS: usize = 8;
/// Typo-tolerant alternatives kept per word.
const MAX_ALTERNATIVES: usize = 8;

/// The query's words, lowercased, split on anything that isn't a letter or digit (the same
/// rule the index uses), without repeats.
pub fn tokens(query: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for word in query.split(|c: char| !c.is_alphanumeric()) {
        if word.is_empty() {
            continue;
        }
        let word = word.to_lowercase();
        if !out.contains(&word) {
            out.push(word);
        }
        if out.len() == MAX_WORDS {
            break;
        }
    }
    out
}

fn quoted(term: &str) -> String {
    // Words only hold letters and digits, so nothing needs escaping; this is belt and braces.
    format!("\"{}\"", term.replace('"', "\"\""))
}

/// FTS5 expression where every word is a prefix and all must match. `None` for no words.
pub fn strict_expression(words: &[String]) -> Option<String> {
    if words.is_empty() {
        return None;
    }
    Some(
        words
            .iter()
            .map(|w| format!("{}*", quoted(w)))
            .collect::<Vec<_>>()
            .join(" AND "),
    )
}

/// How many typos a word of this length may contain: none for short words (too many false
/// hits), one for medium, two for long.
pub fn typo_budget(chars: usize) -> usize {
    match chars {
        0..=3 => 0,
        4..=7 => 1,
        _ => 2,
    }
}

/// Edit distance counting an insertion, deletion, substitution or swap of neighbours as one.
pub fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (n, m) = (a.len(), b.len());
    let mut d = vec![vec![0usize; m + 1]; n + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=n {
        for j in 1..=m {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut best = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                best = best.min(d[i - 2][j - 2] + 1);
            }
            d[i][j] = best;
        }
    }
    d[n][m]
}

/// Indexed terms that are within the word's typo budget of it, either as a whole word or as
/// the start of a longer word (the user may still be typing). Only for words that match no
/// indexed term as a prefix; the others are left to the strict match. Closest first.
pub fn similar_terms(word: &str, vocabulary: &[String]) -> Vec<String> {
    let n = word.chars().count();
    let budget = typo_budget(n);
    // A word that already matches something is taken at its word: no "corrections" for it.
    if budget == 0 || vocabulary.iter().any(|t| t.starts_with(word)) {
        return Vec::new();
    }
    let mut found: Vec<(usize, usize, &String)> = Vec::new();
    for term in vocabulary {
        let len = term.chars().count();
        if len + budget < n {
            continue;
        }
        let whole = (len <= n + budget).then(|| distance(word, term));
        let start: String = term.chars().take(n).collect();
        let prefix = (len > n).then(|| distance(word, &start));
        let Some(d) = [whole, prefix].into_iter().flatten().min() else {
            continue;
        };
        if d <= budget {
            found.push((d, len.abs_diff(n), term));
        }
    }
    found.sort();
    found
        .into_iter()
        .take(MAX_ALTERNATIVES)
        .map(|(_, _, t)| t.clone())
        .collect()
}

/// Like [`strict_expression`], but each word may also match its typo-tolerant alternatives.
/// `None` when no word has any, so the caller can skip a second query.
pub fn fuzzy_expression(words: &[String], vocabulary: &[String]) -> Option<String> {
    let mut any = false;
    let groups: Vec<String> = words
        .iter()
        .map(|w| {
            let alternatives = similar_terms(w, vocabulary);
            any |= !alternatives.is_empty();
            let mut options = vec![format!("{}*", quoted(w))];
            options.extend(alternatives.iter().map(|a| quoted(a)));
            format!("({})", options.join(" OR "))
        })
        .collect();
    (any && !groups.is_empty()).then(|| groups.join(" AND "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn vocab(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| (*w).to_owned()).collect()
    }

    #[test]
    fn words_are_lowercase_unique_and_split_on_punctuation() {
        assert_eq!(tokens("  Fix, log-in FIX "), ["fix", "log", "in"]);
        assert!(tokens("  ... ").is_empty());
        assert_eq!(tokens("Zoë ÅNGSTRÖM"), ["zoë", "ångström"]);
        assert_eq!(tokens("a b c d e f g h i j").len(), MAX_WORDS);
    }

    #[test]
    fn strict_expression_makes_every_word_a_prefix() {
        let w = tokens("fix log");
        assert_eq!(
            strict_expression(&w).as_deref(),
            Some("\"fix\"* AND \"log\"*")
        );
        assert_eq!(strict_expression(&[]), None);
    }

    #[test]
    fn distance_counts_swaps_as_one_edit() {
        assert_eq!(distance("login", "login"), 0);
        assert_eq!(distance("loign", "login"), 1);
        assert_eq!(distance("logn", "login"), 1);
        assert_eq!(distance("logiin", "login"), 1);
        assert_eq!(distance("lagin", "login"), 1);
        assert_eq!(distance("kitten", "sitting"), 3);
        assert_eq!(distance("", "abc"), 3);
    }

    #[test]
    fn typos_find_the_intended_terms() {
        let v = vocab(&["login", "logout", "timeout", "budget", "fix", "priya"]);
        assert_eq!(similar_terms("loign", &v), ["login"]);
        assert_eq!(similar_terms("timeot", &v), ["timeout"]);
        assert_eq!(similar_terms("budgte", &v), ["budget"]);
        // Short words are never fuzzy; exact prefixes are left to the strict match.
        assert!(similar_terms("fox", &v).is_empty());
        assert!(similar_terms("logi", &v).is_empty());
        // Nothing close, nothing returned.
        assert!(similar_terms("zebra", &v).is_empty());
    }

    #[test]
    fn a_word_still_being_typed_can_have_a_typo_in_it() {
        // "logn" is a prefix of nothing, but one edit from the start of "login".
        let v = vocab(&["login", "budget"]);
        assert_eq!(similar_terms("logn", &v), ["login"]);
    }

    #[test]
    fn long_words_get_two_typos() {
        let v = vocab(&["infrastructure"]);
        assert_eq!(similar_terms("infrastrcutre", &v), ["infrastructure"]);
        assert!(similar_terms("infraxtrxcxure", &v).is_empty());
    }

    #[test]
    fn closest_terms_come_first_and_the_list_is_capped() {
        let mut terms = vec!["plan".to_owned(), "plane".to_owned()];
        terms.extend((0..30).map(|i| format!("plax{i}")));
        let found = similar_terms("plam", &terms);
        assert_eq!(found[0], "plan");
        assert!(found.len() <= MAX_ALTERNATIVES);
    }

    #[test]
    fn fuzzy_expression_only_exists_when_something_can_be_corrected() {
        let v = vocab(&["login", "timeout"]);
        assert_eq!(fuzzy_expression(&tokens("login time"), &v), None);
        assert_eq!(
            fuzzy_expression(&tokens("loign time"), &v).as_deref(),
            Some("(\"loign\"* OR \"login\") AND (\"time\"*)")
        );
    }

    proptest! {
        #[test]
        fn expressions_only_contain_quoted_words_and_operators(q in "\\PC{0,60}") {
            let words = tokens(&q);
            let v = vocab(&["login", "budget"]);
            for expr in [strict_expression(&words), fuzzy_expression(&words, &v)].into_iter().flatten() {
                // Strip quoted terms; what is left may only be operators, parentheses, spaces, stars.
                let mut rest = String::new();
                let mut in_quote = false;
                for c in expr.chars() {
                    if c == '"' { in_quote = !in_quote; } else if !in_quote { rest.push(c); }
                }
                prop_assert!(!in_quote);
                prop_assert!(rest.chars().all(|c| matches!(c, ' ' | '*' | '(' | ')') || "ANDOR".contains(c)), "{expr}");
            }
        }

        #[test]
        fn distance_is_symmetric_and_zero_only_for_equal(a in "[a-z]{0,8}", b in "[a-z]{0,8}") {
            prop_assert_eq!(distance(&a, &b), distance(&b, &a));
            prop_assert_eq!(distance(&a, &b) == 0, a == b);
        }
    }
}
