//! Meetings (spec 38): a built-in kind of task with a day, a time and a length that moves itself
//! on with the clock, and follow-ups that link one meeting to the next.

use minimap_store::*;
use minimap_types::*;
use time::macros::{date, datetime};
use uuid::Uuid;

fn db() -> Connection {
    open_in_memory().unwrap()
}

const DAY: Date = date!(2027 - 03 - 03);
const TEN_THIRTY: u16 = 630;

fn clock(date: Date, minute: u16) -> Clock {
    Clock {
        date,
        minute,
        utc_offset_minutes: 0,
    }
}

fn meeting(conn: &mut Connection, title: &str) -> Task {
    meetings::create(
        conn,
        title.into(),
        DAY,
        TEN_THIRTY,
        None,
        None,
        AssigneeChoice::Nobody,
    )
    .unwrap()
}

fn plain_task(conn: &mut Connection, title: &str) -> Task {
    tasks::create(
        conn,
        CreateTask {
            links: Vec::new(),
            task_type: None,
            focus: None,
            start_minute: None,
            length_minutes: None,
            title: title.into(),
            assignee: AssigneeChoice::Nobody,
            description: String::new(),
            project_id: None,
            status: None,
            estimate_days: None,
            start_date: None,
            due_date: None,
            priority: None,
            recurrence: None,
        },
    )
    .unwrap()
}

fn follows(conn: &Connection) -> Vec<(Uuid, Uuid)> {
    edges::list_active_of_type(conn, EdgeType::FollowsUp)
        .unwrap()
        .into_iter()
        .map(|e| (e.from_id, e.to_id))
        .collect()
}

#[test]
fn the_meeting_type_is_always_in_the_list_and_cannot_be_retired() {
    let mut conn = db();
    let types = settings::task_types(&conn).unwrap();
    assert!(TaskType::find(&types, MEETING_TYPE).is_some_and(|t| !t.archived));
    // Archiving it in an edit does not stick; renaming it does.
    let mut wanted = types.clone();
    let m = wanted.iter_mut().find(|t| t.id == MEETING_TYPE).unwrap();
    m.archived = true;
    m.name = "Call".into();
    let saved = settings::update(
        &mut conn,
        UpdateSettings {
            task_types: Some(wanted),
            ..Default::default()
        },
    )
    .unwrap();
    let m = TaskType::find(&saved.task_types, MEETING_TYPE).unwrap();
    assert!(!m.archived);
    assert_eq!(m.name, "Call");
    // A list stored without it (an older installation) has it back when read.
    conn.execute(
        "UPDATE settings SET value = ?1 WHERE key = 'task_types'",
        [r#"[{"id":"design","name":"Design","hue":285}]"#],
    )
    .unwrap();
    let types = settings::task_types(&conn).unwrap();
    assert_eq!(
        types.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
        ["design", MEETING_TYPE]
    );
}

#[test]
fn a_meeting_needs_a_day_and_a_time_however_it_is_made_or_changed() {
    let mut conn = db();
    let m = meeting(&mut conn, "Weekly sync");
    assert_eq!((m.due_date, m.start_minute), (Some(DAY), Some(TEN_THIRTY)));
    assert!(m.is_meeting());
    // Clearing the day or the time through a plain update is refused and writes nothing.
    for patch in [
        UpdateTask {
            due_date: Patch::Clear,
            ..Default::default()
        },
        UpdateTask {
            start_minute: Patch::Clear,
            ..Default::default()
        },
    ] {
        let err = tasks::update(&mut conn, m.id, patch).unwrap_err();
        assert!(
            err.to_string().contains("needs a date and a start time"),
            "{err}"
        );
    }
    assert_eq!(
        tasks::get(&conn, m.id).unwrap().start_minute,
        Some(TEN_THIRTY)
    );
    // A task cannot become a meeting by changing its type alone.
    let t = plain_task(&mut conn, "Plain");
    let err = tasks::update(
        &mut conn,
        t.id,
        UpdateTask {
            task_type: Patch::Set(MEETING_TYPE.into()),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(err.to_string().contains("needs a date"), "{err}");
    // Nor can one be created without a time.
    let err = tasks::create(
        &mut conn,
        CreateTask {
            links: Vec::new(),
            task_type: Some(MEETING_TYPE.into()),
            focus: None,
            start_minute: None,
            length_minutes: None,
            title: "No time".into(),
            assignee: AssigneeChoice::Nobody,
            description: String::new(),
            project_id: None,
            status: None,
            estimate_days: None,
            start_date: None,
            due_date: Some(DAY),
            priority: None,
            recurrence: None,
        },
    )
    .unwrap_err();
    assert!(err.to_string().contains("start time"), "{err}");
    // The time of day has to be a real one.
    assert!(meetings::set_time(&mut conn, t.id, DAY, 1440, None, clock(DAY, 0)).is_err());
}

#[test]
fn making_a_task_a_meeting_sets_all_of_it_and_leaving_the_type_drops_the_time() {
    let mut conn = db();
    let t = plain_task(&mut conn, "Plain");
    let m = meetings::set_time(&mut conn, t.id, DAY, TEN_THIRTY, Some(45), clock(DAY, 0)).unwrap();
    assert!(m.is_meeting());
    assert_eq!(
        (m.due_date, m.start_minute, m.length_minutes),
        (Some(DAY), Some(TEN_THIRTY), Some(45))
    );
    let back = tasks::update(
        &mut conn,
        m.id,
        UpdateTask {
            task_type: Patch::Set("design".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!back.is_meeting());
    assert_eq!((back.start_minute, back.length_minutes), (None, None));
    // The day stays: it is an ordinary due date again.
    assert_eq!(back.due_date, Some(DAY));
}

#[test]
fn the_clock_starts_a_meeting_and_ends_it_an_hour_later() {
    let mut conn = db();
    let m = meeting(&mut conn, "Weekly sync");
    let status = |conn: &Connection| tasks::get(conn, m.id).unwrap();
    // Not yet.
    assert_eq!(
        meetings::advance(&mut conn, clock(DAY, TEN_THIRTY - 1)).unwrap(),
        0
    );
    assert_eq!(status(&conn).status, TaskStatus::Todo);
    // At the start.
    assert_eq!(
        meetings::advance(&mut conn, clock(DAY, TEN_THIRTY)).unwrap(),
        1
    );
    assert_eq!(status(&conn).status, TaskStatus::InProgress);
    // Asking again changes nothing.
    assert_eq!(
        meetings::advance(&mut conn, clock(DAY, TEN_THIRTY + 30)).unwrap(),
        0
    );
    // An hour on, in a zone 5h30 ahead of UTC: done, finished at the moment it ended.
    let ended = Clock {
        date: DAY,
        minute: TEN_THIRTY + 60,
        utc_offset_minutes: 330,
    };
    assert_eq!(meetings::advance(&mut conn, ended).unwrap(), 1);
    let done = status(&conn);
    assert_eq!(done.status, TaskStatus::Done);
    assert_eq!(done.completed_at, Some(datetime!(2027-03-03 06:00 UTC)));
    assert_eq!(meetings::advance(&mut conn, ended).unwrap(), 0);
}

#[test]
fn a_meeting_missed_while_the_app_was_closed_is_done_when_it_ended() {
    let mut conn = db();
    let m = meeting(&mut conn, "Missed");
    // Opened the next week.
    let later = clock(date!(2027 - 03 - 10), 9 * 60);
    assert_eq!(meetings::advance(&mut conn, later).unwrap(), 1);
    let done = tasks::get(&conn, m.id).unwrap();
    assert_eq!(done.status, TaskStatus::Done);
    assert_eq!(done.completed_at, Some(datetime!(2027-03-03 11:30 UTC)));
}

#[test]
fn only_meetings_that_are_to_do_or_in_progress_move() {
    let mut conn = db();
    let later = clock(date!(2027 - 04 - 01), 0);
    let cancelled = meeting(&mut conn, "Cancelled");
    tasks::update(
        &mut conn,
        cancelled.id,
        UpdateTask {
            status: Some(TaskStatus::Cancelled),
            ..Default::default()
        },
    )
    .unwrap();
    let blocked = meeting(&mut conn, "Blocked");
    tasks::update(
        &mut conn,
        blocked.id,
        UpdateTask {
            status: Some(TaskStatus::Blocked),
            ..Default::default()
        },
    )
    .unwrap();
    let mut plain = plain_task(&mut conn, "Plain");
    plain = tasks::update(
        &mut conn,
        plain.id,
        UpdateTask {
            due_date: Patch::Set(DAY),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(meetings::advance(&mut conn, later).unwrap(), 0);
    assert_eq!(
        tasks::get(&conn, cancelled.id).unwrap().status,
        TaskStatus::Cancelled
    );
    assert_eq!(
        tasks::get(&conn, blocked.id).unwrap().status,
        TaskStatus::Blocked
    );
    assert_eq!(
        tasks::get(&conn, plain.id).unwrap().status,
        TaskStatus::Todo
    );
}

#[test]
fn moving_a_started_or_done_meeting_to_a_later_time_opens_it_again() {
    let mut conn = db();
    let m = meeting(&mut conn, "Moved");
    meetings::advance(&mut conn, clock(DAY, TEN_THIRTY + 5)).unwrap();
    assert_eq!(
        tasks::get(&conn, m.id).unwrap().status,
        TaskStatus::InProgress
    );
    let now = clock(DAY, TEN_THIRTY + 10);
    // Same time again (only the length changed): still in progress.
    let same = meetings::set_time(&mut conn, m.id, DAY, TEN_THIRTY, Some(90), now).unwrap();
    assert_eq!(same.status, TaskStatus::InProgress);
    assert_eq!(same.length_minutes, Some(90));
    // Moved to the afternoon: to do again.
    let moved = meetings::set_time(&mut conn, m.id, DAY, 15 * 60, Some(90), now).unwrap();
    assert_eq!(moved.status, TaskStatus::Todo);
    assert_eq!(moved.completed_at, None);
    // A done meeting moved to next week is open again; moved to a time already past it stays done.
    meetings::advance(&mut conn, clock(DAY, 20 * 60)).unwrap();
    assert_eq!(tasks::get(&conn, m.id).unwrap().status, TaskStatus::Done);
    let still =
        meetings::set_time(&mut conn, m.id, DAY, 9 * 60, None, clock(DAY, 20 * 60)).unwrap();
    assert_eq!(still.status, TaskStatus::Done);
    let next_week = meetings::set_time(
        &mut conn,
        m.id,
        date!(2027 - 03 - 10),
        TEN_THIRTY,
        None,
        clock(DAY, 20 * 60),
    )
    .unwrap();
    assert_eq!(next_week.status, TaskStatus::Todo);
}

#[test]
fn a_repeating_meeting_makes_the_next_one_at_the_same_time_and_links_it_to_this_one() {
    let mut conn = db();
    let weekly: Recurrence = Cadence::Weekly {
        every: 1,
        weekday: DAY.weekday().number_days_from_monday(),
    }
    .into();
    let first = meetings::create(
        &mut conn,
        "Weekly sync".into(),
        DAY,
        TEN_THIRTY,
        Some(30),
        None,
        AssigneeChoice::Nobody,
    )
    .unwrap();
    tasks::update(
        &mut conn,
        first.id,
        UpdateTask {
            recurrence: Patch::Set(weekly),
            ..Default::default()
        },
    )
    .unwrap();
    // The clock ends it; the series carries on without anyone touching it.
    meetings::advance(&mut conn, clock(DAY, TEN_THIRTY + 30)).unwrap();
    let all = tasks::list(&conn, false).unwrap();
    let next = all
        .iter()
        .find(|t| t.id != first.id)
        .expect("the next meeting");
    assert!(next.is_meeting());
    assert_eq!(next.title, "Weekly sync");
    assert_eq!(next.status, TaskStatus::Todo);
    assert_eq!(
        (next.start_minute, next.length_minutes),
        (Some(TEN_THIRTY), Some(30))
    );
    assert!(next.due_date.unwrap() > DAY);
    assert!(next.recurrence.is_some());
    // The next one follows up on the last, and so on down the series.
    assert_eq!(follows(&conn), [(next.id, first.id)]);
}

#[test]
fn a_meeting_can_have_several_follow_ups_and_each_follows_one_meeting() {
    let mut conn = db();
    let origin = meeting(&mut conn, "Q3 plan");
    let a = meetings::create_follow_up(
        &mut conn,
        origin.id,
        date!(2027 - 03 - 10),
        TEN_THIRTY,
        None,
    )
    .unwrap();
    let b = meetings::create_follow_up(
        &mut conn,
        origin.id,
        date!(2027 - 03 - 17),
        14 * 60,
        Some(30),
    )
    .unwrap();
    assert_eq!(a.title, "Follow-up: Q3 plan");
    assert_eq!(b.length_minutes, Some(30));
    // The follow-up's own length defaults to the original's.
    assert_eq!(a.length_minutes, Some(60));
    assert_eq!(follows(&conn).len(), 2);
    // A follow-up can have one of its own.
    let c = meetings::create_follow_up(&mut conn, a.id, date!(2027 - 03 - 17), TEN_THIRTY, None)
        .unwrap();
    assert_eq!(c.title, "Follow-up: Q3 plan");
    // An existing meeting that already follows one cannot follow another.
    let other = meeting(&mut conn, "Other");
    let err = meetings::check_follow_up(&conn, a.id, other.id).unwrap_err();
    assert!(
        err.to_string().contains("already follows up on “Q3 plan”"),
        "{err}"
    );
    // Nothing loops.
    let err = meetings::check_follow_up(&conn, origin.id, c.id).unwrap_err();
    assert!(err.to_string().contains("can't follow up"), "{err}");
    // A free meeting can follow up on any.
    assert!(meetings::check_follow_up(&conn, other.id, c.id).is_ok());
}

#[test]
fn a_follow_up_takes_the_project_priority_assignee_and_links_of_its_original() {
    let mut conn = db();
    let person = people::create(
        &mut conn,
        CreatePerson {
            name: "Priya".into(),
            role_title: String::new(),
            email: None,
            weekly_capacity_hours: None,
            is_self: false,
            notes: String::new(),
        },
    )
    .unwrap();
    let project = projects::create(
        &mut conn,
        CreateProject {
            title: "Launch".into(),
            slug: None,
            description: String::new(),
            owner_person_id: None,
            start_date: None,
            target_date: None,
            status: None,
            priority: None,
        },
    )
    .unwrap();
    let origin = meetings::create(
        &mut conn,
        "Design review".into(),
        DAY,
        TEN_THIRTY,
        None,
        Some(project.id),
        AssigneeChoice::Person(person.id),
    )
    .unwrap();
    tasks::update(
        &mut conn,
        origin.id,
        UpdateTask {
            priority: Some(1),
            links: Some(vec![RefLink {
                title: "Agenda".into(),
                url: "https://example.com/agenda".into(),
            }]),
            ..Default::default()
        },
    )
    .unwrap();
    let f = meetings::create_follow_up(
        &mut conn,
        origin.id,
        date!(2027 - 03 - 10),
        TEN_THIRTY,
        None,
    )
    .unwrap();
    assert_eq!((f.project_id, f.priority), (Some(project.id), 1));
    assert_eq!(f.links.len(), 1);
    let assigned: Vec<Uuid> = edges::list_for_node(&conn, f.id, false)
        .unwrap()
        .into_iter()
        .filter(|e| e.edge_type == EdgeType::AssignedTo)
        .map(|e| e.to_id)
        .collect();
    assert_eq!(assigned, [person.id]);
    // Only a meeting can have a follow-up.
    let plain = plain_task(&mut conn, "Plain");
    assert!(meetings::create_follow_up(&mut conn, plain.id, DAY, TEN_THIRTY, None).is_err());
}

#[test]
fn a_link_joins_two_meetings_only() {
    let mut conn = db();
    let m = meeting(&mut conn, "A meeting");
    let t = plain_task(&mut conn, "A task");
    let err = meetings::check_follow_up(&conn, t.id, m.id).unwrap_err();
    assert!(err.to_string().contains("is not a meeting"), "{err}");
    let err = meetings::check_follow_up(&conn, m.id, t.id).unwrap_err();
    assert!(err.to_string().contains("is not a meeting"), "{err}");
}
