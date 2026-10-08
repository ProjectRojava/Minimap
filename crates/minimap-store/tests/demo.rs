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
    assert_eq!(summary.objectives, 3);
    assert_eq!(summary.projects, 3);
    assert_eq!(summary.tasks, 40);
    assert_eq!(summary.people, 8, "me and seven others");
    assert_eq!(summary.teams, 2);
    assert_eq!(summary.notes, 3);
    assert_eq!(summary.decisions, 5);
    assert_eq!(summary.waiting_ons, 3);
    assert_eq!(summary.links, 106);
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
            links: Vec::new(),
            task_type: None,
            title: "Mine".into(),
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

#[test]
fn one_objective_is_ongoing_with_an_overdue_review_and_served_by_repeating_tasks() {
    let conn = seeded(TODAY);
    let ongoing: Vec<Objective> = objectives::list(&conn, false)
        .unwrap()
        .into_iter()
        .filter(|o| o.ongoing)
        .collect();
    assert_eq!(ongoing.len(), 1);
    let o = &ongoing[0];
    assert!(o.target_date.is_none() && o.review_every_days == Some(30));
    assert_eq!(o.review_overdue_days(TODAY), Some(5));
    let served = edges::list_active_of_type(&conn, EdgeType::ContributesTo)
        .unwrap()
        .into_iter()
        .filter(|e| e.to_id == o.id)
        .count();
    assert_eq!(served, 2);
}

#[test]
fn a_few_tasks_carry_reference_links() {
    let conn = seeded(TODAY);
    let with: Vec<(String, usize)> = tasks::list(&conn, false)
        .unwrap()
        .into_iter()
        .filter(|t| !t.links.is_empty())
        .map(|t| (t.title, t.links.len()))
        .collect();
    assert_eq!(with.len(), 3, "{with:?}");
    let all: Vec<RefLink> = tasks::list(&conn, false)
        .unwrap()
        .into_iter()
        .flat_map(|t| t.links)
        .collect();
    assert!(all.iter().any(|l| l.kind() == LinkKind::Drive));
    assert!(all.iter().any(|l| l.kind() == LinkKind::Web));
    assert!(
        all.iter().any(|l| l.title.is_empty()),
        "one link has no name"
    );
}

#[test]
fn some_things_repeat() {
    let conn = seeded(TODAY);
    let rules: Vec<(String, String)> = tasks::list(&conn, false)
        .unwrap()
        .into_iter()
        .filter_map(|t| t.recurrence.map(|r| (t.title, r.describe())))
        .collect();
    assert_eq!(rules.len(), 2, "{rules:?}");
    assert!(
        rules.contains(&("Monthly access review".into(), "monthly on the 17th".into())),
        "{rules:?}"
    );
    assert!(
        rules.contains(&(
            "Rotate service credentials".into(),
            "every 12 weeks on Wednesday".into()
        )),
        "{rules:?}"
    );
    // The 1:1 repeats weekly on the day it was held, and every new one starts with its person.
    let note = notes::list(&conn, false)
        .unwrap()
        .into_iter()
        .find(|n| n.recurrence.is_some())
        .unwrap();
    assert_eq!(note.title, "1:1 with Priya");
    let rule = note.recurrence.unwrap();
    assert_eq!(rule.describe(), "every Wednesday");
    assert!(rule.template.unwrap().contains("@[Priya Nair](node:"));
}

#[test]
fn most_tasks_have_a_type_and_one_decision_moved_and_was_decided_late() {
    let conn = seeded(TODAY);
    let types = settings::task_types(&conn).unwrap();
    let all = tasks::list(&conn, false).unwrap();
    let typed: Vec<_> = all.iter().filter(|t| t.task_type.is_some()).collect();
    assert!(
        typed.len() >= 25 && typed.len() < all.len(),
        "{}",
        typed.len()
    );
    // Every type used is in the default list, and each of the main kinds is shown somewhere.
    for t in &typed {
        assert!(TaskType::find(&types, t.task_type.as_deref().unwrap()).is_some());
    }
    for kind in ["design", "build", "decision", "review", "research", "admin"] {
        assert!(
            typed.iter().any(|t| t.task_type.as_deref() == Some(kind)),
            "no {kind} task"
        );
    }
    // The region decision: planned a week before the date it was moved to, finished on it.
    let region = all
        .iter()
        .find(|t| t.title == "Choose the EU cloud region")
        .unwrap();
    assert_eq!(region.task_type.as_deref(), Some("decision"));
    let history = plan_history(&activity::list_for_node(&conn, region.id).unwrap());
    let first = history.first.unwrap();
    let due = region.due_date.unwrap();
    assert_eq!(history.moves, 1);
    assert_eq!((due - first).whole_days(), 7);
    let finished = region.completed_at.unwrap().date();
    assert_eq!(finish_timing(due, finished), Finish::OnTime);
    assert_eq!(finish_timing(first, finished), Finish::Late(7));
}

#[test]
fn some_tasks_are_part_of_others_one_level_deep_and_one_is_done() {
    let conn = seeded(TODAY);
    let all = tasks::list(&conn, false).unwrap();
    let parts: Vec<_> = edges::list_active_of_type(&conn, EdgeType::SubtaskOf).unwrap();
    assert_eq!(parts.len(), 5);
    let pairs: Vec<_> = parts.iter().map(|e| (e.from_id, e.to_id)).collect();
    for (child, parent) in &pairs {
        assert!(!pairs.iter().any(|(c, _)| c == parent), "two levels");
        assert_eq!(pairs.iter().filter(|(c, _)| c == child).count(), 1);
    }
    // The board's chips have something to show: a parent with none done, and one that is all done.
    let rows = views::task_rows(&conn).unwrap();
    let progress = |title: &str| {
        let r = rows.iter().find(|r| r.task.title == title).unwrap();
        (r.subtasks_done, r.subtask_count)
    };
    assert_eq!(progress("Go-live checklist"), (0, 2));
    assert_eq!(progress("Cost dashboard for teams"), (1, 1));
    assert!(all
        .iter()
        .any(|t| t.title == "Weekly cost report automation"));
}
