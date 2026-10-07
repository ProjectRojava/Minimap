//! Reference links on an item (spec 28): a web page, a Google Drive file or any address that
//! opens in the system's browser. Which addresses are accepted is `minimap_core::links`; this is
//! the record and how a list shows it.

use serde::{Deserialize, Serialize};

/// A link: `title` may be empty, the address is then the label.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RefLink {
    #[serde(default)]
    pub title: String,
    pub url: String,
}

/// What a link points at, for its tag in the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    /// A Google Drive, Docs, Sheets or Slides address.
    Drive,
    Web,
    Email,
}

impl LinkKind {
    pub fn label(self) -> &'static str {
        match self {
            LinkKind::Drive => "Drive",
            LinkKind::Web => "Web",
            LinkKind::Email => "Email",
        }
    }
}

/// The part of an address before the first `/`, `?` or `#`.
pub fn host_part(after_scheme: &str) -> &str {
    after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(after_scheme)
}

impl RefLink {
    pub fn kind(&self) -> LinkKind {
        if self.url.to_ascii_lowercase().starts_with("mailto:") {
            return LinkKind::Email;
        }
        match self.host().as_deref() {
            Some(
                "drive.google.com" | "docs.google.com" | "sheets.google.com" | "slides.google.com",
            ) => LinkKind::Drive,
            _ => LinkKind::Web,
        }
    }

    /// The host of a web address, lower-cased, without `www.` (`None` for an email link).
    pub fn host(&self) -> Option<String> {
        let rest = self.url.split_once("://")?.1;
        let host = host_part(rest);
        let host = host.rsplit('@').next().unwrap_or(host);
        let host = host.split(':').next().unwrap_or(host).to_ascii_lowercase();
        Some(host.strip_prefix("www.").unwrap_or(&host).to_owned())
    }

    /// What to call the link in a list: its title, else a short form of the address
    /// (`example.com/docs/plan`, with the scheme and any query dropped).
    pub fn label(&self) -> String {
        if !self.title.is_empty() {
            return self.title.clone();
        }
        if let Some(mail) = self.url.strip_prefix("mailto:") {
            return mail.split('?').next().unwrap_or(mail).to_owned();
        }
        let rest = self.url.split_once("://").map_or(&*self.url, |(_, r)| r);
        let rest = rest.split(['?', '#']).next().unwrap_or(rest);
        let rest = rest.strip_prefix("www.").unwrap_or(rest);
        let rest = rest.trim_end_matches('/');
        if rest.chars().count() > 60 {
            format!("{}…", rest.chars().take(60).collect::<String>())
        } else {
            rest.to_owned()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(title: &str, url: &str) -> RefLink {
        RefLink {
            title: title.into(),
            url: url.into(),
        }
    }

    #[test]
    fn kinds_hosts_and_labels() {
        assert_eq!(
            link("", "https://drive.google.com/x").kind(),
            LinkKind::Drive
        );
        assert_eq!(
            link("", "https://docs.google.com/x").kind(),
            LinkKind::Drive
        );
        assert_eq!(link("", "https://www.example.com").kind(), LinkKind::Web);
        assert_eq!(link("", "mailto:a@b.co").kind(), LinkKind::Email);
        assert_eq!(
            link("", "https://www.Example.com:8080/a").host(),
            Some("example.com".into())
        );
        assert_eq!(link("Plan", "https://a.co").label(), "Plan");
        assert_eq!(
            link("", "https://www.example.com/docs/plan?x=1#top").label(),
            "example.com/docs/plan"
        );
        assert_eq!(link("", "https://example.com/").label(), "example.com");
        assert_eq!(link("", "mailto:a@b.co?subject=hi").label(), "a@b.co");
    }
}
