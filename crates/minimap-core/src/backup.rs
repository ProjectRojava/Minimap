//! Backup file naming and retention (spec 20). Pure: the store does the copying.
//!
//! Names carry the kind and the UTC time, so a backup folder explains itself and Minimap
//! can tell its own files from anything else in the folder:
//! `minimap-20270303-153000.db` (manual), `minimap-auto-…`, `minimap-pre-migration-v6-…`,
//! `minimap-pre-restore-…`.

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

/// The file name for a backup of `kind` made at `at`. `schema_version` is only used by
/// pre-migration backups (the version being upgraded from).
pub fn file_name(kind: BackupKind, at: OffsetDateTime, schema_version: u32) -> String {
    let middle = match kind {
        BackupKind::Manual => String::new(),
        BackupKind::Auto => "auto-".to_owned(),
        BackupKind::PreMigration => format!("pre-migration-v{schema_version}-"),
        BackupKind::PreRestore => "pre-restore-".to_owned(),
    };
    format!("{PREFIX}{middle}{}{EXTENSION}", stamp(at))
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

/// What a file name says about itself, or `None` when Minimap didn't name it.
pub fn parse_name(name: &str) -> Option<(BackupKind, OffsetDateTime)> {
    let rest = name.strip_prefix(PREFIX)?.strip_suffix(EXTENSION)?;
    if let Some(stamp) = rest.strip_prefix("auto-") {
        return Some((BackupKind::Auto, parse_stamp(stamp)?));
    }
    if let Some(stamp) = rest.strip_prefix("pre-restore-") {
        return Some((BackupKind::PreRestore, parse_stamp(stamp)?));
    }
    if let Some(after) = rest.strip_prefix("pre-migration-v") {
        let (version, stamp) = after.split_once('-')?;
        if version.is_empty() || !version.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        return Some((BackupKind::PreMigration, parse_stamp(stamp)?));
    }
    Some((BackupKind::Manual, parse_stamp(rest)?))
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
    }

    #[test]
    fn only_minimaps_own_names_parse() {
        let at = datetime!(2027-03-03 15:30:05 UTC);
        for kind in [
            BackupKind::Manual,
            BackupKind::Auto,
            BackupKind::PreMigration,
            BackupKind::PreRestore,
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
            for kind in [BackupKind::Manual, BackupKind::Auto, BackupKind::PreMigration, BackupKind::PreRestore] {
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
}
