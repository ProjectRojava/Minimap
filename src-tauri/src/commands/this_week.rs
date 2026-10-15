use minimap_core::{
    quick_add::parse_when,
    this_week::{build_at, rewind, WeekInput},
};
use minimap_store::Connection;
use minimap_types::{AppError, Date, Patch, Task, ThisWeek, UpdateTask, Uuid};
use tauri::State;

use crate::{
    error::{app_error, store_error},
    state::AppState,
};

/// What needs attention now: overdue, due this week, blocked, my work in progress, waiting-ons
/// that are stale or due, 1:1s. The week is Monday to Sunday and `week_start` can be any date in
/// it (today's week when omitted).
#[tauri::command]
pub async fn get_this_week(
    state: State<'_, AppState>,
    week_start: Option<Date>,
    as_of: Option<Date>,
) -> Result<ThisWeek, AppError> {
    state
        .run(move |conn| this_week_impl(conn, week_start, as_of))
        .await
}

pub(crate) fn this_week_impl(
    conn: &Connection,
    week_of: Option<Date>,
    as_of: Option<Date>,
) -> Result<ThisWeek, AppError> {
    let real_today = minimap_store::today();
    if as_of.is_some_and(|d| d > real_today) {
        return Err(app_error("invalid", "Choose today or a day in the past"));
    }
    let settings = minimap_store::settings::get(conn).map_err(store_error)?;
    let self_id = minimap_store::people::get_self(conn)
        .map_err(store_error)?
        .map(|p| p.id);
    // Fog is how stale things are now, so a look back at a past day has none.
    let fog = if as_of.is_some_and(|d| d < real_today) {
        None
    } else {
        Some(crate::commands::fog::fog_at(conn, real_today)?)
    };
    let mut input = WeekInput {
        fog,

        tasks: minimap_store::views::task_rows(conn).map_err(store_error)?,
        blockers: minimap_store::views::open_blockers(conn).map_err(store_error)?,
        waiting: minimap_store::views::waiting_on_items(conn).map_err(store_error)?,
        notes: minimap_store::views::note_items(conn).map_err(store_error)?,
        objectives: minimap_store::objectives::list(conn, false).map_err(store_error)?,
        self_id,
        today: real_today,
        week_of,
        stale_days: settings.stale_waiting_days,
    };
    // A past day: the data as it stood then, as far as it is known, and that day's own week.
    let as_of = as_of.filter(|d| *d < real_today);
    if let Some(day) = as_of {
        rewind(&mut input, day);
        input.week_of = week_of.or(Some(day));
    }
    Ok(build_at(input, as_of, real_today))
}

/// Moves a task's due date to a natural date (`tomorrow`, `fri`, `next-week`, `+3d`,
/// `2027-03-31`; see `docs/quick-add-grammar.md`).
#[tauri::command]
pub async fn reschedule_task(
    state: State<'_, AppState>,
    id: Uuid,
    when: String,
) -> Result<Task, AppError> {
    state
        .run(move |conn| reschedule_impl(conn, id, &when, minimap_store::today()))
        .await
}

pub(crate) fn reschedule_impl(
    conn: &mut Connection,
    id: Uuid,
    when: &str,
    today: Date,
) -> Result<Task, AppError> {
    let due = parse_when(when, today).map_err(|m| app_error("invalid", m))?;
    minimap_store::tasks::update(
        conn,
        id,
        UpdateTask {
            due_date: Patch::Set(due),
            ..Default::default()
        },
    )
    .map_err(store_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{
        AssigneeChoice, CreateNote, CreatePerson, CreateTask, CreateWaitingOn, EdgeType, NewEdge,
        NodeRef, NodeType, NoteKind, TaskStatus,
    };
    use time::Duration;

    fn person(conn: &mut Connection, name: &str, me: bool) -> Uuid {
        minimap_store::people::create(
            conn,
            CreatePerson {
                name: name.into(),
                role_title: String::new(),
                email: None,
                weekly_capacity_hours: None,
                is_self: me,
                notes: String::new(),
            },
        )
        .unwrap()
        .id
    }

    fn task(
        conn: &mut Connection,
        title: &str,
        due: Option<Date>,
        status: TaskStatus,
        who: AssigneeChoice,
    ) -> Uuid {
        let __p = minimap_store::fixtures::project(conn);
        let t = minimap_store::tasks::create(
            conn,
            CreateTask {
                links: Vec::new(),
                task_type: None,
                focus: None,
                start_minute: None,
                length_minutes: None,
                title: title.into(),
                assignee: who,
                description: String::new(),
                project_id: Some(__p),
                status: Some(status),
                estimate_days: None,
                start_date: None,
                due_date: due,
                priority: None,
                recurrence: None,
            },
        )
        .unwrap();
        t.id
    }

    #[test]
    fn a_listed_task_shows_the_notes_that_mention_it_newest_first() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let today = minimap_store::today();
        person(&mut conn, "Me", true);
        let stuck = task(
            &mut conn,
            "Stuck",
            None,
            TaskStatus::Blocked,
            AssigneeChoice::Nobody,
        );
        let quiet = task(
            &mut conn,
            "Quiet",
            None,
            TaskStatus::Blocked,
            AssigneeChoice::Nobody,
        );
        for (n, text) in ["first", "second", "third", "fourth"].iter().enumerate() {
            minimap_store::notes::create(
                &mut conn,
                CreateNote {
                    title: (*text).into(),
                    body: format!(
                        "{text} finding\n\nAbout {}",
                        minimap_types::mention_token("Stuck", stuck)
                    ),
                    note_date: Some(today - Duration::days(4 - n as i64)),
                    kind: None,
                    recurrence: None,
                },
            )
            .unwrap();
        }
        let w = this_week_impl(&conn, None, None).unwrap();
        assert_eq!(w.attention.len(), 2);
        // Only the task that has notes has an entry: three of four, newest first, with no
        // closing "About" line in the text.
        assert_eq!(w.task_notes.len(), 1);
        let notes = &w.task_notes[0];
        assert_eq!((notes.task, notes.total), (stuck, 4));
        let shown: Vec<&str> = notes.notes.iter().map(|n| n.snippet.as_str()).collect();
        assert_eq!(shown, ["fourth finding", "third finding", "second finding"]);
        assert!(w.task_notes.iter().all(|t| t.task != quiet));
    }

    #[test]
    fn a_past_day_is_allowed_and_a_future_day_is_refused() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let today = minimap_store::today();
        person(&mut conn, "Me", true);
        let past = this_week_impl(&conn, None, Some(today - Duration::days(10))).unwrap();
        assert_eq!(past.as_of, Some(today - Duration::days(10)));
        assert_eq!(past.today, today - Duration::days(10));
        assert_eq!(past.real_today, today);
        // Today itself is just the normal view.
        assert_eq!(
            this_week_impl(&conn, None, Some(today)).unwrap().as_of,
            None
        );
        let err = this_week_impl(&conn, None, Some(today + Duration::days(1))).unwrap_err();
        assert_eq!(err.code, "invalid");
    }

    #[test]
    fn the_week_reads_the_plan() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let today = minimap_store::today();
        person(&mut conn, "Me", true);
        let raj = person(&mut conn, "Raj", false);
        let late = task(
            &mut conn,
            "Late",
            Some(today - Duration::days(3)),
            TaskStatus::Todo,
            AssigneeChoice::Me,
        );
        let stuck = task(
            &mut conn,
            "Stuck",
            None,
            TaskStatus::Blocked,
            AssigneeChoice::Nobody,
        );
        let doing = task(
            &mut conn,
            "Doing",
            None,
            TaskStatus::InProgress,
            AssigneeChoice::Me,
        );
        task(
            &mut conn,
            "Theirs",
            None,
            TaskStatus::InProgress,
            AssigneeChoice::Person(raj),
        );
        minimap_store::edges::add(
            &mut conn,
            NewEdge {
                edge_type: EdgeType::Blocks,
                from: NodeRef::new(NodeType::Task, late),
                to: NodeRef::new(NodeType::Task, stuck),
                attrs: serde_json::json!({}),
            },
        )
        .unwrap();
        minimap_store::waiting_on::create(
            &mut conn,
            CreateWaitingOn {
                description: "Sign-off".into(),
                person_id: raj,
                asked_on: Some(today - Duration::days(20)),
                expected_by: None,
                follow_up_on: None,
            },
        )
        .unwrap();
        let w = this_week_impl(&conn, None, None).unwrap();
        assert!(w.has_self && w.is_current_week);
        assert_eq!(w.overdue.len(), 1);
        assert_eq!(w.overdue[0].row.task.id, late);
        assert_eq!(w.overdue[0].overdue_days, Some(3));
        assert_eq!(w.blocked.len(), 1);
        assert_eq!(w.blocked[0].blocked_by[0].label, "Late");
        assert_eq!(w.in_progress.len(), 1);
        assert_eq!(w.in_progress[0].row.task.id, doing);
        assert_eq!(w.waiting.len(), 1);
        assert!(w.waiting[0].stale);
    }

    #[test]
    fn one_on_ones_dated_this_week_appear() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let today = minimap_store::today();
        for (title, kind, on) in [
            ("1:1 with Priya", NoteKind::OneOnOne, today),
            ("Sync", NoteKind::Meeting, today),
            ("Old 1:1", NoteKind::OneOnOne, today - Duration::days(30)),
        ] {
            minimap_store::notes::create(
                &mut conn,
                CreateNote {
                    title: title.into(),
                    body: String::new(),
                    note_date: Some(on),
                    kind: Some(kind),
                    recurrence: None,
                },
            )
            .unwrap();
        }
        let w = this_week_impl(&conn, None, None).unwrap();
        assert_eq!(w.one_on_ones.len(), 1);
        assert_eq!(w.one_on_ones[0].title, "1:1 with Priya");
        // A week asked for by any of its dates snaps to Monday.
        let other = this_week_impl(&conn, Some(today - Duration::days(30)), None).unwrap();
        assert_eq!(other.one_on_ones.len(), 1);
        assert!(!other.is_current_week);
    }

    #[test]
    fn rescheduling_accepts_natural_dates_and_refuses_nonsense() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let today = time::macros::date!(2027 - 03 - 03); // Wednesday
        let id = task(
            &mut conn,
            "T",
            Some(today - Duration::days(5)),
            TaskStatus::Todo,
            AssigneeChoice::Nobody,
        );
        let due = |conn: &Connection| {
            minimap_store::tasks::get(conn, id)
                .unwrap()
                .due_date
                .unwrap()
        };
        reschedule_impl(&mut conn, id, "tomorrow", today).unwrap();
        assert_eq!(due(&conn), time::macros::date!(2027 - 03 - 04));
        reschedule_impl(&mut conn, id, "next-week", today).unwrap();
        assert_eq!(due(&conn), time::macros::date!(2027 - 03 - 08));
        reschedule_impl(&mut conn, id, "+3d", today).unwrap();
        assert_eq!(due(&conn), time::macros::date!(2027 - 03 - 06));
        let e = reschedule_impl(&mut conn, id, "someday", today).unwrap_err();
        assert_eq!(e.code, "invalid");
        assert_eq!(
            due(&conn),
            time::macros::date!(2027 - 03 - 06),
            "a refused change changes nothing"
        );
        assert_eq!(
            reschedule_impl(&mut conn, Uuid::nil(), "fri", today)
                .unwrap_err()
                .code,
            "not_found"
        );
    }

    #[test]
    fn work_nobody_has_touched_is_listed_as_gone_quiet_but_not_when_looking_back() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let me = person(&mut conn, "Me", true);
        let sam = person(&mut conn, "Sam", false);
        let today = minimap_store::today();
        // Sam's task, under way, due next month: on no other part of the page.
        let theirs = task(
            &mut conn,
            "Sam's migration",
            Some(today + Duration::days(40)),
            TaskStatus::InProgress,
            AssigneeChoice::Person(sam),
        );
        // Mine, under way: in the plan (its fog shows as a chip on its row, not here).
        let mine = task(
            &mut conn,
            "My review",
            None,
            TaskStatus::InProgress,
            AssigneeChoice::Person(me),
        );
        // Both were last touched 20 days ago (their assignee links are part of their history).
        let at = format!("{}T09:00:00.000Z", today - Duration::days(20));
        for id in [theirs, mine] {
            conn.execute(
                "UPDATE tasks SET updated_at = ?2 WHERE id = ?1",
                [id.to_string(), at.clone()],
            )
            .unwrap();
        }
        conn.execute("UPDATE activity SET at = ?1", [at]).unwrap();
        let w = this_week_impl(&conn, None, None).unwrap();
        let quiet: Vec<&str> = w
            .quiet
            .iter()
            .map(|q| q.task.row.task.title.as_str())
            .collect();
        assert_eq!(quiet, ["Sam's migration"]);
        assert_eq!(w.quiet[0].mark.age_days, 20);
        assert!(w.priorities.iter().any(|t| t.id() == mine));

        let back = this_week_impl(&conn, None, Some(today - Duration::days(1))).unwrap();
        assert!(back.quiet.is_empty());
    }
}
