//! The order of work (spec 60, ADR-0024): a person's tasks as steps, top to bottom. Thin: the
//! rules are `minimap_core::order`, and the dates come from the same portfolio schedule the
//! Gantt and the forecast read (the order is one of its inputs).

use minimap_store::Connection;
use minimap_types::{
    AppError, Date, NodeRef, NodeSummary, NodeType, ScheduleScope, Uuid, WorkOrder,
};
use tauri::State;

use crate::{
    error::{app_error, store_error},
    state::AppState,
};

/// `person_id`'s order (you when `None`): the steps with the dates the schedule gives them, the
/// person's other open tasks, and what a `blocks` link overrides.
#[tauri::command]
pub async fn get_order(
    state: State<'_, AppState>,
    person_id: Option<Uuid>,
) -> Result<WorkOrder, AppError> {
    state
        .run(move |conn| order_at(conn, minimap_store::today(), person_id))
        .await
}

/// Makes `steps` (top to bottom; tasks in one step side by side) the person's order, and returns
/// it as [`get_order`] does. One undo step.
#[tauri::command]
pub async fn set_order(
    state: State<'_, AppState>,
    person_id: Uuid,
    steps: Vec<Vec<Uuid>>,
) -> Result<WorkOrder, AppError> {
    state
        .run(move |conn| set_order_impl(conn, person_id, steps, minimap_store::today()))
        .await
}

pub(crate) fn set_order_impl(
    conn: &mut Connection,
    person_id: Uuid,
    steps: Vec<Vec<Uuid>>,
    today: Date,
) -> Result<WorkOrder, AppError> {
    minimap_store::order::set(conn, person_id, steps).map_err(store_error)?;
    minimap_store::order::sync_dates(conn, today).map_err(store_error)?;
    order_at(conn, today, Some(person_id))
}

/// [`get_order`] as of `today` (the tests and the demo snapshot fix it).
pub(crate) fn order_at(
    conn: &Connection,
    today: Date,
    person_id: Option<Uuid>,
) -> Result<WorkOrder, AppError> {
    let person = match person_id {
        Some(id) => minimap_store::people::get(conn, id).map_err(store_error)?,
        None => minimap_store::people::get_self(conn)
            .map_err(store_error)?
            .ok_or_else(|| app_error("not_found", "There is no person for you yet"))?,
    };
    let tasks = minimap_store::tasks::list_planned(conn).map_err(store_error)?;
    let edges = minimap_store::edges::list_active(conn).map_err(store_error)?;
    let projects = minimap_store::projects::list(conn, false).map_err(store_error)?;
    let work_week = minimap_store::settings::get(conn)
        .map_err(store_error)?
        .work_week;
    let schedule = minimap_core::schedule::compute(
        &tasks,
        &edges,
        &projects,
        today,
        work_week,
        ScheduleScope::Portfolio,
    )
    .map_err(|e| app_error("cycle", e.to_string()))?;
    let summary = NodeSummary {
        node: NodeRef::new(NodeType::Person, person.id),
        label: person.name,
        archived: person.archived_at.is_some(),
    };
    Ok(minimap_core::order::view(
        summary, &tasks, &edges, &projects, &schedule,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{AssigneeChoice, CreatePerson, CreateTask, Patch, TaskStatus, UpdateTask};
    use time::macros::date;

    // A Wednesday.
    const TODAY: Date = date!(2027 - 03 - 03);

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

    fn task(conn: &mut Connection, title: &str, who: Uuid, est: f64) -> Uuid {
        let project = minimap_store::fixtures::project(conn);
        minimap_store::tasks::create(
            conn,
            CreateTask {
                links: Vec::new(),
                task_type: Some("build".into()),
                focus: None,
                backburner_since: None,
                start_minute: None,
                length_minutes: None,
                title: title.into(),
                assignee: AssigneeChoice::Person(who),
                description: String::new(),
                project_id: Some(project),
                status: None,
                estimate_days: Some(est),
                start_date: None,
                due_date: Some(date!(2027 - 03 - 05)),
                priority: None,
                recurrence: None,
            },
        )
        .unwrap()
        .id
    }

    #[test]
    fn steps_run_one_after_the_other_and_side_by_side_together() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let me = person(&mut conn, "Me", true);
        let a = task(&mut conn, "A", me, 2.0);
        let b = task(&mut conn, "B", me, 3.0);
        let c = task(&mut conn, "C", me, 1.0);
        let d = task(&mut conn, "D", me, 1.0);
        let before = order_at(&conn, TODAY, None).unwrap();
        assert!(before.steps.is_empty());
        assert_eq!(before.unordered.len(), 4);
        // Without an order, all four start today.
        assert!(before.unordered.iter().all(|c| c.start == Some(TODAY)));

        let o = set_order_impl(&mut conn, me, vec![vec![a, b], vec![c]], TODAY).unwrap();
        assert_eq!(o.steps.len(), 2);
        let s1 = &o.steps[0];
        assert_eq!(s1.cards.len(), 2);
        assert_eq!((s1.start, s1.load_pct), (Some(TODAY), 200));
        // B (3 days from Wed) ends on Fri; C starts on the Monday after.
        assert_eq!(s1.finish, Some(date!(2027 - 03 - 05)));
        assert_eq!(o.steps[1].start, Some(date!(2027 - 03 - 08)));
        assert!(
            o.steps[1].cards[0].past_due,
            "C now finishes after its due date"
        );
        assert_eq!(o.unordered.iter().map(|c| c.id).collect::<Vec<_>>(), [d]);
        assert!(o.conflicts.is_empty());
        // The Gantt and the forecast read the same schedule.
        let s =
            crate::commands::schedule::schedule_at(&conn, TODAY, ScheduleScope::Portfolio).unwrap();
        let start = |id| s.tasks.iter().find(|t| t.id == id).unwrap().start;
        assert_eq!(start(c), date!(2027 - 03 - 08));
        assert_eq!(start(d), TODAY);
    }

    #[test]
    fn the_task_below_gets_a_start_date_and_a_slip_moves_it_down() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let me = person(&mut conn, "Me", true);
        let a = task(&mut conn, "A", me, 2.0);
        let b = task(&mut conn, "B", me, 2.0);
        let c = task(&mut conn, "C", me, 1.0);
        let free = task(&mut conn, "Free", me, 1.0);
        let start = |conn: &Connection, id| minimap_store::tasks::get(conn, id).unwrap().start_date;
        set_order_impl(&mut conn, me, vec![vec![a], vec![b], vec![c]], TODAY).unwrap();
        // A is Wed-Thu, B Fri-Mon, C Tue. Step 1 and a task outside the order keep no date.
        assert_eq!(start(&conn, a), None);
        assert_eq!(start(&conn, free), None);
        assert_eq!(start(&conn, b), Some(date!(2027 - 03 - 05)));
        assert_eq!(start(&conn, c), Some(date!(2027 - 03 - 09)));
        // Nothing changed: nothing is written again.
        assert_eq!(
            minimap_store::order::sync_dates(&mut conn, TODAY).unwrap(),
            0
        );
        // A needs a day longer: B and C move down, one working day each.
        minimap_store::tasks::update(
            &mut conn,
            a,
            UpdateTask {
                estimate_days: Patch::Set(3.0),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            minimap_store::order::sync_dates(&mut conn, TODAY).unwrap(),
            2
        );
        assert_eq!(start(&conn, b), Some(date!(2027 - 03 - 08)));
        assert_eq!(start(&conn, c), Some(date!(2027 - 03 - 10)));
        // The dates are real: the Order screen shows them and nothing is held back any more.
        let o = order_at(&conn, TODAY, Some(me)).unwrap();
        assert_eq!(o.steps[1].start, Some(date!(2027 - 03 - 08)));
    }

    #[test]
    fn a_reorder_renumbers_and_finishing_or_reassigning_takes_a_task_out() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let me = person(&mut conn, "Me", true);
        let other = person(&mut conn, "Raj", false);
        let a = task(&mut conn, "A", me, 1.0);
        let b = task(&mut conn, "B", me, 1.0);
        let c = task(&mut conn, "C", me, 1.0);
        set_order_impl(&mut conn, me, vec![vec![a], vec![b], vec![c]], TODAY).unwrap();
        let step = |conn: &Connection, id| minimap_store::tasks::get(conn, id).unwrap().order_step;
        assert_eq!(
            (step(&conn, a), step(&conn, b), step(&conn, c)),
            (Some(1), Some(2), Some(3))
        );
        // Moving C to the top renumbers all three.
        set_order_impl(&mut conn, me, vec![vec![c], vec![a], vec![b]], TODAY).unwrap();
        assert_eq!(
            (step(&conn, a), step(&conn, b), step(&conn, c)),
            (Some(2), Some(3), Some(1))
        );
        // Finishing takes a task out of the order.
        minimap_store::tasks::update(
            &mut conn,
            a,
            UpdateTask {
                status: Some(TaskStatus::Done),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(step(&conn, a), None);
        // So does giving it to someone else.
        minimap_store::tasks::set_assignee(&mut conn, b, Some(other)).unwrap();
        assert_eq!(step(&conn, b), None);
        // A task that isn't theirs can't be put in their order.
        let err = set_order_impl(&mut conn, me, vec![vec![b]], TODAY).unwrap_err();
        assert_eq!(err.code, "invalid");
        assert!(err.message.contains("isn't assigned"), "{}", err.message);
        // Clearing one by hand is an ordinary patch field.
        minimap_store::tasks::update(
            &mut conn,
            c,
            UpdateTask {
                order_step: Patch::Clear,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(order_at(&conn, TODAY, Some(me)).unwrap().steps.is_empty());
    }
}
