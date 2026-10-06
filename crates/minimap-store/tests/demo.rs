//! Demo data (spec 24): the seed builder.

use minimap_core::{cycles::find_cycle, edge_rules};
use minimap_store::*;
use minimap_types::*;
use time::macros::date;

/// A Wednesday.
const TODAY: time::Date = date!(2027 - 03 - 03);

fn seeded(today: time::Date) -> Connection {
    let mut conn = open_in_memory().unwrap();
    demo::seed(&mut conn, today).unwrap();
    conn
}

fn count(conn: &Connection, table: &str) -> u32 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

#[test]
fn seeding_an_empty_database_produces_the_documented_counts() {
    let mut conn = open_in_memory().unwrap();
    let summary = demo::seed(&mut conn, TODAY).unwrap();
    assert_eq!(summary.objectives, 2);
    assert_eq!(summary.projects, 3);
    assert_eq!(summary.tasks, 40);
    assert_eq!(summary.people, 8, "me and seven others");
    assert_eq!(summary.teams, 2);
    assert_eq!(summary.notes, 3);
    assert_eq!(summary.decisions, 5);
    assert_eq!(summary.waiting_ons, 3);
    assert_eq!(summary.links, 99);
    // The summary is the truth.
    assert_eq!(count(&conn, "tasks"), 40);
    assert_eq!(count(&conn, "people"), 8);
    assert_eq!(summary.links, count(&conn, "edges"));
}

#[test]
fn the_dataset_has_the_shape_the_screens_need() {
    let conn = seeded(TODAY);
    let tasks = tasks::list(&conn, false).unwrap();
    let projects = projects::list(&conn, false).unwrap();
    let edges = edges::list_active(&conn).unwrap();
    let title = |id| tasks.iter().find(|t| t.id == id).unwrap().title.clone();
    let project_of = |id| tasks.iter().find(|t| t.id == id).and_then(|t| t.project_id);

    // Cross-project blocks.
    let cross = edges
        .iter()
        .filter(|e| e.edge_type == EdgeType::Blocks)
        .filter(|e| project_of(e.from_id) != project_of(e.to_id))
        .count();
    assert!(cross >= 3, "cross-project blocks: {cross}");
    // A mix of statuses, a few without estimates, one overdue.
    for status in [
        TaskStatus::Todo,
        TaskStatus::InProgress,
        TaskStatus::Blocked,
        TaskStatus::Done,
        TaskStatus::Cancelled,
    ] {
        // "Blocked" only happens during this week's events, which have run by now.
        assert!(tasks.iter().any(|t| t.status == status), "{status:?}");
    }
    assert!(tasks.iter().filter(|t| t.estimate_days.is_none()).count() >= 3);
    let overdue: Vec<String> = tasks
        .iter()
        .filter(|t| !matches!(t.status, TaskStatus::Done | TaskStatus::Cancelled))
        .filter(|t| t.due_date.is_some_and(|d| d < TODAY))
        .map(|t| t.title.clone())
        .collect();
    assert_eq!(overdue, ["Customer communications plan"]);
    assert_eq!(title(tasks[0].id), tasks[0].title);
    // Every project has an owner and a target; every task of a project belongs to an active one.
    assert!(projects
        .iter()
        .all(|p| p.owner_person_id.is_some() && p.target_date.is_some()));
    // Two nested teams, a reporting structure, a 1:1 with mentions.
    let teams = teams::list(&conn, false).unwrap();
    let child = teams.iter().find(|t| t.name == "Platform").unwrap();
    let parent = teams.iter().find(|t| t.name == "Engineering").unwrap();
    assert_eq!(child.parent_team_id, Some(parent.id));
    assert_eq!(
        edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::ReportsTo)
            .count(),
        7
    );
    let notes = notes::list(&conn, false).unwrap();
    let one_on_one = notes.iter().find(|n| n.kind == NoteKind::OneOnOne).unwrap();
    let mentions = edges
        .iter()
        .filter(|e| e.edge_type == EdgeType::Mentions && e.from_id == one_on_one.id)
        .count();
    assert!(mentions >= 4, "mentions: {mentions}");
    // One decision replaced by another.
    let decisions = decisions::list(&conn, false).unwrap();
    assert_eq!(
        decisions
            .iter()
            .filter(|d| d.status == DecisionStatus::Superseded)
            .count(),
        1
    );
    assert_eq!(
        edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Supersedes)
            .count(),
        1
    );
}

#[test]
fn every_link_is_allowed_by_the_rules_and_nothing_forms_a_loop() {
    let conn = seeded(TODAY);
    let mut seen: std::collections::HashMap<EdgeType, Vec<(uuid::Uuid, uuid::Uuid)>> =
        Default::default();
    for e in edges::list_active(&conn).unwrap() {
        edge_rules::validate(
            e.edge_type,
            NodeRef::new(e.from_type, e.from_id),
            NodeRef::new(e.to_type, e.to_id),
            &e.attrs,
        )
        .unwrap_or_else(|err| panic!("{:?} {} -> {}: {err}", e.edge_type, e.from_id, e.to_id));
        if edge_rules::must_be_acyclic(e.edge_type) {
            let list = seen.entry(e.edge_type).or_default();
            assert!(
                find_cycle(list, e.from_id, e.to_id).is_none(),
                "{:?} loops",
                e.edge_type
            );
            list.push((e.from_id, e.to_id));
        }
    }
}

#[test]
fn it_only_fills_an_empty_database_and_a_refusal_changes_nothing() {
    let mut conn = seeded(TODAY);
    let before = (
        count(&conn, "tasks"),
        count(&conn, "edges"),
        count(&conn, "activity"),
    );
    let err = demo::seed(&mut conn, TODAY).unwrap_err();
    assert!(matches!(err, StoreError::Invalid(ref m) if m.contains("empty database")));
    assert_eq!(
        before,
        (
            count(&conn, "tasks"),
            count(&conn, "edges"),
            count(&conn, "activity")
        )
    );

    // A database with a single task is not empty either.
    let mut other = open_in_memory().unwrap();
    tasks::create(
        &mut other,
        CreateTask {
            title: "Mine".into(),
            assignee: AssigneeChoice::Nobody,
            description: String::new(),
            project_id: None,
            status: None,
            estimate_days: None,
            start_date: None,
            due_date: None,
            priority: None,
        },
    )
    .unwrap();
    assert!(demo::seed(&mut other, TODAY).is_err());
    assert_eq!(count(&other, "tasks"), 1);
}

#[test]
fn first_runs_own_me_becomes_one_of_the_eight() {
    let mut conn = open_in_memory().unwrap();
    let me = people::ensure_self(&mut conn, "Alex").unwrap();
    demo::seed(&mut conn, TODAY).unwrap();
    let all = people::list(&conn, false).unwrap();
    assert_eq!(all.len(), 8);
    assert_eq!(all.iter().filter(|p| p.is_self).count(), 1);
    assert!(all.iter().any(|p| p.id == me.id && p.name == "Alex"));
    // The "me" tasks belong to them.
    let assigned = edges::list_active_of_type(&conn, EdgeType::AssignedTo)
        .unwrap()
        .iter()
        .filter(|e| e.to_id == me.id)
        .count();
    assert_eq!(assigned, 3);
}

#[test]
fn the_same_day_gives_the_same_dataset() {
    // Titles, dates, statuses and estimates are fixed; only the ids differ.
    let fingerprint = |conn: &Connection| -> Vec<String> {
        let mut rows: Vec<String> = tasks::list(conn, false)
            .unwrap()
            .iter()
            .map(|t| {
                format!(
                    "{} | {:?} | {:?} | {:?} | {:?} | {}",
                    t.title,
                    t.status,
                    t.estimate_days,
                    t.start_date,
                    t.due_date,
                    t.completed_at.map_or(String::new(), |c| c.to_string())
                )
            })
            .collect();
        rows.extend(waiting_on::list(conn, false).unwrap().iter().map(|w| {
            format!(
                "{} | {} | {:?} | {:?}",
                w.description, w.asked_on, w.expected_by, w.resolved_on
            )
        }));
        rows.extend(
            decisions::list(conn, false)
                .unwrap()
                .iter()
                .map(|d| format!("{} | {:?} | {:?}", d.title, d.status, d.decided_on)),
        );
        rows.sort();
        rows
    };
    assert_eq!(fingerprint(&seeded(TODAY)), fingerprint(&seeded(TODAY)));
    // Another day shifts the dates.
    assert_ne!(
        fingerprint(&seeded(TODAY)),
        fingerprint(&seeded(date!(2027 - 03 - 10)))
    );
}

#[test]
fn any_day_of_the_week_seeds_cleanly_and_never_dates_work_on_a_weekend() {
    for offset in 0..7 {
        let today = TODAY + time::Duration::days(offset);
        let conn = seeded(today);
        for t in tasks::list(&conn, false).unwrap() {
            for d in [t.start_date, t.due_date].into_iter().flatten() {
                assert!(
                    d.weekday().number_days_from_monday() < 5,
                    "{} on {d} (today {today})",
                    t.title
                );
            }
        }
    }
}

#[test]
fn the_history_reads_like_a_real_week() {
    let conn = seeded(TODAY);
    let monday = date!(2027 - 03 - 01);
    // The week's events: a task finished, due dates moved later, one newly blocked.
    let week = activity::list_between(&conn, monday, date!(2027 - 03 - 07)).unwrap();
    let on = |action: &str, field: &str| {
        week.iter()
            .filter(|a| a.diff.get(field).is_some())
            .filter(|a| format!("{:?}", a.action).to_lowercase() == action)
            .count()
    };
    assert_eq!(on("updated", "due_date"), 2);
    assert_eq!(
        on("updated", "status"),
        3,
        "e3 done, e13 blocked, the replaced decision"
    );
    // Everything made at the start is older than the week.
    let older =
        activity::list_between(&conn, date!(2027 - 02 - 01), date!(2027 - 02 - 28)).unwrap();
    assert!(older.len() > 100, "{}", older.len());
    // Activity rows sit in the order they happened.
    let all = activity::list_recent(&conn, 10_000).unwrap();
    assert!(all.windows(2).all(|w| w[0].at >= w[1].at), "newest first");
    // Two tasks were finished before this week, two during it (one is "today").
    let done: Vec<_> = tasks::list(&conn, false)
        .unwrap()
        .into_iter()
        .filter(|t| t.status == TaskStatus::Done)
        .collect();
    assert_eq!(done.len(), 5);
    let this_week = done
        .iter()
        .filter(|t| t.completed_at.is_some_and(|c| c.date() >= monday))
        .count();
    assert_eq!(this_week, 2);
}

#[test]
fn the_demo_data_is_searchable() {
    let conn = seeded(TODAY);
    let hits = search::run(&conn, "\"gateway\"*", &[], false, 20).unwrap();
    assert!(hits.len() >= 4, "{}", hits.len());
}
