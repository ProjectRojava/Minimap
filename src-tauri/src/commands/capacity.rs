use minimap_core::capacity::{compute, CapacityInput};
use minimap_store::Connection;
use minimap_types::{AppError, Capacity, Date};
use tauri::State;

use crate::{error::store_error, state::AppState};

/// How loaded each person is, week by week (Monday to Sunday weeks): scheduled working days of
/// their open tasks, weighted by allocation, against weekly capacity; over 100% is overloaded.
/// `from` and `to` can be any dates in the first and last week; without `to`, `weeks` weeks
/// (default 8) are shown starting at `from` (this week when omitted).
#[tauri::command]
pub async fn get_capacity(
    state: State<'_, AppState>,
    from: Option<Date>,
    to: Option<Date>,
    weeks: Option<u32>,
) -> Result<Capacity, AppError> {
    state
        .run(move |conn| capacity_impl(conn, from, to, weeks))
        .await
}

pub(crate) fn capacity_impl(
    conn: &Connection,
    from: Option<Date>,
    to: Option<Date>,
    weeks: Option<u32>,
) -> Result<Capacity, AppError> {
    let settings = minimap_store::settings::get(conn).map_err(store_error)?;
    let tasks = minimap_store::tasks::list(conn, false).map_err(store_error)?;
    let edges = minimap_store::edges::list_active(conn).map_err(store_error)?;
    let projects = minimap_store::projects::list(conn, false).map_err(store_error)?;
    let people = minimap_store::people::list(conn, false).map_err(store_error)?;
    Ok(compute(&CapacityInput {
        tasks: &tasks,
        edges: &edges,
        projects: &projects,
        people: &people,
        today: minimap_store::today(),
        hours_per_day: settings.hours_per_day,
        work_week: settings.work_week,
        from,
        to,
        weeks,
        task_limit: settings.capacity_task_limit,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{AssigneeChoice, CreatePerson, CreateTask, UpdateSettings, Uuid};

    fn person(conn: &mut Connection, name: &str, hours: Option<f64>) -> Uuid {
        minimap_store::people::create(
            conn,
            CreatePerson {
                name: name.into(),
                role_title: String::new(),
                email: None,
                weekly_capacity_hours: hours,
                is_self: false,
                notes: String::new(),
            },
        )
        .unwrap()
        .id
    }

    fn task(conn: &mut Connection, title: &str, days: Option<f64>, who: Uuid) {
        minimap_store::tasks::create(
            conn,
            CreateTask {
                links: Vec::new(),
                title: title.into(),
                assignee: AssigneeChoice::Person(who),
                description: String::new(),
                project_id: None,
                status: None,
                estimate_days: days,
                start_date: None,
                due_date: None,
                priority: None,
                recurrence: None,
            },
        )
        .unwrap();
    }

    #[test]
    fn capacity_reads_people_tasks_and_settings() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let priya = person(&mut conn, "Priya", None);
        let part = person(&mut conn, "Part-timer", Some(20.0));
        task(&mut conn, "Big", Some(10.0), priya);
        task(&mut conn, "Also big", Some(10.0), part);
        let c = capacity_impl(&conn, None, None, Some(3)).unwrap();
        assert_eq!(c.weeks.len(), 3);
        assert_eq!(c.task_limit, 10);
        assert_eq!(c.hours_per_day, 8.0);
        // The part-timer (2.5 days a week) is far more loaded than a full-timer on equal work.
        let by = |name: &str| c.people.iter().find(|p| p.person.label == name).unwrap();
        assert!(by("Part-timer").peak_pct > by("Priya").peak_pct);
        assert!(by("Part-timer").peak_pct > 100.0);
        assert_eq!(
            c.people[0].person.label, "Part-timer",
            "overloaded people first"
        );
    }

    #[test]
    fn the_task_limit_setting_changes_the_flag() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let p = person(&mut conn, "Busy", None);
        for i in 0..4 {
            task(&mut conn, &format!("t{i}"), Some(0.1), p);
        }
        assert!(!capacity_impl(&conn, None, None, Some(1)).unwrap().people[0].over_task_limit);
        minimap_store::settings::update(
            &mut conn,
            UpdateSettings {
                capacity_task_limit: Some(3),
                ..Default::default()
            },
        )
        .unwrap();
        let c = capacity_impl(&conn, None, None, Some(1)).unwrap();
        assert!(c.people[0].over_task_limit);
        assert_eq!(c.task_limit, 3);
    }
}
