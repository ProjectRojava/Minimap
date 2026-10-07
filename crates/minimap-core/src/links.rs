//! Reference links on an item (spec 28): what counts as an address, how it is tidied before it
//! is stored, and how it is labelled. Only web addresses (`http`, `https`) and email addresses
//! (`mailto`) are accepted: a link opens in the system's browser or mail program, and anything
//! else (`file:`, `javascript:`, `data:`, ...) could run something or reach into the disk.

use minimap_types::{host_part, RefLink};

/// Most links one item can carry.
pub const MAX_LINKS: usize = 50;
/// Longest address, in characters.
pub const MAX_URL_LEN: usize = 2000;
/// Longest title, in characters.
pub const MAX_TITLE_LEN: usize = 200;

const NOT_ALLOWED: &str =
    "Only web addresses (http or https) and email addresses (mailto:) can be added as links";

/// The address as it will be stored and opened. A bare `example.com/page` or `drive.google.com/...`
/// gets `https://`; a scheme is lower-cased; anything that is not a web or email address is
/// refused with a message for the user.
pub fn normalize(raw: &str) -> Result<String, String> {
    let text = raw.trim();
    if text.is_empty() {
        return Err("Type or paste an address".to_owned());
    }
    if text.chars().any(char::is_whitespace) {
        return Err("An address can't contain spaces".to_owned());
    }
    if text.chars().count() > MAX_URL_LEN {
        return Err(format!(
            "An address can be at most {MAX_URL_LEN} characters long"
        ));
    }
    match scheme_of(text) {
        Some((scheme, rest)) => match scheme.to_ascii_lowercase().as_str() {
            "http" | "https" => {
                let rest = rest.strip_prefix("//").ok_or(NOT_ALLOWED)?;
                host_ok(rest)?;
                Ok(format!("{}://{rest}", scheme.to_ascii_lowercase()))
            }
            "mailto" => {
                if rest.trim_start_matches('?').is_empty() || !rest.contains('@') {
                    return Err(
                        "An email link needs an address, like mailto:name@example.com".to_owned(),
                    );
                }
                Ok(format!("mailto:{rest}"))
            }
            _ => Err(NOT_ALLOWED.to_owned()),
        },
        None => {
            host_ok(text)?;
            Ok(format!("https://{text}"))
        }
    }
}

/// `(scheme, rest after the colon)` when the text starts with `scheme:` (a letter, then letters,
/// digits, `+`, `-` or `.`). `localhost:3000` is a host and a port, not a scheme.
fn scheme_of(text: &str) -> Option<(&str, &str)> {
    let colon = text.find(':')?;
    let (scheme, rest) = (&text[..colon], &text[colon + 1..]);
    let mut chars = scheme.chars();
    let valid = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    let port = rest.chars().next().is_some_and(|c| c.is_ascii_digit())
        && rest
            .split(['/', '?', '#'])
            .next()
            .is_some_and(|p| p.chars().all(|c| c.is_ascii_digit()));
    (valid && !port).then_some((scheme, rest))
}

/// The part before the first `/`, `?` or `#` is a host: something, with a dot (or `localhost`).
fn host_ok(after_scheme: &str) -> Result<(), String> {
    let host = host_part(after_scheme);
    let host = host.rsplit('@').next().unwrap_or(host);
    let name = host.split(':').next().unwrap_or(host);
    if name.is_empty() || !(name.contains('.') || name.eq_ignore_ascii_case("localhost")) {
        return Err("That doesn't look like a web address (like example.com/page)".to_owned());
    }
    Ok(())
}

/// A stored list made ready to store: titles trimmed and capped, addresses normalised, a repeat
/// of an address dropped (the first stays). The error names the link it is about.
pub fn clean(links: Vec<RefLink>) -> Result<Vec<RefLink>, String> {
    let mut out: Vec<RefLink> = Vec::with_capacity(links.len());
    for link in links {
        let url = normalize(&link.url).map_err(|e| format!("{e} ({})", shorten(&link.url)))?;
        let title = link.title.split_whitespace().collect::<Vec<_>>().join(" ");
        if title.chars().count() > MAX_TITLE_LEN {
            return Err(format!(
                "A link's name can be at most {MAX_TITLE_LEN} characters long"
            ));
        }
        if out.iter().any(|l| l.url == url) {
            continue;
        }
        out.push(RefLink { title, url });
    }
    if out.len() > MAX_LINKS {
        return Err(format!("An item can have at most {MAX_LINKS} links"));
    }
    Ok(out)
}

fn shorten(text: &str) -> String {
    let text = text.trim();
    if text.chars().count() > 40 {
        format!("{}…", text.chars().take(40).collect::<String>())
    } else {
        text.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn link(title: &str, url: &str) -> RefLink {
        RefLink {
            title: title.into(),
            url: url.into(),
        }
    }

    #[test]
    fn web_addresses_are_kept_and_bare_ones_get_https() {
        assert_eq!(
            normalize("https://example.com/a?b=1#c").unwrap(),
            "https://example.com/a?b=1#c"
        );
        assert_eq!(
            normalize("  example.com/plan ").unwrap(),
            "https://example.com/plan"
        );
        assert_eq!(
            normalize("HTTP://Example.com").unwrap(),
            "http://Example.com"
        );
        assert_eq!(
            normalize("drive.google.com/file/d/abc/view").unwrap(),
            "https://drive.google.com/file/d/abc/view"
        );
        assert_eq!(
            normalize("localhost:3000/x").unwrap(),
            "https://localhost:3000/x"
        );
        assert_eq!(normalize("mailto:a@b.co").unwrap(), "mailto:a@b.co");
    }

    #[test]
    fn anything_that_could_run_or_read_the_disk_is_refused() {
        for bad in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "data:text/html,hi",
            "ftp://example.com/a",
            "vbscript:x",
            "/home/me/doc.pdf",
            "C:\\docs\\a.pdf",
            "https:example.com",
        ] {
            assert!(normalize(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn nonsense_is_refused_with_a_reason() {
        assert!(normalize("").unwrap_err().contains("address"));
        assert!(normalize("two words.com").unwrap_err().contains("spaces"));
        assert!(normalize("justaword").unwrap_err().contains("web address"));
        assert!(normalize("https://").is_err());
        assert!(normalize("mailto:nobody").is_err());
        assert!(normalize(&format!("https://a.co/{}", "x".repeat(MAX_URL_LEN))).is_err());
    }

    #[test]
    fn clean_tidies_titles_drops_repeats_and_keeps_order() {
        let out = clean(vec![
            link("  Design   doc ", "docs.google.com/document/d/1"),
            link("", "https://example.com"),
            link("Again", "https://docs.google.com/document/d/1"),
        ])
        .unwrap();
        assert_eq!(
            out,
            vec![
                link("Design doc", "https://docs.google.com/document/d/1"),
                link("", "https://example.com"),
            ]
        );
    }

    #[test]
    fn clean_names_the_link_it_refuses_and_caps_the_count() {
        let err = clean(vec![link("x", "javascript:alert(1)")]).unwrap_err();
        assert!(err.contains("javascript:alert(1)"), "{err}");
        let many: Vec<RefLink> = (0..=MAX_LINKS)
            .map(|i| link("", &format!("https://example.com/{i}")))
            .collect();
        assert!(clean(many).unwrap_err().contains("at most"));
        assert!(clean(vec![link(&"t".repeat(MAX_TITLE_LEN + 1), "https://a.co")]).is_err());
    }

    proptest! {
        /// Whatever is typed, an accepted address is http(s) or mailto with no spaces, and
        /// normalising it again changes nothing.
        #[test]
        fn accepted_addresses_are_safe_and_stable(raw in "\\PC{0,80}") {
            if let Ok(url) = normalize(&raw) {
                let lower = url.to_ascii_lowercase();
                prop_assert!(lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:"));
                prop_assert!(!url.chars().any(char::is_whitespace));
                prop_assert_eq!(normalize(&url).unwrap(), url);
            }
        }
    }
}
