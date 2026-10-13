//! Backup file naming and retention (spec 20). Pure: the store does the copying.
//!
//! Names carry the kind and the UTC time, so a backup folder explains itself and Minimap
//! can tell its own files from anything else in the folder:
//! `minimap-20270303-153000.db` (manual), `minimap-auto-…`, `minimap-pre-migration-v6-…`,
//! `minimap-pre-restore-…`, `minimap-pre-encryption-…`.
//!
//! A manual backup can carry a name (a *save point*, spec 51): `minimap-20270303-153000--before-q2-replan.db`,
//! two hyphens and the name's slug after the time. Every older name still parses the same.

use minimap_types::BackupKind;
use time::{Date, Duration, Month, OffsetDateTime, Time};

/// An automatic backup is due when the last backup is older than this.
pub const AUTO_INTERVAL: Duration = Duration::hours(24);

const PREFIX: &str = "minimap-";
const EXTENSION: &str = ".db";

fn stamp(at: OffsetDateTime) -> String {
    let at = at.to_offset(time::UtcOffset::UTC);
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        at.year(),
        at.month() as u8,
        at.day(),
        at.hour(),
        at.minute(),
        at.second()
    )
}

/// The most characters a save point's name may have as typed.
pub const MAX_LABEL_CHARS: usize = 80;
/// The longest slug a name becomes in a file name.
pub const MAX_SLUG_LEN: usize = 40;
/// The slug used when a name has no ASCII letters or digits to make one from (the original name
/// is not kept anywhere: a backup is one file).
pub const FALLBACK_SLUG: &str = "save";

/// The slug of a save point's name, as it goes into the file name: lower-case ASCII words
/// joined by `-`, at most [`MAX_SLUG_LEN`] long; a name with none (all non-ASCII, say) becomes
/// `save`. A blank name, or one over [`MAX_LABEL_CHARS`], is refused with a sentence.
pub fn label_slug(text: &str) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Give the save point a name, or leave it blank".to_owned());
    }
    if text.chars().count() > MAX_LABEL_CHARS {
        return Err(format!(
            "A save point's name can be at most {MAX_LABEL_CHARS} characters"
        ));
    }
    let mut slug = String::new();
    for word in text
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
    {
        let extra = usize::from(!slug.is_empty()) + word.len();
        if slug.len() + extra > MAX_SLUG_LEN {
            if slug.is_empty() {
                slug.push_str(&word[..MAX_SLUG_LEN].to_ascii_lowercase());
            }
            break;
        }
        if !slug.is_empty() {
            slug.push('-');
        }
        slug.push_str(&word.to_ascii_lowercase());
    }
    Ok(if slug.is_empty() {
        FALLBACK_SLUG.to_owned()
    } else {
        slug
    })
}

/// A slug as words: `before-q2-replan` is "before q2 replan".
pub fn label_text(slug: &str) -> String {
    slug.replace('-', " ")
}

fn is_slug(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= MAX_SLUG_LEN
        && !text.starts_with('-')
        && !text.ends_with('-')
        && !text.contains("--")
        && text
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// The file name for a backup of `kind` made at `at`. `schema_version` is only used by
/// pre-migration backups (the version being upgraded from).
pub fn file_name(kind: BackupKind, at: OffsetDateTime, schema_version: u32) -> String {
    file_name_with_label(kind, at, schema_version, None)
}

/// [`file_name`] for a manual backup with a name: `label` is the slug from [`label_slug`]. Only a
/// manual backup has one; for any other kind it is ignored.
pub fn file_name_with_label(
    kind: BackupKind,
    at: OffsetDateTime,
    schema_version: u32,
    label: Option<&str>,
) -> String {
    let suffix = match (kind, label) {
        (BackupKind::Manual, Some(slug)) if is_slug(slug) => format!("--{slug}"),
        _ => String::new(),
    };
    let middle = match kind {
        BackupKind::Manual => String::new(),
        BackupKind::Auto => "auto-".to_owned(),
        BackupKind::PreMigration => format!("pre-migration-v{schema_version}-"),
        BackupKind::PreRestore => "pre-restore-".to_owned(),
        BackupKind::PreEncryption => "pre-encryption-".to_owned(),
        BackupKind::PreSync => "pre-sync-".to_owned(),
        BackupKind::PreRecover => "pre-recover-".to_owned(),
    };
    format!("{PREFIX}{middle}{}{suffix}{EXTENSION}", stamp(at))
}

fn parse_stamp(s: &str) -> Option<OffsetDateTime> {
    let (date, time) = s.split_once('-')?;
    if date.len() != 8 || time.len() != 6 || !s.bytes().all(|b| b.is_ascii_digit() || b == b'-') {
        return None;
    }
    let n = |part: &str| part.parse::<u32>().ok();
    let month = Month::try_from(u8::try_from(n(&date[4..6])?).ok()?).ok()?;
    let date = Date::from_calendar_date(
        i32::try_from(n(&date[0..4])?).ok()?,
        month,
        u8::try_from(n(&date[6..8])?).ok()?,
    )
    .ok()?;
    let time = Time::from_hms(
        u8::try_from(n(&time[0..2])?).ok()?,
        u8::try_from(n(&time[2..4])?).ok()?,
        u8::try_from(n(&time[4..6])?).ok()?,
    )
    .ok()?;
    Some(date.with_time(time).assume_utc())
}

/// What a backup's file name says about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedName {
    pub kind: BackupKind,
    pub at: OffsetDateTime,
    /// A named save point's slug (see [`label_text`]).
    pub label: Option<String>,
}

/// What a file name says about itself, or `None` when Minimap didn't name it.
pub fn parse_name(name: &str) -> Option<(BackupKind, OffsetDateTime)> {
    parse_name_full(name).map(|p| (p.kind, p.at))
}

/// [`parse_name`], and the name a manual backup was given.
pub fn parse_name_full(name: &str) -> Option<ParsedName> {
    let plain = |kind, at| {
        Some(ParsedName {
            kind,
            at,
            label: None,
        })
    };
    let rest = name.strip_prefix(PREFIX)?.strip_suffix(EXTENSION)?;
    if let Some(stamp) = rest.strip_prefix("auto-") {
        return plain(BackupKind::Auto, parse_stamp(stamp)?);
    }
    if let Some(stamp) = rest.strip_prefix("pre-restore-") {
        return plain(BackupKind::PreRestore, parse_stamp(stamp)?);
    }
    if let Some(stamp) = rest.strip_prefix("pre-encryption-") {
        return plain(BackupKind::PreEncryption, parse_stamp(stamp)?);
    }
    if let Some(stamp) = rest.strip_prefix("pre-sync-") {
        return plain(BackupKind::PreSync, parse_stamp(stamp)?);
    }
    if let Some(stamp) = rest.strip_prefix("pre-recover-") {
        return plain(BackupKind::PreRecover, parse_stamp(stamp)?);
    }
    if let Some(after) = rest.strip_prefix("pre-migration-v") {
        let (version, stamp) = after.split_once('-')?;
        if version.is_empty() || !version.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        return plain(BackupKind::PreMigration, parse_stamp(stamp)?);
    }
    match rest.split_once("--") {
        Some((stamp, slug)) if is_slug(slug) => Some(ParsedName {
            kind: BackupKind::Manual,
            at: parse_stamp(stamp)?,
            label: Some(slug.to_owned()),
        }),
        Some(_) => None,
        None => plain(BackupKind::Manual, parse_stamp(rest)?),
    }
}

/// Whether the daily backup should run: no backup yet, or the newest is over 24 hours old.
/// `newest` is the time of the newest manual or automatic backup.
pub fn is_due(newest: Option<OffsetDateTime>, now: OffsetDateTime) -> bool {
    newest.is_none_or(|t| now - t > AUTO_INTERVAL)
}

/// The automatic backups to delete so only the newest `keep` remain. `entries` are
/// `(file name, kind, time)`; manual, pre-migration and pre-restore files are never listed.
pub fn prune_plan(entries: &[(String, BackupKind, OffsetDateTime)], keep: usize) -> Vec<String> {
    let mut auto: Vec<&(String, BackupKind, OffsetDateTime)> = entries
        .iter()
        .filter(|(_, kind, _)| *kind == BackupKind::Auto)
        .collect();
    // Newest first; ties broken by name so the plan is deterministic.
    auto.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| b.0.cmp(&a.0)));
    auto.into_iter().skip(keep).map(|e| e.0.clone()).collect()
}

/// "2027-03-03 15:30 UTC".
pub fn display_time(at: OffsetDateTime) -> String {
    let at = at.to_offset(time::UtcOffset::UTC);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02} UTC",
        at.year(),
        at.month() as u8,
        at.day(),
        at.hour(),
        at.minute()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use time::macros::datetime;

    #[test]
    fn names_say_what_they_are_and_when() {
        let at = datetime!(2027-03-03 15:30:05 UTC);
        assert_eq!(
            file_name(BackupKind::Manual, at, 0),
            "minimap-20270303-153005.db"
        );
        assert_eq!(
            file_name(BackupKind::Auto, at, 0),
            "minimap-auto-20270303-153005.db"
        );
        assert_eq!(
            file_name(BackupKind::PreMigration, at, 6),
            "minimap-pre-migration-v6-20270303-153005.db"
        );
        assert_eq!(
            file_name(BackupKind::PreRestore, at, 0),
            "minimap-pre-restore-20270303-153005.db"
        );
        assert_eq!(
            file_name(BackupKind::PreEncryption, at, 0),
            "minimap-pre-encryption-20270303-153005.db"
        );
        assert_eq!(
            file_name(BackupKind::PreSync, at, 0),
            "minimap-pre-sync-20270303-153005.db"
        );
        assert_eq!(
            file_name(BackupKind::PreRecover, at, 0),
            "minimap-pre-recover-20270303-153005.db"
        );
    }

    #[test]
    fn only_minimaps_own_names_parse() {
        let at = datetime!(2027-03-03 15:30:05 UTC);
        for kind in [
            BackupKind::Manual,
            BackupKind::Auto,
            BackupKind::PreMigration,
            BackupKind::PreRestore,
            BackupKind::PreEncryption,
            BackupKind::PreSync,
            BackupKind::PreRecover,
        ] {
            assert_eq!(parse_name(&file_name(kind, at, 12)), Some((kind, at)));
        }
        for other in [
            "minimap.db",
            "minimap.db-wal",
            "minimap-20270303-153005.db.part",
            "minimap-20271303-153005.db",
            "minimap-20270303-256005.db",
            "minimap-auto-2027.db",
            "minimap-pre-migration-vx-20270303-153005.db",
            "minimap-pre-migration-v-20270303-153005.db",
            "other-20270303-153005.db",
            "minimap-20270303-153005.sqlite",
            "",
        ] {
            assert_eq!(parse_name(other), None, "{other}");
        }
    }

    #[test]
    fn the_daily_backup_is_due_after_a_day() {
        let now = datetime!(2027-03-03 12:00 UTC);
        assert!(is_due(None, now));
        assert!(!is_due(Some(datetime!(2027-03-03 11:00 UTC)), now));
        assert!(
            !is_due(Some(datetime!(2027-03-02 12:00 UTC)), now),
            "exactly 24 h is not over"
        );
        assert!(is_due(Some(datetime!(2027-03-02 11:59 UTC)), now));
        // A clock that moved back never makes one due.
        assert!(!is_due(Some(datetime!(2027-03-04 12:00 UTC)), now));
    }

    #[test]
    fn only_the_newest_automatic_backups_are_kept() {
        let day = |d: u8| datetime!(2027-03-01 08:00 UTC) + Duration::days(i64::from(d));
        let mut entries: Vec<(String, BackupKind, OffsetDateTime)> = (0..20u8)
            .map(|d| {
                (
                    file_name(BackupKind::Auto, day(d), 0),
                    BackupKind::Auto,
                    day(d),
                )
            })
            .collect();
        // Other kinds are never pruned, however old.
        entries.push((
            file_name(BackupKind::Manual, day(0), 0),
            BackupKind::Manual,
            day(0),
        ));
        entries.push((
            file_name(BackupKind::PreMigration, day(0), 3),
            BackupKind::PreMigration,
            day(0),
        ));
        entries.push((
            file_name(BackupKind::PreRestore, day(0), 0),
            BackupKind::PreRestore,
            day(0),
        ));
        let doomed = prune_plan(&entries, 14);
        assert_eq!(doomed.len(), 6);
        let oldest: Vec<String> = (0..6u8)
            .map(|d| file_name(BackupKind::Auto, day(d), 0))
            .collect();
        for name in &oldest {
            assert!(doomed.contains(name), "{name}");
        }
        assert!(prune_plan(&entries, 20).is_empty());
        assert_eq!(prune_plan(&entries, 0).len(), 20);
    }

    #[test]
    fn times_read_in_utc() {
        assert_eq!(
            display_time(datetime!(2027-03-03 15:30:59 UTC)),
            "2027-03-03 15:30 UTC"
        );
        assert_eq!(
            display_time(datetime!(2027-03-03 17:30 +2)),
            "2027-03-03 15:30 UTC"
        );
    }

    proptest! {
        #[test]
        fn names_round_trip_and_sort_by_time(
            a in 0i64..4_000_000_000, b in 0i64..4_000_000_000, version in 0u32..1000
        ) {
            let ta = OffsetDateTime::from_unix_timestamp(a).unwrap();
            let tb = OffsetDateTime::from_unix_timestamp(b).unwrap();
            for kind in [BackupKind::Manual, BackupKind::Auto, BackupKind::PreMigration, BackupKind::PreRestore, BackupKind::PreEncryption] {
                let name = file_name(kind, ta, version);
                prop_assert_eq!(parse_name(&name), Some((kind, ta)));
            }
            // Names of one kind sort the same as their times, so a folder listing is in order.
            let (na, nb) = (file_name(BackupKind::Auto, ta, 0), file_name(BackupKind::Auto, tb, 0));
            prop_assert_eq!(na.cmp(&nb), ta.cmp(&tb));
        }

        #[test]
        fn pruning_keeps_exactly_the_newest(n in 0usize..40, keep in 0usize..20) {
            let base = datetime!(2027-01-01 00:00 UTC);
            let entries: Vec<_> = (0..n)
                .map(|i| {
                    let at = base + Duration::hours(i as i64);
                    (file_name(BackupKind::Auto, at, 0), BackupKind::Auto, at)
                })
                .collect();
            let doomed = prune_plan(&entries, keep);
            prop_assert_eq!(doomed.len(), n.saturating_sub(keep));
            for (i, (name, _, _)) in entries.iter().enumerate() {
                prop_assert_eq!(doomed.contains(name), i < n.saturating_sub(keep), "{}", name);
            }
        }
    }
    #[test]
    fn a_name_becomes_a_slug_of_ascii_words() {
        assert_eq!(label_slug("before Q2 replan").unwrap(), "before-q2-replan");
        assert_eq!(
            label_slug("  Before the Q2 replan!!  ").unwrap(),
            "before-the-q2-replan"
        );
        assert_eq!(label_slug("a_b/c").unwrap(), "a-b-c");
        // No ASCII letters or digits to make one from: the fallback.
        assert_eq!(label_slug("日本語").unwrap(), "save");
        assert_eq!(label_slug("???").unwrap(), "save");
        assert!(label_slug("   ").is_err());
        assert!(label_slug("").is_err());
        assert!(label_slug(&"x".repeat(81)).is_err());
        // Long names are cut at a word and never over 40.
        let long = label_slug("one two three four five six seven eight nine ten eleven").unwrap();
        assert!(long.len() <= MAX_SLUG_LEN, "{long}");
        assert!(long.starts_with("one-two-three"));
        assert!(!long.ends_with('-'));
        // A single endless word is cut.
        assert_eq!(label_slug(&"a".repeat(70)).unwrap().len(), MAX_SLUG_LEN);
        assert_eq!(label_text("before-q2-replan"), "before q2 replan");
    }

    #[test]
    fn a_named_manual_backup_has_the_slug_after_two_hyphens_and_nothing_else_does() {
        let at = datetime!(2027-03-03 15:30:05 UTC);
        assert_eq!(
            file_name_with_label(BackupKind::Manual, at, 0, Some("before-q2-replan")),
            "minimap-20270303-153005--before-q2-replan.db"
        );
        // Unnamed names are as they always were, and only a manual backup can be named.
        assert_eq!(
            file_name_with_label(BackupKind::Manual, at, 0, None),
            "minimap-20270303-153005.db"
        );
        assert_eq!(
            file_name_with_label(BackupKind::Auto, at, 0, Some("nope")),
            "minimap-auto-20270303-153005.db"
        );
        // A label that is not a slug is not trusted into a file name.
        assert_eq!(
            file_name_with_label(BackupKind::Manual, at, 0, Some("../x")),
            "minimap-20270303-153005.db"
        );
        let parsed = parse_name_full("minimap-20270303-153005--before-q2-replan.db").unwrap();
        assert_eq!(parsed.kind, BackupKind::Manual);
        assert_eq!(parsed.at, at);
        assert_eq!(parsed.label.as_deref(), Some("before-q2-replan"));
        assert_eq!(
            parse_name("minimap-20270303-153005--before-q2-replan.db"),
            Some((BackupKind::Manual, at))
        );
        // Every older name parses as before, with no label.
        for kind in [
            BackupKind::Manual,
            BackupKind::Auto,
            BackupKind::PreMigration,
            BackupKind::PreRestore,
            BackupKind::PreEncryption,
            BackupKind::PreSync,
            BackupKind::PreRecover,
        ] {
            let parsed = parse_name_full(&file_name(kind, at, 9)).unwrap();
            assert_eq!((parsed.kind, parsed.at, parsed.label), (kind, at, None));
        }
        for bad in [
            "minimap-20270303-153005--.db",
            "minimap-20270303-153005---x.db",
            "minimap-20270303-153005--Upper.db",
            "minimap-20270303-153005--two--dashes.db",
            "minimap-20270303-153005--x-.db",
        ] {
            assert_eq!(parse_name_full(bad), None, "{bad}");
        }
    }

    proptest! {
        /// A name that was made is read back: same kind, same second, same slug.
        #[test]
        fn a_made_name_reads_back(
            secs in 0i64..4_000_000_000,
            text in "[!-~][ -~]{0,59}",
            version in 0u32..200,
        ) {
            let at = OffsetDateTime::from_unix_timestamp(secs).unwrap();
            let slug = label_slug(&text).unwrap();
            let named = file_name_with_label(BackupKind::Manual, at, version, Some(&slug));
            let parsed = parse_name_full(&named).expect("parses");
            prop_assert_eq!(parsed.kind, BackupKind::Manual);
            prop_assert_eq!(parsed.at, at);
            prop_assert_eq!(parsed.label, Some(slug));
            for kind in [BackupKind::Auto, BackupKind::PreMigration, BackupKind::PreRestore] {
                let parsed = parse_name_full(&file_name(kind, at, version)).expect("parses");
                prop_assert_eq!((parsed.kind, parsed.at, parsed.label), (kind, at, None));
            }
        }
    }
}
