//! The safe zone (spec 44): the room a project has, in working days, between when it will really
//! finish (the schedule's projected finish) and when it has to (its target date). Pure.
//!
//! It is a view of the same schedule run that health reads, so the two can never disagree about the
//! dates; it does not feed health (health already judges lateness).

use minimap_types::{Date, ProjectForecast, ProjectStatus, WorkWeek, Zone, ZoneLevel};

use crate::schedule::{end_index, working_index};

/// The zone of a project. `forecast` is the project's entry of the schedule.
///
/// - Done, cancelled and paused projects, and projects with no tasks, show nothing (a paused
///   project's clock is not running). Planned and active ones are worked out.
/// - A planned or active project whose tasks are all done is `AllDone` (with the day its last
///   task finished), so a finished project does not look like one nobody has planned.
/// - `remaining` is the working days from today (inclusive) to the projected finish (inclusive).
/// - Inside: `room` is the working days strictly after the projected finish up to and including the
///   target (a target on a day off counts up to the working day before it). Equal days: no room.
/// - Outside: `over` is the forecast's own `late_by_days`, so the two always agree.
pub fn of(
    forecast: &ProjectForecast,
    status: ProjectStatus,
    today: Date,
    week: WorkWeek,
    narrow_days: u32,
) -> Zone {
    if matches!(
        status,
        ProjectStatus::Done | ProjectStatus::Cancelled | ProjectStatus::Paused
    ) {
        return Zone::None;
    }
    let Some(finish) = forecast.projected_finish else {
        return Zone::None;
    };
    if forecast.open_tasks == 0 {
        return Zone::AllDone { finished: finish };
    }
    let finish_end = end_index(week, finish);
    let remaining = (finish_end - working_index(week, today)).max(0) as u32;
    match (forecast.target_date, forecast.late_by_days) {
        (None, _) => Zone::NoTarget { remaining },
        (Some(_), Some(over)) => Zone::Outside {
            remaining,
            over: over.max(1),
        },
        (Some(target), None) => {
            let room = (end_index(week, target) - finish_end).max(0) as u32;
            let level = if room == 0 {
                ZoneLevel::Closing
            } else if room <= narrow_days {
                ZoneLevel::Narrow
            } else {
                ZoneLevel::Wide
            };
            Zone::Inside {
                remaining,
                room,
                level,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::Uuid;
    use proptest::prelude::*;
    use time::{macros::date, Duration};

    // Monday 2027-03-01 .. Friday 03-05, Monday 03-08 ..
    const MON: Date = date!(2027 - 03 - 01);

    fn forecast(
        finish: Option<Date>,
        target: Option<Date>,
        late: Option<u32>,
        open: u32,
    ) -> ProjectForecast {
        ProjectForecast {
            project_id: Uuid::nil(),
            title: "p".into(),
            projected_finish: finish,
            finish_offset: None,
            target_date: target,
            target_offset: None,
            late_by_days: late,
            open_tasks: open,
            unestimated_tasks: 0,
            critical_tasks: 0,
        }
    }

    fn zone(f: &ProjectForecast) -> Zone {
        of(f, ProjectStatus::Active, MON, WorkWeek::MON_FRI, 3)
    }

    #[test]
    fn nothing_to_show_without_open_work_or_for_closed_and_paused_projects() {
        let f = forecast(
            Some(date!(2027 - 03 - 03)),
            Some(date!(2027 - 03 - 10)),
            None,
            2,
        );
        assert!(matches!(zone(&f), Zone::Inside { .. }));
        for status in [
            ProjectStatus::Done,
            ProjectStatus::Cancelled,
            ProjectStatus::Paused,
        ] {
            assert_eq!(of(&f, status, MON, WorkWeek::MON_FRI, 3), Zone::None);
        }
        // Planned is worked out like active.
        assert!(matches!(
            of(&f, ProjectStatus::Planned, MON, WorkWeek::MON_FRI, 3),
            Zone::Inside { .. }
        ));
        // No tasks at all (so no forecast date): nothing.
        let no_tasks = forecast(None, Some(date!(2027 - 03 - 10)), None, 0);
        assert_eq!(zone(&no_tasks), Zone::None);
        let no_finish = forecast(None, Some(date!(2027 - 03 - 10)), None, 1);
        assert_eq!(zone(&no_finish), Zone::None);
    }

    #[test]
    fn a_project_whose_tasks_are_all_done_says_so_and_when() {
        // All done: no open tasks, the last one finished on Thursday 2027-02-25.
        let f = forecast(
            Some(date!(2027 - 02 - 25)),
            Some(date!(2027 - 03 - 10)),
            None,
            0,
        );
        let all_done = Zone::AllDone {
            finished: date!(2027 - 02 - 25),
        };
        assert_eq!(zone(&f), all_done);
        // With or without a target, and whether planned or active.
        let no_target = forecast(Some(date!(2027 - 02 - 25)), None, None, 0);
        assert_eq!(zone(&no_target), all_done);
        assert_eq!(
            of(&f, ProjectStatus::Planned, MON, WorkWeek::MON_FRI, 3),
            all_done
        );
        // But not once the project itself is closed or paused.
        for status in [
            ProjectStatus::Done,
            ProjectStatus::Cancelled,
            ProjectStatus::Paused,
        ] {
            assert_eq!(of(&f, status, MON, WorkWeek::MON_FRI, 3), Zone::None);
        }
    }

    #[test]
    fn open_work_without_a_target_says_how_much_is_ahead() {
        // Monday to Wednesday inclusive: 3 working days.
        let f = forecast(Some(date!(2027 - 03 - 03)), None, None, 2);
        assert_eq!(zone(&f), Zone::NoTarget { remaining: 3 });
    }

    #[test]
    fn room_counts_the_working_days_after_the_finish_up_to_the_target() {
        // Finishes Wednesday 03-03, target Wednesday 03-10: Thu, Fri, Mon, Tue, Wed = 5.
        let f = forecast(
            Some(date!(2027 - 03 - 03)),
            Some(date!(2027 - 03 - 10)),
            None,
            1,
        );
        assert_eq!(
            zone(&f),
            Zone::Inside {
                remaining: 3,
                room: 5,
                level: ZoneLevel::Wide
            }
        );
    }

    #[test]
    fn the_level_changes_at_zero_one_three_and_four_days_of_room() {
        let level = |target: Date| match zone(&forecast(
            Some(date!(2027 - 03 - 03)),
            Some(target),
            None,
            1,
        )) {
            Zone::Inside { room, level, .. } => (room, level),
            other => panic!("{other:?}"),
        };
        assert_eq!(level(date!(2027 - 03 - 03)), (0, ZoneLevel::Closing));
        assert_eq!(level(date!(2027 - 03 - 04)), (1, ZoneLevel::Narrow));
        assert_eq!(level(date!(2027 - 03 - 08)), (3, ZoneLevel::Narrow));
        assert_eq!(level(date!(2027 - 03 - 09)), (4, ZoneLevel::Wide));
    }

    #[test]
    fn a_weekend_between_the_finish_and_the_target_is_not_room() {
        // Finishes Friday, target the next Monday: only Monday is room.
        let f = forecast(
            Some(date!(2027 - 03 - 05)),
            Some(date!(2027 - 03 - 08)),
            None,
            1,
        );
        assert!(matches!(zone(&f), Zone::Inside { room: 1, .. }));
        // A target on the Sunday gives the same room as the Friday: none.
        let f = forecast(
            Some(date!(2027 - 03 - 05)),
            Some(date!(2027 - 03 - 07)),
            None,
            1,
        );
        assert!(matches!(
            zone(&f),
            Zone::Inside {
                room: 0,
                level: ZoneLevel::Closing,
                ..
            }
        ));
    }

    #[test]
    fn a_finish_after_the_target_is_outside_by_the_forecasts_own_count() {
        let f = forecast(
            Some(date!(2027 - 03 - 08)),
            Some(date!(2027 - 03 - 05)),
            Some(1),
            1,
        );
        assert_eq!(
            zone(&f),
            Zone::Outside {
                remaining: 6,
                over: 1
            }
        );
        // Never zero days over, whatever the forecast said.
        let f = forecast(
            Some(date!(2027 - 03 - 08)),
            Some(date!(2027 - 03 - 05)),
            Some(0),
            1,
        );
        assert!(matches!(zone(&f), Zone::Outside { over: 1, .. }));
    }

    #[test]
    fn a_finish_today_or_earlier_has_nothing_remaining() {
        let f = forecast(
            Some(date!(2027 - 02 - 26)),
            Some(date!(2027 - 03 - 05)),
            None,
            1,
        );
        assert!(matches!(zone(&f), Zone::Inside { remaining: 0, .. }));
    }

    #[test]
    fn a_work_week_that_is_not_monday_to_friday_counts_its_own_days() {
        // Sunday to Thursday: days 6, 0, 1, 2, 3 (Monday = 0).
        let week = WorkWeek::from_days(&[6, 0, 1, 2, 3]).unwrap();
        // Finishes Thursday 03-04; target Monday 03-08: Sunday 03-07 and Monday 03-08 are room
        // (Friday and Saturday are days off).
        let f = forecast(
            Some(date!(2027 - 03 - 04)),
            Some(date!(2027 - 03 - 08)),
            None,
            1,
        );
        assert!(matches!(
            of(&f, ProjectStatus::Active, MON, week, 3),
            Zone::Inside { room: 2, .. }
        ));
        // Today Monday to the Thursday finish: Mon, Tue, Wed, Thu.
        assert!(matches!(
            of(&f, ProjectStatus::Active, MON, week, 3),
            Zone::Inside { remaining: 4, .. }
        ));
    }

    #[test]
    fn a_narrow_setting_of_zero_leaves_only_no_room_as_narrow() {
        let f = forecast(
            Some(date!(2027 - 03 - 03)),
            Some(date!(2027 - 03 - 04)),
            None,
            1,
        );
        assert!(matches!(
            of(&f, ProjectStatus::Active, MON, WorkWeek::MON_FRI, 0),
            Zone::Inside {
                level: ZoneLevel::Wide,
                ..
            }
        ));
    }

    fn working_days_between(week: WorkWeek, from_excl: Date, to_incl: Date) -> i64 {
        let mut n = 0;
        let mut d = from_excl;
        while d < to_incl {
            d += Duration::days(1);
            if week.contains(d.weekday().number_days_from_monday()) {
                n += 1;
            }
        }
        n
    }

    proptest! {
        // With a target, exactly one of Inside / Outside holds, and room / over are the working
        // days between the finish and the target with the sign flipped.
        #[test]
        fn inside_or_outside_and_the_numbers_match_the_dates(
            finish_off in 0i64..60,
            target_off in 0i64..60,
            narrow in 0u32..10,
        ) {
            let week = WorkWeek::MON_FRI;
            // The finish is a working day (a task's last day).
            let mut finish = MON + Duration::days(finish_off);
            while !week.contains(finish.weekday().number_days_from_monday()) {
                finish += Duration::days(1);
            }
            let target = MON + Duration::days(target_off);
            let (e_f, e_t) = (end_index(week, finish), end_index(week, target));
            let late = (e_f > e_t).then(|| (e_f - e_t) as u32);
            let f = forecast(Some(finish), Some(target), late, 1);
            match of(&f, ProjectStatus::Active, MON, week, narrow) {
                Zone::Inside { room, level, remaining } => {
                    prop_assert!(late.is_none());
                    prop_assert_eq!(i64::from(room), working_days_between(week, finish, target).max(0));
                    prop_assert_eq!(level == ZoneLevel::Closing, room == 0);
                    prop_assert_eq!(level == ZoneLevel::Narrow, room >= 1 && room <= narrow);
                    prop_assert!(remaining >= 1);
                }
                Zone::Outside { over, .. } => {
                    prop_assert!(late.is_some());
                    prop_assert_eq!(i64::from(over), working_days_between(week, target, finish));
                    prop_assert!(over >= 1);
                }
                other => prop_assert!(false, "{other:?}"),
            }
        }
    }
}
