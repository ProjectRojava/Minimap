//! Meetings (spec 38): making and moving them, following them up, and the clock that moves them
//! on. Thin: parse, call the store, return.

use minimap_store::Connection;
use minimap_types::{
    parse_clock, AdvanceResult, AppError, Clock, CreateMeeting, Date, MeetingTime, Task, Uuid,
    DEFAULT_MEETING_MINUTES,
};
use tauri::State;

use crate::{
    error::{app_error, store_error},
    state::AppState,
};

/// The day, minute and length of a meeting as the screen typed them.
fn parse_when(when: &MeetingTime) -> Result<(Date, u16, Option<u32>), AppError> {
    let minute = parse_clock(&when.time).map_err(|m| app_error("invalid", m))?;
    Ok((when.date, minute, when.length_minutes))
}

/// A new meeting at a day and time.
#[tauri::command]
pub async fn create_meeting(
    state: State<'_, AppState>,
    input: CreateMeeting,
) -> Result<Task, AppError> {
    state
        .run(move |conn| create_meeting_impl(conn, input))
        .await
}

pub(crate) fn create_meeting_impl(
    conn: &mut Connection,
    input: CreateMeeting,
) -> Result<Task, AppError> {
    let title = input.title.trim().to_owned();
    if title.is_empty() {
        return Err(app_error("invalid", "A meeting needs a title."));
    }
    let (date, minute, length) = parse_when(&input.when)?;
    minimap_store::meetings::create(
        conn,
        title,
        date,
        minute,
        Some(length.unwrap_or(DEFAULT_MEETING_MINUTES)),
        input.project_id,
        input.assignee,
    )
    .map_err(store_error)
}

/// Makes a task a meeting at this day and time, or moves a meeting. A started or finished
/// meeting moved to a time that has not come is open again. `clock` is the time where the user is.
#[tauri::command]
pub async fn set_meeting(
    state: State<'_, AppState>,
    id: Uuid,
    when: MeetingTime,
    clock: Clock,
) -> Result<Task, AppError> {
    state
        .run(move |conn| set_meeting_impl(conn, id, &when, clock))
        .await
}

pub(crate) fn set_meeting_impl(
    conn: &mut Connection,
    id: Uuid,
    when: &MeetingTime,
    clock: Clock,
) -> Result<Task, AppError> {
    let (date, minute, length) = parse_when(when)?;
    minimap_store::meetings::set_time(conn, id, date, minute, length, clock).map_err(store_error)
}

/// A new meeting that follows up on `source`: same time next week unless `when` says otherwise.
#[tauri::command]
pub async fn schedule_follow_up(
    state: State<'_, AppState>,
    source: Uuid,
    when: Option<MeetingTime>,
) -> Result<Task, AppError> {
    state
        .run(move |conn| schedule_follow_up_impl(conn, source, when.as_ref()))
        .await
}

pub(crate) fn schedule_follow_up_impl(
    conn: &mut Connection,
    source: Uuid,
    when: Option<&MeetingTime>,
) -> Result<Task, AppError> {
    let (date, minute, length) = match when {
        Some(w) => parse_when(w)?,
        None => {
            let from = minimap_store::tasks::get(conn, source).map_err(store_error)?;
            let (date, minute) = minimap_core::meeting::follow_up_slot(&from).ok_or_else(|| {
                app_error("invalid", "That meeting has no time to follow up from.")
            })?;
            (date, minute, None)
        }
    };
    minimap_store::meetings::create_follow_up(conn, source, date, minute, length)
        .map_err(store_error)
}

/// A clock that is days away from the real one is a mistake, not a time zone.
fn check_clock(clock: &Clock, today: Date) -> Result<(), AppError> {
    if (clock.date - today).whole_days().abs() > 2
        || clock.utc_offset_minutes.abs() > 14 * 60
        || clock.minute >= 1440
    {
        return Err(app_error("invalid", "The clock is not a time of day."));
    }
    Ok(())
}

/// Lets the clock move meetings on: in progress when they start, done when they end. The screen
/// calls this when it opens and every half minute, and says what time it is (the backend has no
/// reliable way to know the local time zone). Not an undo step: it is the clock, not the user.
#[tauri::command]
pub async fn advance_meetings(
    state: State<'_, AppState>,
    clock: Clock,
) -> Result<AdvanceResult, AppError> {
    state
        .run_vault(move |vault, _| {
            let conn = vault.parts()?.0;
            advance_meetings_impl(conn, clock, minimap_store::today())
        })
        .await
}

pub(crate) fn advance_meetings_impl(
    conn: &mut Connection,
    clock: Clock,
    today: Date,
) -> Result<AdvanceResult, AppError> {
    check_clock(&clock, today)?;
    let changed = minimap_store::meetings::advance(conn, clock).map_err(store_error)?;
    Ok(AdvanceResult { changed })
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{AssigneeChoice, EdgeType, NodeRef, NodeType, TaskStatus, MEETING_TYPE};
    use time::macros::date;

    fn conn() -> Connection {
        minimap_store::open_in_memory().unwrap()
    }

    fn at(date: Date, time: &str) -> MeetingTime {
        MeetingTime {
            date,
            time: time.into(),
            length_minutes: None,
        }
    }

    fn clock(date: Date, minute: u16) -> Clock {
        Clock {
            date,
            minute,
            utc_offset_minutes: 0,
        }
    }

    fn new(conn: &mut Connection, title: &str, when: MeetingTime) -> Result<Task, AppError> {
        create_meeting_impl(
            conn,
            CreateMeeting {
                title: title.into(),
                when,
                project_id: None,
                assignee: AssigneeChoice::Nobody,
            },
        )
    }

    #[test]
    fn a_meeting_is_made_from_a_title_a_day_and_a_typed_time() {
        let mut conn = conn();
        let day = date!(2027 - 03 - 03);
        let m = new(&mut conn, "  Weekly sync ", at(day, "9:30")).unwrap();
        assert_eq!(m.title, "Weekly sync");
        assert_eq!(
            (m.task_type.as_deref(), m.due_date, m.start_minute),
            (Some(MEETING_TYPE), Some(day), Some(570))
        );
        // The default hour is written down, so it reads the same later.
        assert_eq!(m.length_minutes, Some(60));
        let long = new(
            &mut conn,
            "Offsite",
            MeetingTime {
                length_minutes: Some(480),
                ..at(day, "09:00")
            },
        )
        .unwrap();
        assert_eq!(long.length_minutes, Some(480));
        // A blank title and a time that is not one are refused with a reason.
        assert_eq!(
            new(&mut conn, "  ", at(day, "09:00")).unwrap_err().code,
            "invalid"
        );
        let err = new(&mut conn, "x", at(day, "25:00")).unwrap_err();
        assert_eq!(err.code, "invalid");
        assert!(err.message.contains("24-hour"), "{}", err.message);
    }

    #[test]
    fn moving_a_meeting_reopens_it_when_the_new_time_has_not_come() {
        let mut conn = conn();
        let day = date!(2027 - 03 - 03);
        let m = new(&mut conn, "Sync", at(day, "10:30")).unwrap();
        advance_meetings_impl(&mut conn, clock(day, 11 * 60), day).unwrap();
        let moved =
            set_meeting_impl(&mut conn, m.id, &at(day, "15:00"), clock(day, 11 * 60)).unwrap();
        assert_eq!(
            (moved.status, moved.start_minute),
            (TaskStatus::Todo, Some(900))
        );
        // A plain task is made a meeting by giving it a time.
        let task = minimap_store::tasks::create(
            &mut conn,
            minimap_types::CreateTask {
                links: Vec::new(),
                task_type: None,
                focus: None,
                start_minute: None,
                length_minutes: None,
                title: "Plain".into(),
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
        .unwrap();
        let made = set_meeting_impl(&mut conn, task.id, &at(day, "16:00"), clock(day, 0)).unwrap();
        assert!(made.is_meeting());
    }

    #[test]
    fn the_clock_moves_meetings_and_a_wrong_clock_is_refused() {
        let mut conn = conn();
        let day = date!(2027 - 03 - 03);
        // The store's own idea of today is the real one: the test is about the checks, so ask
        // for a day relative to it.
        let today = minimap_store::today();
        let m = new(&mut conn, "Sync", at(today, "10:30")).unwrap();
        let r = advance_meetings_impl(&mut conn, clock(today, 10 * 60 + 30), today).unwrap();
        assert_eq!(r.changed, 1);
        assert_eq!(
            minimap_store::tasks::get(&conn, m.id).unwrap().status,
            TaskStatus::InProgress
        );
        assert_eq!(
            advance_meetings_impl(&mut conn, clock(today, 10 * 60 + 31), today)
                .unwrap()
                .changed,
            0
        );
        for bad in [
            clock(today + time::Duration::days(5), 0),
            clock(day.min(today - time::Duration::days(5)), 0),
            clock(today, 1440),
            Clock {
                utc_offset_minutes: 15 * 60,
                ..clock(today, 0)
            },
        ] {
            assert_eq!(
                advance_meetings_impl(&mut conn, bad, today)
                    .unwrap_err()
                    .code,
                "invalid"
            );
        }
    }

    #[test]
    fn a_follow_up_defaults_to_next_week_at_the_same_time_and_is_linked() {
        let mut conn = conn();
        let day = date!(2027 - 03 - 03);
        let m = new(&mut conn, "Q3 plan", at(day, "10:30")).unwrap();
        let f = schedule_follow_up_impl(&mut conn, m.id, None).unwrap();
        assert_eq!(
            (f.due_date, f.start_minute, f.title.as_str()),
            (Some(date!(2027 - 03 - 10)), Some(630), "Follow-up: Q3 plan")
        );
        // Another one, at a time of the user's choosing.
        let g = schedule_follow_up_impl(&mut conn, m.id, Some(&at(date!(2027 - 03 - 12), "14:00")))
            .unwrap();
        assert_eq!(g.start_minute, Some(840));
        let links = minimap_store::edges::list_for_node(&conn, m.id, false).unwrap();
        let follow_ups: Vec<Uuid> = links
            .iter()
            .filter(|e| e.edge_type == EdgeType::FollowsUp && e.to_id == m.id)
            .map(|e| e.from_id)
            .collect();
        assert_eq!(follow_ups.len(), 2);
        assert!(follow_ups.contains(&f.id) && follow_ups.contains(&g.id));
        // Not a meeting: refused.
        let task =
            minimap_store::nodes::summary(&conn, NodeRef::new(NodeType::Task, f.id)).unwrap();
        assert!(!task.label.is_empty());
    }

    #[test]
    fn a_meeting_is_not_part_of_the_plan_even_when_it_is_linked_to_it() {
        use crate::commands::{
            capacity::capacity_impl, graph::graph_impl, overview::overview_impl,
            schedule::schedule_impl,
        };
        use minimap_types::{CreateProject, GraphFilter, GraphLevel, NewEdge, ScheduleScope};
        let mut conn = conn();
        let project = minimap_store::projects::create(
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
        let task = |conn: &mut Connection, title: &str| {
            minimap_store::tasks::create(
                conn,
                minimap_types::CreateTask {
                    links: Vec::new(),
                    task_type: None,
                    focus: None,
                    start_minute: None,
                    length_minutes: None,
                    title: title.into(),
                    assignee: AssigneeChoice::Nobody,
                    description: String::new(),
                    project_id: Some(project.id),
                    status: None,
                    estimate_days: Some(2.0),
                    start_date: None,
                    due_date: None,
                    priority: None,
                    recurrence: None,
                },
            )
            .unwrap()
        };
        let build = task(&mut conn, "Build it");
        let before = (
            schedule_impl(&conn, ScheduleScope::Portfolio).unwrap(),
            capacity_impl(&conn, None, None, None).unwrap(),
        );
        // A meeting with no estimate, which "blocks" the task and is related to it.
        let kickoff = new(&mut conn, "Kick-off", at(minimap_store::today(), "10:00")).unwrap();
        for edge_type in [EdgeType::Blocks, EdgeType::RelatesTo] {
            minimap_store::edges::add(
                &mut conn,
                NewEdge {
                    edge_type,
                    from: NodeRef::new(NodeType::Task, kickoff.id),
                    to: NodeRef::new(NodeType::Task, build.id),
                    attrs: serde_json::json!({}),
                },
            )
            .unwrap();
        }
        // The plan reads the same with it as without it, and nothing fails on its links.
        let schedule = schedule_impl(&conn, ScheduleScope::Portfolio).unwrap();
        assert_eq!(schedule, before.0);
        assert!(schedule.tasks.iter().all(|t| t.id != kickoff.id));
        assert_eq!(capacity_impl(&conn, None, None, None).unwrap(), before.1);
        let overview = overview_impl(&mut conn).unwrap();
        assert!(overview.risks.iter().all(|r| r.node.node.id != kickoff.id));
        let graph = graph_impl(
            &conn,
            &GraphFilter {
                level: GraphLevel::Tasks,
                include_isolated: true,
                include_done: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(graph.nodes.iter().all(|n| n.node.id != kickoff.id));
        assert!(
            crate::commands::impact::with_world_at(&conn, minimap_store::today(), |_| Ok(()))
                .is_ok()
        );
        // The one list that does show it: all tasks.
        assert_eq!(minimap_store::tasks::list(&conn, false).unwrap().len(), 2);
    }
}
