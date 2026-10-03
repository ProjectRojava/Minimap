//! Project handles (`#api-launch`): generation, validation and uniqueness.

pub const MAX_LEN: usize = 40;

/// Lowercase ASCII letters and digits joined by single hyphens; anything else separates
/// words. Falls back to "project" when nothing usable is left.
pub fn slugify(title: &str) -> String {
    let mut out = String::new();
    let mut pending_dash = false;
    for c in title.chars() {
        if c.is_ascii_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.push(c.to_ascii_lowercase());
        } else {
            pending_dash = true;
        }
    }
    out.truncate(MAX_LEN);
    let out = out.trim_end_matches('-').to_owned();
    if out.is_empty() {
        "project".to_owned()
    } else {
        out
    }
}

/// `a-z`, `0-9` and single inner hyphens, at most [`MAX_LEN`] characters.
pub fn validate(slug: &str) -> Result<(), String> {
    if slug.is_empty() {
        return Err("the handle must not be empty".into());
    }
    if slug.len() > MAX_LEN {
        return Err(format!("the handle must be at most {MAX_LEN} characters"));
    }
    let ok = slug.split('-').all(|part| {
        !part.is_empty()
            && part
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    });
    if ok {
        Ok(())
    } else {
        Err("the handle may only use lowercase letters, digits and single hyphens (e.g. api-launch)".into())
    }
}

/// `base`, or `base-2`, `base-3`, ... the first one `is_taken` doesn't claim.
pub fn unique(base: &str, is_taken: impl Fn(&str) -> bool) -> String {
    if !is_taken(base) {
        return base.to_owned();
    }
    (2..)
        .map(|n| {
            // Keep the whole handle within the length limit.
            let suffix = format!("-{n}");
            let keep = MAX_LEN.saturating_sub(suffix.len());
            let stem = base[..base.len().min(keep)].trim_end_matches('-');
            format!("{stem}{suffix}")
        })
        .find(|candidate| !is_taken(candidate))
        .unwrap_or_else(|| base.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn slugify_titles() {
        assert_eq!(slugify("API Launch"), "api-launch");
        assert_eq!(slugify("  Q1 -- EU region!! "), "q1-eu-region");
        assert_eq!(slugify("Café déjà vu"), "caf-d-j-vu");
        assert_eq!(slugify("日本語"), "project");
        assert_eq!(slugify(""), "project");
        assert_eq!(slugify("a_b.c/d"), "a-b-c-d");
        let long = slugify(&"word ".repeat(30));
        assert!(long.len() <= MAX_LEN && !long.ends_with('-'));
    }

    #[test]
    fn validation() {
        for ok in ["a", "api-launch", "q1-2027", "x".repeat(MAX_LEN).as_str()] {
            assert!(validate(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "-a",
            "a-",
            "a--b",
            "A",
            "a b",
            "a_b",
            "é",
            "x".repeat(MAX_LEN + 1).as_str(),
        ] {
            assert!(validate(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn unique_adds_the_first_free_suffix() {
        let taken = ["api", "api-2", "api-3"];
        assert_eq!(unique("web", |s| taken.contains(&s)), "web");
        assert_eq!(unique("api", |s| taken.contains(&s)), "api-4");
        assert_eq!(unique("api", |_| false), "api");
    }

    #[test]
    fn unique_respects_the_length_limit() {
        let base = "x".repeat(MAX_LEN);
        let result = unique(&base, |s| s == base);
        assert!(
            result.len() <= MAX_LEN && result.ends_with("-2"),
            "{result}"
        );
        assert!(validate(&result).is_ok());
    }

    proptest! {
        #[test]
        fn slugify_always_yields_a_valid_handle(title in ".{0,120}") {
            prop_assert!(validate(&slugify(&title)).is_ok());
        }
    }
}
