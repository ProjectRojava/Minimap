//! Meetings (spec 38): the rules of the built-in "meeting" kind of task. Pure.
//!
//! A meeting is a task with a day (`due_date`), a start (`start_minute`, the clock on the user's
//! wall) and a length. It moves itself: in progress when it starts, done when it ends. Times are
//! floating ("10:30" means 10:30 wherever the user is), so a weekly meeting keeps its hour across
//! summer time and the same rule gives the same answer on every device.

use minimap_types::{Clock, Date, Task, TaskStatus, Uuid, MAX_MEETING_MINUTES};
use time::{Duration, OffsetDateTime, PrimitiveDateTime, Time};

use minimap_types::absolute_minutes as absolute;

/// The rules for the time fields of a task. A meeting needs a day and a time; any task's time
/// must be a real minute of the day and its length at most a day.
pub fn validate(
    is_meeting: bool,
    due: Option<Date>,
    start_minute: Option<u16>,
    length_minutes: Option<u32>,
) -> Result<(), String> {
    if start_minute.is_some_and(|m| m >= 1440) {
        return Err("A start time is a time of day, from 00:00 to 23:59.".into());
    }
    if length_minutes.is_some_and(|l| l == 0 || l > MAX_MEETING_MINUTES) {
        return Err(format!(
            "A meeting lasts from 1 minute to {} hours.",
            MAX_MEETING_MINUTES / 60
        ));
    }
    if is_meeting && (due.is_none() || start_minute.is_none()) {
        return Err("A meeting needs a date and a start time.".into());
    }
    Ok(())
}

/// When a meeting starts and ends, in wall-clock minutes (see [`absolute`]), if it has a time.
fn window(task: &Task) -> Option<(i64, i64)> {
    let start = absolute(task.due_date?, task.start_minute?);
    Some((start, start + i64::from(task.meeting_minutes())))
}

/// Where a meeting has got to by `now` and the status that goes with it, when that differs from
/// the one it has. A meeting that is to do moves to in progress when it starts, and to done when
/// it ends (a meeting that ended while nobody was looking goes straight to done); one in progress
/// moves to done when it ends. A blocked, done or cancelled meeting is left alone, and so is a
/// task that is not a meeting.
pub fn next_status(task: &Task, now: &Clock) -> Option<TaskStatus> {
    if !task.is_meeting() {
        return None;
    }
    let (start, end) = window(task)?;
    let now = absolute(now.date, now.minute);
    match task.status {
        TaskStatus::Todo if now >= end => Some(TaskStatus::Done),
        TaskStatus::Todo if now >= start => Some(TaskStatus::InProgress),
        TaskStatus::InProgress if now >= end => Some(TaskStatus::Done),
        _ => None,
    }
}

/// A meeting that has started or is done, moved to a time that has not come, is open again
/// (`task` already has the new day and time). The new status, if it changes.
pub fn reopens_when_moved(status: TaskStatus, task: &Task, now: &Clock) -> bool {
    matches!(status, TaskStatus::InProgress | TaskStatus::Done)
        && window(task).is_some_and(|(start, _)| start > absolute(now.date, now.minute))
}

/// The instant a meeting ended, for `completed_at` (so a meeting that finished while the app was
/// closed is done when it ended, not when the app was next opened).
pub fn ended_at(task: &Task, utc_offset_minutes: i32) -> Option<OffsetDateTime> {
    let (_, end) = window(task)?;
    let utc = end - i64::from(utc_offset_minutes);
    let date = Date::from_julian_day(i32::try_from(utc.div_euclid(1440)).ok()?).ok()?;
    let minute = utc.rem_euclid(1440);
    let time = Time::from_hms((minute / 60) as u8, (minute % 60) as u8, 0).ok()?;
    Some(PrimitiveDateTime::new(date, time).assume_utc())
}

/// Where a follow-up to this meeting goes unless the user says otherwise: the same time on the
/// same weekday next week.
pub fn follow_up_slot(task: &Task) -> Option<(Date, u16)> {
    Some((task.due_date? + Duration::days(7), task.start_minute?))
}

/// The title a follow-up starts with.
pub fn follow_up_title(title: &str) -> String {
    const PREFIX: &str = "Follow-up: ";
    if title.starts_with(PREFIX) {
        title.to_owned()
    } else {
        format!("{PREFIX}{title}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FollowUpError {
    /// The meeting already follows up on another.
    AlreadyFollows(Uuid),
    /// The links would go round in a circle.
    Circle,
}

impl FollowUpError {
    /// The sentence for the user; `other` names the meeting it already follows up on.
    pub fn message(&self, follow_up: &str, original: &str, other: &str) -> String {
        match self {
            Self::AlreadyFollows(_) => format!(
                "“{follow_up}” already follows up on “{other}”. A meeting follows up on one meeting: remove that link first."
            ),
            Self::Circle => format!(
                "“{original}” already comes after “{follow_up}”, so “{follow_up}” can't follow up on it."
            ),
        }
    }
}

/// May `follow_up` follow up on `original`, given the existing `(follow_up, original)` links? A
/// meeting follows up on one meeting and has any number of follow-ups; the chain never loops. A
/// link that already exists passes (the store reports the duplicate).
pub fn check_follow_up(
    existing: &[(Uuid, Uuid)],
    follow_up: Uuid,
    original: Uuid,
) -> Result<(), FollowUpError> {
    if existing.contains(&(follow_up, original)) {
        return Ok(());
    }
    if let Some((_, current)) = existing.iter().find(|(f, _)| *f == follow_up) {
        return Err(FollowUpError::AlreadyFollows(*current));
    }
    // Walk back from the original: if it leads to the new follow-up, this would close a loop.
    let mut at = original;
    for _ in 0..=existing.len() {
        if at == follow_up {
            return Err(FollowUpError::Circle);
        }
        match existing.iter().find(|(f, _)| *f == at) {
            Some((_, before)) => at = *before,
            None => break,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::MEETING_TYPE;
    use time::macros::{date, datetime};

    fn meeting(status: TaskStatus, due: Date, start: u16, length: Option<u32>) -> Task {
        Task {
            links: Vec::new(),
            task_type: Some(MEETING_TYPE.into()),
            focus: None,
            start_minute: Some(start),
            length_minutes: length,
            id: Uuid::nil(),
            title: "Sync".into(),
            description: String::new(),
            project_id: None,
            status,
            estimate_days: None,
            start_date: None,
            due_date: Some(due),
            completed_at: None,
            priority: 3,
            recurrence: None,
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
            archived_at: None,
        }
    }

    fn at(date: Date, minute: u16) -> Clock {
        Clock {
            date,
            minute,
            utc_offset_minutes: 0,
        }
    }

    const DAY: Date = date!(2027 - 03 - 03);
    const TEN_THIRTY: u16 = 630;

    #[test]
    fn a_meeting_starts_and_ends_on_the_clock() {
        let m = |s| meeting(s, DAY, TEN_THIRTY, None);
        // Before it starts: nothing.
        assert_eq!(
            next_status(&m(TaskStatus::Todo), &at(DAY, TEN_THIRTY - 1)),
            None
        );
        // At the start: in progress.
        assert_eq!(
            next_status(&m(TaskStatus::Todo), &at(DAY, TEN_THIRTY)),
            Some(TaskStatus::InProgress)
        );
        assert_eq!(
            next_status(&m(TaskStatus::InProgress), &at(DAY, TEN_THIRTY + 59)),
            None
        );
        // An hour later: done.
        assert_eq!(
            next_status(&m(TaskStatus::InProgress), &at(DAY, TEN_THIRTY + 60)),
            Some(TaskStatus::Done)
        );
        // Missed altogether (the app was closed): straight to done.
        assert_eq!(
            next_status(&m(TaskStatus::Todo), &at(DAY, TEN_THIRTY + 61)),
            Some(TaskStatus::Done)
        );
        assert_eq!(
            next_status(&m(TaskStatus::Todo), &at(date!(2027 - 03 - 10), 0)),
            Some(TaskStatus::Done)
        );
    }

    #[test]
    fn the_length_is_the_meetings_own_and_defaults_to_an_hour() {
        let short = meeting(TaskStatus::InProgress, DAY, TEN_THIRTY, Some(15));
        assert_eq!(next_status(&short, &at(DAY, TEN_THIRTY + 14)), None);
        assert_eq!(
            next_status(&short, &at(DAY, TEN_THIRTY + 15)),
            Some(TaskStatus::Done)
        );
        let long = meeting(TaskStatus::InProgress, DAY, TEN_THIRTY, Some(180));
        assert_eq!(next_status(&long, &at(DAY, TEN_THIRTY + 179)), None);
        assert_eq!(
            next_status(&long, &at(DAY, TEN_THIRTY + 180)),
            Some(TaskStatus::Done)
        );
    }

    #[test]
    fn a_meeting_over_midnight_ends_the_next_day() {
        let late = meeting(TaskStatus::InProgress, DAY, 23 * 60 + 30, Some(60));
        assert_eq!(next_status(&late, &at(DAY, 23 * 60 + 59)), None);
        assert_eq!(next_status(&late, &at(date!(2027 - 03 - 04), 29)), None);
        assert_eq!(
            next_status(&late, &at(date!(2027 - 03 - 04), 30)),
            Some(TaskStatus::Done)
        );
    }

    #[test]
    fn other_statuses_and_other_tasks_are_left_alone() {
        let way_after = at(date!(2030 - 01 - 01), 0);
        for s in [TaskStatus::Blocked, TaskStatus::Done, TaskStatus::Cancelled] {
            assert_eq!(
                next_status(&meeting(s, DAY, TEN_THIRTY, None), &way_after),
                None,
                "{s}"
            );
        }
        let mut plain = meeting(TaskStatus::Todo, DAY, TEN_THIRTY, None);
        plain.task_type = Some("design".into());
        assert_eq!(next_status(&plain, &way_after), None);
        // A meeting without a time (not possible through the store) is left alone too.
        let mut timeless = meeting(TaskStatus::Todo, DAY, TEN_THIRTY, None);
        timeless.start_minute = None;
        assert_eq!(next_status(&timeless, &way_after), None);
        // Not started yet, and in progress before its time (dragged early): left alone.
        let early = meeting(TaskStatus::InProgress, DAY, TEN_THIRTY, None);
        assert_eq!(next_status(&early, &at(DAY, 0)), None);
    }

    #[test]
    fn a_meeting_ended_at_the_instant_it_ended() {
        let m = meeting(TaskStatus::InProgress, DAY, TEN_THIRTY, Some(60));
        assert_eq!(ended_at(&m, 0), Some(datetime!(2027-03-03 11:30 UTC)));
        // India, 5h30 ahead: 11:30 there is 06:00 UTC.
        assert_eq!(ended_at(&m, 330), Some(datetime!(2027-03-03 06:00 UTC)));
        // New York in winter, 5h behind.
        assert_eq!(ended_at(&m, -300), Some(datetime!(2027-03-03 16:30 UTC)));
        // Across midnight in UTC.
        let late = meeting(TaskStatus::InProgress, DAY, 23 * 60, Some(60));
        assert_eq!(ended_at(&late, -60), Some(datetime!(2027-03-04 01:00 UTC)));
    }

    #[test]
    fn a_meeting_needs_a_day_and_a_time() {
        assert_eq!(validate(true, Some(DAY), Some(600), None), Ok(()));
        assert_eq!(validate(true, Some(DAY), Some(600), Some(30)), Ok(()));
        assert!(validate(true, None, Some(600), None)
            .unwrap_err()
            .contains("date"));
        assert!(validate(true, Some(DAY), None, None)
            .unwrap_err()
            .contains("start time"));
        // Other tasks need neither, but a time that is given must be real.
        assert_eq!(validate(false, None, None, None), Ok(()));
        assert!(validate(false, None, Some(1440), None).is_err());
        assert!(validate(true, Some(DAY), Some(600), Some(0)).is_err());
        assert!(validate(true, Some(DAY), Some(600), Some(1441)).is_err());
    }

    #[test]
    fn a_follow_up_is_a_week_later_at_the_same_time() {
        let m = meeting(TaskStatus::Done, DAY, TEN_THIRTY, None);
        assert_eq!(
            follow_up_slot(&m),
            Some((date!(2027 - 03 - 10), TEN_THIRTY))
        );
        assert_eq!(follow_up_title("Q3 plan"), "Follow-up: Q3 plan");
        assert_eq!(follow_up_title("Follow-up: Q3 plan"), "Follow-up: Q3 plan");
    }

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    #[test]
    fn a_meeting_has_one_original_and_any_number_of_follow_ups() {
        // 2 and 3 both follow up on 1.
        let existing = [(id(2), id(1)), (id(3), id(1))];
        assert_eq!(check_follow_up(&existing, id(4), id(1)), Ok(()));
        assert_eq!(check_follow_up(&existing, id(4), id(2)), Ok(()));
        // 2 already follows up on 1.
        assert_eq!(
            check_follow_up(&existing, id(2), id(5)),
            Err(FollowUpError::AlreadyFollows(id(1)))
        );
        // The link that exists passes.
        assert_eq!(check_follow_up(&existing, id(2), id(1)), Ok(()));
    }

    #[test]
    fn a_chain_of_follow_ups_never_loops() {
        // 3 follows 2 follows 1.
        let existing = [(id(2), id(1)), (id(3), id(2))];
        assert_eq!(
            check_follow_up(&existing, id(1), id(3)),
            Err(FollowUpError::Circle)
        );
        assert_eq!(
            check_follow_up(&existing, id(1), id(2)),
            Err(FollowUpError::Circle)
        );
        assert_eq!(check_follow_up(&existing, id(4), id(3)), Ok(()));
        let msg = FollowUpError::Circle.message("A", "B", "");
        assert!(msg.contains('A') && msg.contains('B'));
    }
}
