//! Your people (docs/product-design/06-your-people.md, spec 49): compact frames for you and your
//! direct reports. Thin: the rules are `minimap_core::people_frames`; the load is the number
//! `get_capacity` gives for the current week.

use minimap_core::people_frames::{build, FramesInput};
use minimap_store::Connection;
use minimap_types::{AppError, Date, PeopleFrames};
use tauri::State;

use crate::{commands::capacity::capacity_at, error::store_error, state::AppState};

/// You and each of your direct reports, by name: this week's load, what you wait on them for,
/// their blocked and overdue tasks, and how long since your last 1:1. `None` before first run.
/// Never a ranking: nothing here is ordered by how busy someone is.
#[tauri::command]
pub async fn get_people_frames(
    state: State<'_, AppState>,
) -> Result<Option<PeopleFrames>, AppError> {
    state
        .run(|conn| frames_at(conn, minimap_store::today()))
        .await
}

/// [`get_people_frames`] as of `today` (the tests and the demo snapshots fix it).
pub(crate) fn frames_at(conn: &Connection, today: Date) -> Result<Option<PeopleFrames>, AppError> {
    let settings = minimap_store::settings::get(conn).map_err(store_error)?;
    // Today's week, through the same code the Capacity screen runs.
    let capacity = capacity_at(conn, today, None, None, Some(1))?;
    let people = minimap_store::people::list(conn, false).map_err(store_error)?;
    let edges = minimap_store::edges::list_active(conn).map_err(store_error)?;
    let tasks = minimap_store::tasks::list_planned(conn).map_err(store_error)?;
    let waiting = minimap_store::waiting_on::list(conn, false).map_err(store_error)?;
    let notes = minimap_store::views::note_items(conn).map_err(store_error)?;
    Ok(build(&FramesInput {
        today,
        people: &people,
        edges: &edges,
        tasks: &tasks,
        waiting: &waiting,
        notes: &notes,
        capacity: &capacity,
        stale_waiting_days: settings.stale_waiting_days,
        one_on_one_every_days: settings.one_on_one_every_days,
        all_levels: settings.frames_all_levels,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::capacity::capacity_at;
    use minimap_types::{
        AssigneeChoice, CreateNote, CreatePerson, CreateTask, EdgeType, NewEdge, NodeRef, NodeType,
        NoteKind, UpdateSettings, Uuid,
    };
    use time::macros::date;

    // Wednesday 2027-03-03, as in the demo snapshots.
    const TODAY: Date = date!(2027 - 03 - 03);

    fn person(conn: &mut Connection, name: &str) -> Uuid {
        minimap_store::people::create(
            conn,
            CreatePerson {
                name: name.into(),
                role_title: "Engineer".into(),
                email: None,
                weekly_capacity_hours: None,
                is_self: false,
                notes: String::new(),
            },
        )
        .unwrap()
        .id
    }

    fn reports_to(conn: &mut Connection, who: Uuid, boss: Uuid) {
        minimap_store::edges::add(
            conn,
            NewEdge {
                edge_type: EdgeType::ReportsTo,
                from: NodeRef::new(NodeType::Person, who),
                to: NodeRef::new(NodeType::Person, boss),
                attrs: serde_json::json!({}),
            },
        )
        .unwrap();
    }

    fn task(conn: &mut Connection, who: Uuid, days: f64) {
        minimap_store::tasks::create(
            conn,
            CreateTask {
                links: Vec::new(),
                task_type: None,
                focus: None,
                start_minute: None,
                length_minutes: None,
                title: format!("Work {days}"),
                assignee: AssigneeChoice::Person(who),
                description: String::new(),
                project_id: None,
                status: None,
                estimate_days: Some(days),
                start_date: None,
                due_date: None,
                priority: None,
                recurrence: None,
            },
        )
        .unwrap();
    }

    #[test]
    fn there_are_no_frames_before_first_run() {
        let conn = minimap_store::open_in_memory().unwrap();
        assert!(frames_at(&conn, TODAY).unwrap().is_none());
    }

    #[test]
    fn you_and_your_reports_come_with_the_load_the_capacity_screen_shows() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let me = minimap_store::people::ensure_self(&mut conn, "Me")
            .unwrap()
            .id;
        let raj = person(&mut conn, "Raj");
        let anna = person(&mut conn, "Anna");
        let stranger = person(&mut conn, "Stranger");
        reports_to(&mut conn, raj, me);
        reports_to(&mut conn, anna, me);
        task(&mut conn, raj, 3.0);
        task(&mut conn, raj, 3.0);
        let frames = frames_at(&conn, TODAY).unwrap().unwrap();
        assert!(frames.me.is_self);
        let names: Vec<&str> = frames
            .reports
            .iter()
            .map(|f| f.person.label.as_str())
            .collect();
        assert_eq!(names, ["Anna", "Raj"], "by name, not by load");
        assert!(!names.contains(&"Stranger"));
        assert_ne!(stranger, raj);
        // The same number the heatmap shows.
        let capacity = capacity_at(&conn, TODAY, None, None, None).unwrap();
        let cell = capacity
            .people
            .iter()
            .find(|c| c.person.node.id == raj)
            .and_then(|c| {
                c.weeks
                    .iter()
                    .find(|w| w.week_start == capacity.current_week_start)
            })
            .unwrap();
        let frame = frames
            .reports
            .iter()
            .find(|f| f.person.node.id == raj)
            .unwrap();
        assert_eq!(frame.load_pct, Some(cell.load_pct.round() as u32));
        assert_eq!(frame.over, cell.over);
        assert_eq!(frame.open_tasks, 2);
    }

    #[test]
    fn the_settings_set_the_rhythm_and_whether_everyone_below_is_shown() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let me = minimap_store::people::ensure_self(&mut conn, "Me")
            .unwrap()
            .id;
        let raj = person(&mut conn, "Raj");
        let maya = person(&mut conn, "Maya");
        reports_to(&mut conn, raj, me);
        reports_to(&mut conn, maya, raj);
        // Both were added "now"; a 1:1 a week ago.
        let made = minimap_store::notes::create(
            &mut conn,
            CreateNote {
                title: "1:1 with Raj".into(),
                body: minimap_types::mention_token("Raj", raj),
                note_date: Some(date!(2027 - 02 - 24)),
                kind: Some(NoteKind::OneOnOne),
                recurrence: None,
            },
        )
        .unwrap();
        assert_eq!(made.kind, NoteKind::OneOnOne);
        let by_default = frames_at(&conn, TODAY).unwrap().unwrap();
        assert_eq!(
            by_default.reports[0].last_one_on_one,
            Some(date!(2027 - 02 - 24))
        );
        assert_eq!(by_default.reports[0].one_on_one_due, None);
        // Everyone below your reports is shown by default.
        assert_eq!(by_default.below.len(), 1);
        assert_eq!(by_default.below[0].frames[0].person.label, "Maya");
        minimap_store::settings::update(
            &mut conn,
            UpdateSettings {
                one_on_one_every_days: Some(5),
                frames_all_levels: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
        let tighter = frames_at(&conn, TODAY).unwrap().unwrap();
        assert_eq!(tighter.reports[0].one_on_one_due, Some(7));
        assert!(tighter.below.is_empty(), "off: only your direct reports");
    }
}
