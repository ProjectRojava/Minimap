//! The decisions behind multi-device sync (spec 22), as pure functions: which copy of a row
//! wins, which links to drop when two devices together closed a loop, when to save, how long to
//! wait after a failure, and which checkpoints to keep. The store and the sync engine do the IO.

use std::collections::HashSet;

use minimap_types::{Uuid, DAILY_CHECKPOINTS_KEPT, HOURLY_CHECKPOINTS_KEPT};
use time::{Date, Duration, Month, OffsetDateTime, Time};

use crate::cycles::find_cycle;

// ------------------------------------------------------------------ timing

/// Save once nothing has changed for this long...
pub const IDLE_BEFORE_SAVE_MS: u64 = 5_000;
/// ...but never later than this after the first unsaved change, so steady typing still saves.
pub const MAX_UNSAVED_MS: u64 = 30_000;
/// How often other devices' snapshots are looked for.
pub const POLL_SECS: u64 = 15;

/// How long to wait before saving. `None` = nothing to save; `Some(0)` = save now.
/// `first_unsaved_ms` is when the oldest unsaved change was made, `last_change_ms` the newest;
/// all three clocks are the same monotonic millisecond counter.
pub fn save_wait_ms(
    first_unsaved_ms: Option<u64>,
    last_change_ms: u64,
    now_ms: u64,
) -> Option<u64> {
    let first = first_unsaved_ms?;
    let idle_at = last_change_ms.saturating_add(IDLE_BEFORE_SAVE_MS);
    let deadline = first.saturating_add(MAX_UNSAVED_MS);
    let at = idle_at.min(deadline);
    Some(at.saturating_sub(now_ms))
}

/// Seconds to wait after the `failures`-th failure in a row (0 = the first): 5 s, 30 s, 2 min,
/// then every 10 min.
pub fn backoff_secs(failures: u32) -> u64 {
    match failures {
        0 => 5,
        1 => 30,
        2 => 120,
        _ => 600,
    }
}

/// After this many days without a successful save the app keeps a warning on screen.
pub const OFFLINE_WARNING_DAYS: i64 = 3;

// ------------------------------------------------------------------ who wins

/// 64-bit FNV-1a: a hash that is the same on every machine and every run (unlike the standard
/// library's), used to break ties between two versions of a row with the same timestamp.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Should the remote copy of a row replace the local one? Timestamps are fixed-width ISO text,
/// so text order is time order. The later write wins; equal timestamps go to the larger content
/// hash, so both devices pick the same copy; identical content changes nothing.
pub fn remote_wins(
    local_updated: &str,
    local_hash: u64,
    remote_updated: &str,
    remote_hash: u64,
) -> bool {
    match remote_updated.cmp(local_updated) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => remote_hash > local_hash,
    }
}

// ------------------------------------------------------------------ loops

/// A link taking part in an acyclic relation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopLink {
    pub from: Uuid,
    pub to: Uuid,
    /// When it was last written (fixed-width ISO text).
    pub stamp: String,
    /// A stable tie-break, the same on every device (the link's identity).
    pub key: String,
}

/// Which links to drop so that what remains has no loop: links are taken oldest first and a link
/// that would close a loop with the ones already kept is dropped. The result depends only on the
/// links, so every device drops the same ones. Returns indices into `links`.
pub fn links_to_drop(links: &[LoopLink]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..links.len()).collect();
    order.sort_by(|&a, &b| {
        (links[a].stamp.as_str(), links[a].key.as_str())
            .cmp(&(links[b].stamp.as_str(), links[b].key.as_str()))
    });
    let mut kept: Vec<(Uuid, Uuid)> = Vec::new();
    let mut dropped = Vec::new();
    for i in order {
        let l = &links[i];
        if find_cycle(&kept, l.from, l.to).is_some() {
            dropped.push(i);
        } else {
            kept.push((l.from, l.to));
        }
    }
    dropped.sort_unstable();
    dropped
}

// ------------------------------------------------------------------ names on Drive

/// `<device id>.db.enc`: a device's own snapshot.
pub fn device_file_name(device_id: &str) -> String {
    format!("{device_id}.db.enc")
}

/// The device id in a snapshot file name, if it is one.
pub fn parse_device_file_name(name: &str) -> Option<&str> {
    let id = name.strip_suffix(".db.enc")?;
    is_valid_device_id(id).then_some(id)
}

/// Device ids end up in file names, so only lowercase letters, digits and hyphens are allowed.
pub fn is_valid_device_id(id: &str) -> bool {
    (8..=64).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

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
    Some(OffsetDateTime::new_utc(date, time))
}

/// `<device id>-YYYYMMDD-HHMMSS.db.enc`.
pub fn checkpoint_name(device_id: &str, at: OffsetDateTime) -> String {
    format!("{device_id}-{}.db.enc", stamp(at))
}

/// The device and time in a checkpoint's file name.
pub fn parse_checkpoint_name(name: &str) -> Option<(String, OffsetDateTime)> {
    let body = name.strip_suffix(".db.enc")?;
    // The stamp is the last 15 characters: YYYYMMDD-HHMMSS.
    let split = body.len().checked_sub(15)?;
    if !body.is_char_boundary(split) || split == 0 {
        return None;
    }
    let (id, stamp_text) = body.split_at(split);
    let id = id.strip_suffix('-')?;
    if !is_valid_device_id(id) {
        return None;
    }
    Some((id.to_owned(), parse_stamp(stamp_text)?))
}

/// The first snapshot of each hour is also kept as a checkpoint.
pub fn checkpoint_due(last: Option<OffsetDateTime>, now: OffsetDateTime) -> bool {
    match last {
        None => true,
        Some(last) => {
            let hour = |t: OffsetDateTime| {
                let t = t.to_offset(time::UtcOffset::UTC);
                (t.date(), t.hour())
            };
            hour(last) != hour(now)
        }
    }
}

/// Which checkpoints to delete: the newest `hourly` stay, then the newest one of each calendar
/// day (UTC) for `daily` more days; everything older goes. `files` are one device's checkpoints.
pub fn checkpoint_prune_plan(
    files: &[(String, OffsetDateTime)],
    hourly: u32,
    daily: u32,
) -> Vec<String> {
    let mut sorted: Vec<&(String, OffsetDateTime)> = files.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.cmp(&a.0)));
    let mut keep: HashSet<&str> = HashSet::new();
    let mut days: Vec<Date> = Vec::new();
    for (i, (name, at)) in sorted.iter().enumerate() {
        if i < hourly as usize {
            keep.insert(name);
            continue;
        }
        let day = at.to_offset(time::UtcOffset::UTC).date();
        if !days.contains(&day) && days.len() < daily as usize {
            days.push(day);
            keep.insert(name);
        }
    }
    sorted
        .into_iter()
        .filter(|(name, _)| !keep.contains(name.as_str()))
        .map(|(name, _)| name.clone())
        .collect()
}

/// [`checkpoint_prune_plan`] with the defaults from the spec (24 hourly + 30 daily).
pub fn default_checkpoint_prune_plan(files: &[(String, OffsetDateTime)]) -> Vec<String> {
    checkpoint_prune_plan(files, HOURLY_CHECKPOINTS_KEPT, DAILY_CHECKPOINTS_KEPT)
}

/// Attachment blobs nobody references are removed from Drive only after this long, so a device
/// that was offline can still catch up.
pub const ORPHAN_MEDIA_GRACE: Duration = Duration::days(7);

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn at(day: u8, hour: u8, minute: u8) -> OffsetDateTime {
        OffsetDateTime::new_utc(
            Date::from_calendar_date(2027, Month::March, day).unwrap(),
            Time::from_hms(hour, minute, 0).unwrap(),
        )
    }

    #[test]
    fn nothing_dirty_means_nothing_to_wait_for() {
        assert_eq!(save_wait_ms(None, 0, 10_000), None);
    }

    #[test]
    fn a_quiet_app_saves_after_five_seconds() {
        // Changed at t=1s; nothing since. At t=3s there are 3 s to go; at t=6s it is time.
        assert_eq!(save_wait_ms(Some(1_000), 1_000, 3_000), Some(3_000));
        assert_eq!(save_wait_ms(Some(1_000), 1_000, 6_000), Some(0));
        assert_eq!(save_wait_ms(Some(1_000), 1_000, 60_000), Some(0));
    }

    #[test]
    fn steady_typing_still_saves_within_thirty_seconds() {
        // The first change was at t=0; the latest at t=29 s, which would push the idle save out
        // to t=34 s, but the cap says t=30 s.
        assert_eq!(save_wait_ms(Some(0), 29_000, 29_500), Some(500));
        assert_eq!(save_wait_ms(Some(0), 29_900, 30_000), Some(0));
    }

    #[test]
    fn failures_back_off_then_settle_at_ten_minutes() {
        let all: Vec<u64> = (0..6).map(backoff_secs).collect();
        assert_eq!(all, [5, 30, 120, 600, 600, 600]);
    }

    #[test]
    fn the_later_write_wins_and_ties_are_broken_the_same_way_everywhere() {
        let (old, new) = ("2027-03-01T10:00:00.000Z", "2027-03-01T10:00:00.001Z");
        assert!(remote_wins(old, 1, new, 0));
        assert!(!remote_wins(new, 0, old, 1));
        // Same timestamp: whichever side has the larger hash wins, seen from either device.
        assert!(remote_wins(old, 1, old, 2));
        assert!(!remote_wins(old, 2, old, 1));
        // Identical copies change nothing.
        assert!(!remote_wins(old, 7, old, 7));
    }

    #[test]
    fn fnv1a_is_stable() {
        // Known value for the empty input and for "a" (the FNV-1a 64-bit reference vectors).
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
    }

    fn link(from: u128, to: u128, stamp: &str) -> LoopLink {
        LoopLink {
            from: id(from),
            to: id(to),
            stamp: stamp.into(),
            key: format!("{from}-{to}"),
        }
    }

    #[test]
    fn the_newest_link_that_closes_a_loop_is_the_one_dropped() {
        // Device A wrote 1->2 and 2->3; device B (later) wrote 3->1.
        let links = [
            link(1, 2, "2027-01-01T00:00:00.000Z"),
            link(2, 3, "2027-01-02T00:00:00.000Z"),
            link(3, 1, "2027-01-03T00:00:00.000Z"),
        ];
        assert_eq!(links_to_drop(&links), vec![2]);
        // The order the links are listed in doesn't matter.
        let shuffled = [links[2].clone(), links[0].clone(), links[1].clone()];
        assert_eq!(links_to_drop(&shuffled), vec![0]);
    }

    #[test]
    fn links_without_a_loop_all_stay() {
        let links = [
            link(1, 2, "2027-01-01T00:00:00.000Z"),
            link(1, 3, "2027-01-01T00:00:00.000Z"),
            link(2, 3, "2027-01-01T00:00:00.000Z"),
        ];
        assert!(links_to_drop(&links).is_empty());
    }

    #[test]
    fn a_self_link_is_always_dropped() {
        assert_eq!(
            links_to_drop(&[link(4, 4, "2027-01-01T00:00:00.000Z")]),
            vec![0]
        );
    }

    #[test]
    fn device_and_checkpoint_names_round_trip() {
        let device = "0192f3a4-7b1c-7d2e-8a9b-1c2d3e4f5a6b";
        assert_eq!(
            parse_device_file_name(&device_file_name(device)),
            Some(device)
        );
        assert_eq!(parse_device_file_name("../evil.db.enc"), None);
        assert_eq!(parse_device_file_name("vault.json"), None);
        let when = at(3, 15, 30);
        let name = checkpoint_name(device, when);
        assert_eq!(name, format!("{device}-20270303-153000.db.enc"));
        assert_eq!(
            parse_checkpoint_name(&name),
            Some((device.to_owned(), when))
        );
        assert_eq!(parse_checkpoint_name("junk.db.enc"), None);
        assert_eq!(
            parse_checkpoint_name(&format!("{device}-20271303-153000.db.enc")),
            None
        );
        assert_eq!(
            parse_checkpoint_name(&format!("{device}-20270303-153000.db")),
            None
        );
    }

    #[test]
    fn a_checkpoint_is_due_once_per_hour() {
        let now = at(3, 15, 40);
        assert!(checkpoint_due(None, now));
        assert!(!checkpoint_due(Some(at(3, 15, 1)), now));
        assert!(checkpoint_due(Some(at(3, 14, 59)), now));
        assert!(
            checkpoint_due(Some(at(2, 15, 1)), now),
            "same hour, other day"
        );
    }

    #[test]
    fn retention_keeps_recent_hours_then_one_per_day() {
        // Two checkpoints a day on 6 days, hourly = 3, daily = 2.
        let mut files = Vec::new();
        for day in 1..=6u8 {
            for hour in [9u8, 17] {
                files.push((format!("d{day}-{hour}"), at(day, hour, 0)));
            }
        }
        let plan = checkpoint_prune_plan(&files, 3, 2);
        let kept: Vec<&str> = files
            .iter()
            .map(|(n, _)| n.as_str())
            .filter(|n| !plan.iter().any(|p| p == n))
            .collect();
        // Newest three: d6-17, d6-9, d5-17. Then one per day for two more days: d5 already has
        // a kept file... the next distinct days after the hourly ones are d5 (d5-9 is the
        // newest not yet kept) and d4 (d4-17).
        assert_eq!(kept.len(), 5);
        assert!(kept.contains(&"d6-17") && kept.contains(&"d6-9") && kept.contains(&"d5-17"));
        assert!(kept.contains(&"d5-9") && kept.contains(&"d4-17"));
        assert_eq!(plan.len(), 7);
    }

    #[test]
    fn retention_with_few_files_deletes_nothing() {
        let files = vec![("a".to_owned(), at(1, 1, 0)), ("b".to_owned(), at(1, 2, 0))];
        assert!(default_checkpoint_prune_plan(&files).is_empty());
        assert!(default_checkpoint_prune_plan(&[]).is_empty());
    }

    proptest! {
        /// Whatever links two devices wrote, what is kept has no loop and what is dropped is
        /// the same whichever order the links arrive in.
        #[test]
        fn what_remains_is_acyclic_and_independent_of_order(
            raw in prop::collection::vec((0u128..6, 0u128..6, 0u32..5), 0..14),
            seed in 0u64..1000,
        ) {
            let links: Vec<LoopLink> = raw
                .iter()
                .enumerate()
                .map(|(i, (a, b, t))| LoopLink {
                    from: id(*a),
                    to: id(*b),
                    stamp: format!("2027-01-0{}T00:00:00.000Z", t + 1),
                    key: format!("{a}-{b}-{i}"),
                })
                .collect();
            let dropped = links_to_drop(&links);
            let kept: Vec<(Uuid, Uuid)> = links
                .iter()
                .enumerate()
                .filter(|(i, _)| !dropped.contains(i))
                .map(|(_, l)| (l.from, l.to))
                .collect();
            for (i, (from, to)) in kept.iter().enumerate() {
                let others: Vec<(Uuid, Uuid)> = kept
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| *j != i)
                    .map(|(_, e)| *e)
                    .collect();
                // No kept link closes a loop with the rest.
                prop_assert!(find_cycle(&others, *from, *to).is_none());
            }
            // The same links in another order drop the same ones (compared by key).
            let mut shuffled = links.clone();
            let n = shuffled.len();
            if n > 1 {
                shuffled.rotate_left((seed as usize) % n);
            }
            let names = |ls: &[LoopLink], d: &[usize]| {
                let mut v: Vec<String> = d.iter().map(|&i| ls[i].key.clone()).collect();
                v.sort();
                v
            };
            prop_assert_eq!(names(&links, &dropped), names(&shuffled, &links_to_drop(&shuffled)));
        }
    }
}
