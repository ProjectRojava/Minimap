//! Save points (docs/product-design/08-save-points.md, spec 51): every restorable copy of the
//! data on one timeline. The local backups of spec 20 (named, manual, daily, safety copies) and
//! the Drive checkpoints of spec 22 are merged newest first and grouped by the user's own
//! calendar day, each marked if Minimap is about to delete it, and the newest says whether the
//! data is safe: when nothing is under a day old, why not. Pure; the commands do the listing.

use std::collections::HashSet;

use minimap_types::{
    BackupEntry, BackupKind, CheckpointCadence, CheckpointInfo, SaveDay, SavePoint, SaveSource,
    SaveTimeline, StaleReason, DAILY_CHECKPOINTS_KEPT, HOURLY_CHECKPOINTS_KEPT,
};
use time::{OffsetDateTime, UtcOffset};

use crate::{
    backup::{parse_name_full, prune_plan, AUTO_INTERVAL},
    sync::{checkpoint_prune_plan, parse_checkpoint_name},
};

/// Everything `merge` reads.
pub struct TimelineInput<'a> {
    /// The backups in the backup folder (as `store::backup::list` found them).
    pub local: &'a [BackupEntry],
    /// The checkpoints on Google Drive, from every computer (empty when not connected).
    pub drive: &'a [CheckpointInfo],
    /// This computer's id on Drive, to say which checkpoints it made.
    pub this_device_id: &'a str,
    pub now: OffsetDateTime,
    /// The user's offset from UTC, in minutes, for what counts as a day.
    pub utc_offset_minutes: i32,
    /// How many automatic backups are kept.
    pub keep_auto: usize,
    pub auto_backup: bool,
    pub drive_connected: bool,
}

/// The user's offset, clamped to what a clock can have.
fn offset_of(minutes: i32) -> UtcOffset {
    UtcOffset::from_whole_seconds(minutes.clamp(-14 * 60, 14 * 60) * 60).unwrap_or(UtcOffset::UTC)
}

/// Why nothing is recent, given what is switched on.
fn stale_reason(auto_backup: bool, drive_connected: bool) -> StaleReason {
    if auto_backup {
        StaleReason::AutoFailing
    } else if drive_connected {
        StaleReason::DriveOffline
    } else {
        StaleReason::AutoOff
    }
}

/// Whether `last` is too old to call the data safe.
pub fn stale(
    last: Option<OffsetDateTime>,
    now: OffsetDateTime,
    auto_backup: bool,
    drive_connected: bool,
) -> Option<StaleReason> {
    let old = last.is_none_or(|at| now - at > AUTO_INTERVAL);
    old.then(|| stale_reason(auto_backup, drive_connected))
}

/// All the save points, newest first, grouped by day.
pub fn merge(input: &TimelineInput) -> SaveTimeline {
    let mut points: Vec<SavePoint> = Vec::new();

    // ------------------------------------------------------------------ this computer
    let parsed: Vec<(&BackupEntry, minimap_types::BackupKind, OffsetDateTime)> = input
        .local
        .iter()
        .filter_map(|e| parse_name_full(&e.file_name).map(|p| (e, p.kind, p.at)))
        .collect();
    let plan_input: Vec<(String, BackupKind, OffsetDateTime)> = parsed
        .iter()
        .map(|(e, kind, at)| (e.file_name.clone(), *kind, *at))
        .collect();
    let doomed: HashSet<String> = prune_plan(&plan_input, input.keep_auto)
        .into_iter()
        .collect();
    for (entry, kind, at) in &parsed {
        points.push(SavePoint {
            source: SaveSource::Local {
                path: entry.path.clone(),
                file_name: entry.file_name.clone(),
                kind: *kind,
                label: entry.label.clone(),
                encrypted: entry.encrypted,
            },
            at: *at,
            bytes: entry.bytes,
            will_be_pruned: doomed.contains(&entry.file_name),
        });
    }

    // ------------------------------------------------------------------------- Drive
    // Each computer keeps its own checkpoints: the newest hourly ones, then one a day.
    let mut by_device: std::collections::BTreeMap<String, Vec<(String, OffsetDateTime)>> =
        Default::default();
    for c in input.drive {
        if let Some((device, at)) = parse_checkpoint_name(&c.name) {
            by_device
                .entry(device)
                .or_default()
                .push((c.name.clone(), at));
        }
    }
    for (device, files) in &by_device {
        let doomed: HashSet<String> =
            checkpoint_prune_plan(files, HOURLY_CHECKPOINTS_KEPT, DAILY_CHECKPOINTS_KEPT)
                .into_iter()
                .collect();
        let mut newest_first: Vec<&(String, OffsetDateTime)> = files.iter().collect();
        newest_first.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.cmp(&a.0)));
        for (rank, (name, at)) in newest_first.into_iter().enumerate() {
            let Some(info) = input.drive.iter().find(|c| &c.name == name) else {
                continue;
            };
            points.push(SavePoint {
                source: SaveSource::Drive {
                    name: name.clone(),
                    device_name: info.device_name.clone(),
                    this_device: device == input.this_device_id,
                    cadence: if rank < HOURLY_CHECKPOINTS_KEPT as usize {
                        CheckpointCadence::Hourly
                    } else {
                        CheckpointCadence::Daily
                    },
                },
                at: *at,
                bytes: info.bytes,
                will_be_pruned: doomed.contains(name),
            });
        }
    }

    // ----------------------------------------------------------- order and group by day
    points.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| key_of(b).cmp(key_of(a))));
    let offset = offset_of(input.utc_offset_minutes);
    let mut days: Vec<SaveDay> = Vec::new();
    for point in &points {
        let date = point.at.to_offset(offset).date();
        match days.last_mut() {
            Some(day) if day.date == date => day.points.push(point.clone()),
            _ => days.push(SaveDay {
                date,
                points: vec![point.clone()],
            }),
        }
    }
    let last = points.first().cloned();
    SaveTimeline {
        stale: stale(
            last.as_ref().map(|p| p.at),
            input.now,
            input.auto_backup,
            input.drive_connected,
        ),
        days,
        last,
        drive_note: None,
    }
}

/// A stable tie-break between two save points made in the same second.
fn key_of(p: &SavePoint) -> &str {
    match &p.source {
        SaveSource::Local { file_name, .. } => file_name,
        SaveSource::Drive { name, .. } => name,
    }
}

/// Minutes since `at` (never negative).
pub fn age_minutes(at: OffsetDateTime, now: OffsetDateTime) -> u64 {
    (now - at).whole_minutes().max(0) as u64
}

/// The newest of two optional times.
pub fn newest(a: Option<OffsetDateTime>, b: Option<OffsetDateTime>) -> Option<OffsetDateTime> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use time::macros::{date, datetime};
    use time::Duration;

    const NOW: OffsetDateTime = datetime!(2027-03-03 12:00 UTC);

    fn local(kind: BackupKind, at: OffsetDateTime, label: Option<&str>) -> BackupEntry {
        let name = crate::backup::file_name_with_label(kind, at, 6, label);
        BackupEntry {
            path: format!("/b/{name}"),
            file_name: name,
            kind,
            created: crate::backup::display_time(at),
            age_minutes: age_minutes(at, NOW),
            bytes: 1000,
            encrypted: false,
            label: label.map(str::to_owned),
        }
    }

    fn checkpoint(device: &str, at: OffsetDateTime) -> CheckpointInfo {
        CheckpointInfo {
            name: crate::sync::checkpoint_name(device, at),
            device_name: format!("{device}'s computer"),
            created: crate::backup::display_time(at),
            bytes: 5000,
        }
    }

    fn timeline(
        local: &[BackupEntry],
        drive: &[CheckpointInfo],
        tweak: impl FnOnce(&mut TimelineInput),
    ) -> SaveTimeline {
        let mut input = TimelineInput {
            local,
            drive,
            this_device_id: "device-aaaa",
            now: NOW,
            utc_offset_minutes: 0,
            keep_auto: 14,
            auto_backup: true,
            drive_connected: false,
        };
        tweak(&mut input);
        merge(&input)
    }

    fn names(t: &SaveTimeline) -> Vec<String> {
        t.days
            .iter()
            .flat_map(|d| d.points.iter())
            .map(|p| key_of(p).to_owned())
            .collect()
    }

    #[test]
    fn local_and_drive_points_are_interleaved_newest_first() {
        let l = [
            local(
                BackupKind::Manual,
                datetime!(2027-03-03 11:00 UTC),
                Some("before-q2"),
            ),
            local(BackupKind::Auto, datetime!(2027-03-03 09:00 UTC), None),
        ];
        let d = [
            checkpoint("device-aaaa", datetime!(2027-03-03 10:00 UTC)),
            checkpoint("device-bbbb", datetime!(2027-03-03 11:30 UTC)),
        ];
        let t = timeline(&l, &d, |i| i.drive_connected = true);
        let got = names(&t);
        assert_eq!(got.len(), 4);
        assert!(got[0].starts_with("device-bbbb-"), "{got:?}");
        assert!(got[1].starts_with("minimap-20270303-110000--before-q2"));
        assert!(got[2].starts_with("device-aaaa-"));
        assert!(got[3].starts_with("minimap-auto-"));
        assert_eq!(t.days.len(), 1);
        assert_eq!(t.last.as_ref().unwrap().at, datetime!(2027-03-03 11:30 UTC));
        assert_eq!(t.stale, None);
        // Which checkpoints are this computer's is said.
        let mine: Vec<bool> = t.days[0]
            .points
            .iter()
            .filter_map(|p| match &p.source {
                SaveSource::Drive { this_device, .. } => Some(*this_device),
                _ => None,
            })
            .collect();
        assert_eq!(mine, [false, true]);
    }

    #[test]
    fn days_are_the_users_own_calendar_days() {
        // 23:30 UTC on the 2nd is 01:30 on the 3rd at UTC+2, and 18:30 on the 2nd at UTC-5.
        let l = [
            local(BackupKind::Manual, datetime!(2027-03-02 23:30 UTC), None),
            local(BackupKind::Auto, datetime!(2027-03-03 08:00 UTC), None),
        ];
        let east = timeline(&l, &[], |i| i.utc_offset_minutes = 120);
        assert_eq!(east.days.len(), 1);
        assert_eq!(east.days[0].date, date!(2027 - 03 - 03));
        let west = timeline(&l, &[], |i| i.utc_offset_minutes = -300);
        let dates: Vec<_> = west.days.iter().map(|d| d.date).collect();
        assert_eq!(dates, [date!(2027 - 03 - 03), date!(2027 - 03 - 02)]);
        assert_eq!(west.days[1].points.len(), 1);
    }

    #[test]
    fn automatic_backups_beyond_the_newest_fourteen_are_marked_and_others_never_are() {
        let mut l: Vec<BackupEntry> = (0..16i64)
            .map(|d| {
                local(
                    BackupKind::Auto,
                    datetime!(2027-02-01 08:00 UTC) + Duration::days(d),
                    None,
                )
            })
            .collect();
        l.push(local(
            BackupKind::Manual,
            datetime!(2027-01-01 08:00 UTC),
            Some("old"),
        ));
        l.push(local(
            BackupKind::PreRestore,
            datetime!(2027-01-02 08:00 UTC),
            None,
        ));
        let t = timeline(&l, &[], |_| {});
        let pruned: Vec<String> = t
            .days
            .iter()
            .flat_map(|d| d.points.iter())
            .filter(|p| p.will_be_pruned)
            .map(|p| key_of(p).to_owned())
            .collect();
        // The same two the real prune would delete.
        let plan_input: Vec<_> = l
            .iter()
            .map(|e| {
                let p = parse_name_full(&e.file_name).unwrap();
                (e.file_name.clone(), p.kind, p.at)
            })
            .collect();
        let mut want = prune_plan(&plan_input, 14);
        want.sort();
        let mut got = pruned.clone();
        got.sort();
        assert_eq!(got, want);
        assert_eq!(got.len(), 2);
        assert!(got.iter().all(|n| n.contains("auto")));
    }

    #[test]
    fn drive_checkpoints_say_hourly_or_daily_and_the_ones_about_to_go() {
        // One computer: 30 hourly checkpoints (24 kept as hourly) and old ones from other days.
        let mut d: Vec<CheckpointInfo> = (0..30i64)
            .map(|h| checkpoint("device-aaaa", NOW - Duration::hours(h)))
            .collect();
        d.push(checkpoint("device-aaaa", NOW - Duration::days(5)));
        let t = timeline(&[], &d, |i| i.drive_connected = true);
        let drive: Vec<&SavePoint> = t.days.iter().flat_map(|d| d.points.iter()).collect();
        assert_eq!(drive.len(), 31);
        let cadence = |p: &SavePoint| match &p.source {
            SaveSource::Drive { cadence, .. } => *cadence,
            _ => panic!(),
        };
        assert_eq!(
            drive
                .iter()
                .filter(|p| cadence(p) == CheckpointCadence::Hourly)
                .count(),
            24
        );
        // The prune flags are exactly what the real plan deletes.
        let files: Vec<(String, OffsetDateTime)> = d
            .iter()
            .map(|c| (c.name.clone(), parse_checkpoint_name(&c.name).unwrap().1))
            .collect();
        let mut want =
            checkpoint_prune_plan(&files, HOURLY_CHECKPOINTS_KEPT, DAILY_CHECKPOINTS_KEPT);
        want.sort();
        let mut got: Vec<String> = drive
            .iter()
            .filter(|p| p.will_be_pruned)
            .map(|p| key_of(p).to_owned())
            .collect();
        got.sort();
        assert_eq!(got, want);
        assert!(!got.is_empty());
    }

    #[test]
    fn it_is_stale_after_a_day_and_says_why() {
        let old = [local(BackupKind::Auto, NOW - Duration::hours(25), None)];
        let fresh = [local(BackupKind::Auto, NOW - Duration::hours(24), None)];
        assert_eq!(
            timeline(&fresh, &[], |_| {}).stale,
            None,
            "exactly a day is not over"
        );
        // Automatic backups on but nothing for a day.
        assert_eq!(
            timeline(&old, &[], |_| {}).stale,
            Some(StaleReason::AutoFailing)
        );
        // Off and no Drive.
        assert_eq!(
            timeline(&old, &[], |i| i.auto_backup = false).stale,
            Some(StaleReason::AutoOff)
        );
        // Off, with Drive connected but nothing on it or here.
        assert_eq!(
            timeline(&old, &[], |i| {
                i.auto_backup = false;
                i.drive_connected = true;
            })
            .stale,
            Some(StaleReason::DriveOffline)
        );
        // A recent Drive checkpoint is a save point too.
        let d = [checkpoint("device-aaaa", NOW - Duration::hours(1))];
        assert_eq!(
            timeline(&old, &d, |i| {
                i.auto_backup = false;
                i.drive_connected = true;
            })
            .stale,
            None
        );
        // None at all.
        let none = timeline(&[], &[], |i| i.auto_backup = false);
        assert_eq!(none.stale, Some(StaleReason::AutoOff));
        assert!(none.last.is_none() && none.days.is_empty());
    }

    #[test]
    fn files_that_are_not_minimaps_are_left_out_and_names_carry_the_label() {
        let mut odd = local(BackupKind::Manual, NOW, None);
        odd.file_name = "holiday.db".into();
        let named = local(
            BackupKind::Manual,
            NOW - Duration::hours(1),
            Some("before-q2"),
        );
        let t = timeline(&[odd, named], &[], |_| {});
        assert_eq!(names(&t).len(), 1);
        match &t.last.as_ref().unwrap().source {
            SaveSource::Local { label, kind, .. } => {
                assert_eq!(label.as_deref(), Some("before-q2"));
                assert_eq!(*kind, BackupKind::Manual);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn age_and_newest_are_small_helpers() {
        assert_eq!(age_minutes(NOW - Duration::minutes(90), NOW), 90);
        assert_eq!(age_minutes(NOW + Duration::minutes(5), NOW), 0);
        assert_eq!(newest(Some(NOW), None), Some(NOW));
        assert_eq!(newest(Some(NOW), Some(NOW - Duration::hours(1))), Some(NOW));
        assert_eq!(newest(None, None), None);
    }

    proptest! {
        /// Every point lands in exactly one day, days are newest first, and so are the points.
        #[test]
        fn every_point_is_in_one_day_in_order(
            hours in proptest::collection::vec(0i64..2000, 0..25),
            offset in -720i32..840,
        ) {
            let l: Vec<BackupEntry> = hours
                .iter()
                .map(|h| local(BackupKind::Manual, NOW - Duration::hours(*h) - Duration::seconds(*h), None))
                .collect();
            let t = timeline(&l, &[], |i| i.utc_offset_minutes = offset);
            let total: usize = t.days.iter().map(|d| d.points.len()).sum();
            prop_assert_eq!(total, l.len());
            prop_assert!(t.days.windows(2).all(|w| w[0].date > w[1].date));
            let flat: Vec<OffsetDateTime> = t.days.iter().flat_map(|d| d.points.iter()).map(|p| p.at).collect();
            prop_assert!(flat.windows(2).all(|w| w[0] >= w[1]));
            prop_assert_eq!(t.last.map(|p| p.at), flat.first().copied());
        }
    }
}
