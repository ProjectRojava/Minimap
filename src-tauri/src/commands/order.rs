//! The order of work (spec 60, ADR-0024): a person's tasks as steps, top to bottom. Thin: the
//! rules are `minimap_core::order`, and the dates come from the same portfolio schedule the
//! Gantt and the forecast read (the order is one of its inputs).

use minimap_store::Connection;
use minimap_types::{
    AppError, Date, NodeRef, NodeSummary, NodeType, ScheduleScope, TeamMember, TeamOrders, Uuid,
    WorkOrder,
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

/// Everyone in Minimap (you first, then by name), each with their order and this week's load
/// (spec 61, *My team*).
#[tauri::command]
pub async fn get_team_orders(state: State<'_, AppState>) -> Result<TeamOrders, AppError> {
    state
        .run(|conn| team_at(conn, minimap_store::today()))
        .await
}

/// Gives `task_id` to `person_id` (their only assignee) and makes `steps` their order, which may
/// hold the task or not (dragged to their column on *My team*, spec 61). One undo step; returns
/// the team as [`get_team_orders`] does.
#[tauri::command]
pub async fn give_task(
    state: State<'_, AppState>,
    task_id: Uuid,
    person_id: Uuid,
    steps: Vec<Vec<Uuid>>,
) -> Result<TeamOrders, AppError> {
    state
        .run(move |conn| give_task_impl(conn, task_id, person_id, steps, minimap_store::today()))
        .await
}

pub(crate) fn give_task_impl(
    conn: &mut Connection,
    task_id: Uuid,
    person_id: Uuid,
    steps: Vec<Vec<Uuid>>,
    today: Date,
) -> Result<TeamOrders, AppError> {
    minimap_store::order::give(conn, task_id, person_id, steps).map_err(store_error)?;
    minimap_store::order::sync_dates(conn, today).map_err(store_error)?;
    team_at(conn, today)
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
    let summary = NodeSummary {
        node: NodeRef::new(NodeType::Person, person.id),
        label: person.name,
        archived: person.archived_at.is_some(),
    };
    let mut orders = orders_at(conn, today, vec![summary])?;
    orders
        .pop()
        .ok_or_else(|| app_error("internal", "no order was worked out"))
}

/// [`get_team_orders`] as of `today`: everyone in Minimap who isn't archived, you first, then the
/// others by name, each with this week's load as the Capacity screen shows it.
pub(crate) fn team_at(conn: &Connection, today: Date) -> Result<TeamOrders, AppError> {
    let mut people = minimap_store::people::list(conn, false).map_err(store_error)?;
    people.retain(|p| p.archived_at.is_none());
    people.sort_by(|a, b| {
        b.is_self
            .cmp(&a.is_self)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then(a.id.cmp(&b.id))
    });
    // Today's week, through the same code the Capacity screen runs.
    let capacity = crate::commands::capacity::capacity_at(conn, today, None, None, Some(1))?;
    let load_of = |id: Uuid| {
        capacity
            .people
            .iter()
            .find(|c| c.person.node.id == id)
            .and_then(|c| {
                c.weeks
                    .iter()
                    .find(|w| w.week_start == capacity.current_week_start)
            })
            .filter(|w| w.capacity_days > 0.0)
            .map(|w| w.load_pct.round().max(0.0) as u32)
    };
    let summaries = people
        .iter()
        .map(|p| NodeSummary {
            node: NodeRef::new(NodeType::Person, p.id),
            label: p.name.clone(),
            archived: false,
        })
        .collect();
    let orders = orders_at(conn, today, summaries)?;
    Ok(TeamOrders {
        members: people
            .iter()
            .zip(orders)
            .map(|(p, order)| TeamMember {
                order,
                is_self: p.is_self,
                load_pct: load_of(p.id),
            })
            .collect(),
    })
}

/// Each person's order from one portfolio schedule, with the ruler the screen draws on.
fn orders_at(
    conn: &Connection,
    today: Date,
    people: Vec<NodeSummary>,
) -> Result<Vec<WorkOrder>, AppError> {
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
    Ok(people
        .into_iter()
        .map(|summary| {
            let mut o = minimap_core::order::view(summary, &tasks, &edges, &projects, &schedule);
            o.ruler = minimap_core::order::ruler(&o, today, work_week);
            o.today = Some(today);
            o
        })
        .collect())
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

    fn reports_to(conn: &mut Connection, who: Uuid, boss: Uuid) {
        use minimap_types::{EdgeType, NewEdge};
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

    fn assignees(conn: &Connection, task: Uuid) -> Vec<Uuid> {
        minimap_store::edges::list_for_node(conn, task, false)
            .unwrap()
            .into_iter()
            .filter(|e| e.edge_type == minimap_types::EdgeType::AssignedTo)
            .map(|e| e.to_id)
            .collect()
    }

    #[test]
    fn the_team_is_you_then_everyone_else_by_name_each_with_a_ruler() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        assert!(team_at(&conn, TODAY).unwrap().members.is_empty());
        let me = person(&mut conn, "Me", true);
        let zoe = person(&mut conn, "Zoe", false);
        let raj = person(&mut conn, "Raj", false);
        let _ann = person(&mut conn, "ann", false);
        let gone = person(&mut conn, "Bob", false);
        minimap_store::nodes::archive(&mut conn, NodeRef::new(NodeType::Person, gone)).unwrap();
        // Reporting lines don't matter: everyone is on the team.
        reports_to(&mut conn, zoe, me);
        let a = task(&mut conn, "A", raj, 2.0);
        set_order_impl(&mut conn, raj, vec![vec![a]], TODAY).unwrap();
        let team = team_at(&conn, TODAY).unwrap();
        let names: Vec<_> = team
            .members
            .iter()
            .map(|m| m.order.person.label.as_str())
            .collect();
        assert_eq!(names, ["Me", "ann", "Raj", "Zoe"]);
        assert!(team.members[0].is_self && !team.members[2].is_self);
        let raj_col = &team.members[2];
        assert_eq!(raj_col.order.steps.len(), 1);
        // 2 days of work in a 5-day week, as the Capacity screen counts it.
        assert_eq!(raj_col.load_pct, Some(40));
        let ruler = &raj_col.order.ruler;
        assert_eq!(ruler.len(), minimap_core::order::RULER_DAYS);
        assert_eq!(ruler[0], TODAY);
        assert_eq!(raj_col.order.today, Some(TODAY));
    }

    #[test]
    fn giving_a_task_reassigns_and_orders_it_in_one_change() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let me = person(&mut conn, "Me", true);
        let raj = person(&mut conn, "Raj", false);
        let a = task(&mut conn, "A", me, 2.0);
        let b = task(&mut conn, "B", raj, 1.0);
        set_order_impl(&mut conn, me, vec![vec![a]], TODAY).unwrap();
        set_order_impl(&mut conn, raj, vec![vec![b]], TODAY).unwrap();
        // A goes to Raj, before B.
        let team = give_task_impl(&mut conn, a, raj, vec![vec![a], vec![b]], TODAY).unwrap();
        assert_eq!(assignees(&conn, a), [raj]);
        let theirs = &team.members[1].order;
        assert_eq!(theirs.steps.len(), 2);
        assert_eq!(theirs.steps[0].cards[0].id, a);
        assert_eq!(theirs.steps[1].start, Some(date!(2027 - 03 - 05)));
        assert!(team.members[0].order.steps.is_empty());
        // Given without a place: it waits in their tray.
        give_task_impl(&mut conn, a, me, vec![], TODAY).unwrap();
        let mine = order_at(&conn, TODAY, Some(me)).unwrap();
        assert_eq!(mine.unordered.iter().map(|c| c.id).collect::<Vec<_>>(), [a]);
        // A refused order leaves the task with the person it had.
        let other = task(&mut conn, "Other", me, 1.0);
        let err = give_task_impl(&mut conn, a, raj, vec![vec![a, other]], TODAY).unwrap_err();
        assert_eq!(err.code, "invalid");
        assert_eq!(assignees(&conn, a), [me]);
    }
}
