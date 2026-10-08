//! Removing the demo data (spec 24): exact, safe for the user's own items, all or nothing.

use minimap_store::*;
use minimap_types::*;
use time::macros::date;

const TODAY: time::Date = date!(2027 - 03 - 03);

fn seeded() -> Connection {
    let mut conn = open_in_memory().unwrap();
    demo::seed(&mut conn, TODAY).unwrap();
    conn
}

fn count(conn: &Connection, table: &str) -> u32 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

fn new_task(title: &str, project: Option<uuid::Uuid>) -> CreateTask {
    CreateTask {
        links: Vec::new(),
        task_type: None,
        title: title.into(),
        assignee: AssigneeChoice::Nobody,
        description: String::new(),
        project_id: project,
        status: None,
        estimate_days: None,
        start_date: None,
        due_date: None,
        priority: None,
        recurrence: None,
    }
}

fn demo_project(conn: &Connection) -> Project {
    projects::list(conn, false)
        .unwrap()
        .into_iter()
        .find(|p| p.title.contains("EU Region"))
        .unwrap()
}

/// The `n`-th person who is not me.
fn demo_person(conn: &Connection, n: usize) -> Person {
    people::list(conn, false)
        .unwrap()
        .into_iter()
        .filter(|p| !p.is_self)
        .nth(n)
        .unwrap()
}

#[test]
fn nothing_to_remove_in_an_empty_or_ordinary_database() {
    let mut conn = open_in_memory().unwrap();
    assert!(!demo_remove::status(&conn).unwrap().found);
    assert_eq!(
        demo_remove::remove(&mut conn).unwrap_err().to_string(),
        "invalid value: There is no demo data to remove."
    );
    people::ensure_self(&mut conn, "Me").unwrap();
    tasks::create(&mut conn, new_task("Write the handbook", None)).unwrap();
    assert!(!demo_remove::status(&conn).unwrap().found);
    assert_eq!(count(&conn, "tasks"), 1);
}

#[test]
fn seeded_data_is_found_by_its_recorded_ids_and_counted_without_me() {
    let conn = seeded();
    let status = demo_remove::status(&conn).unwrap();
    assert!(status.found);
    assert_eq!(status.source, Some(DemoSource::Recorded));
    assert_eq!(status.items.objectives, 3);
    assert_eq!(status.items.projects, 3);
    assert_eq!(status.items.tasks, 40);
    assert_eq!(status.items.people, 7, "seven others; me is not demo data");
    assert_eq!(status.items.links, 101);
    assert_eq!(status.impact, DemoImpact::default());
    // Looking changes nothing.
    assert_eq!(count(&conn, "tasks"), 40);
}

#[test]
fn removing_leaves_an_empty_database_that_can_be_seeded_again() {
    let mut conn = seeded();
    let (removed, impact) = demo_remove::remove(&mut conn).unwrap();
    assert_eq!(removed.tasks, 40);
    assert_eq!(impact, DemoImpact::default());
    for table in [
        "objectives",
        "projects",
        "tasks",
        "teams",
        "notes",
        "decisions",
        "waiting_on",
        "edges",
    ] {
        assert_eq!(count(&conn, table), 0, "{table}");
    }
    assert_eq!(count(&conn, "people"), 1, "only me");
    assert!(people::get_self(&conn).unwrap().is_some());
    // No history of the demo data, and the marker is gone.
    let me = people::get_self(&conn).unwrap().unwrap();
    let others: u32 = conn
        .query_row(
            "SELECT COUNT(*) FROM activity WHERE node_id != ?1",
            [me.id.to_string()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(others, 0, "only me has history left");
    assert!(meta::get(&conn, meta::DEMO_ITEMS).unwrap().is_none());
    assert!(!demo_remove::status(&conn).unwrap().found);
    // The deletions are tombstoned, so they reach other computers.
    assert!(count(&conn, "tombstones") >= 40);
    // A fresh start: seeding works again and finds everything again.
    demo::seed(&mut conn, TODAY).unwrap();
    assert_eq!(demo_remove::status(&conn).unwrap().items.tasks, 40);
}

#[test]
fn your_own_items_stay_and_are_detached_from_the_demo_items() {
    let mut conn = seeded();
    let project = demo_project(&conn);
    let person = demo_person(&conn, 0);
    let owner = demo_person(&conn, 1);
    let objective = objectives::list(&conn, false).unwrap().remove(0);

    // Yours: a task in a demo project, linked to a demo objective; a project owned by a demo
    // person; a team inside a demo team; a waiting-on about a demo person.
    let task = tasks::create(&mut conn, new_task("My own task", Some(project.id))).unwrap();
    edges::add(
        &mut conn,
        NewEdge {
            edge_type: EdgeType::ContributesTo,
            from: NodeRef::new(NodeType::Task, task.id),
            to: NodeRef::new(NodeType::Objective, objective.id),
            attrs: serde_json::json!({ "weight": 1.0 }),
        },
    )
    .unwrap();
    let mine = projects::create(
        &mut conn,
        CreateProject {
            title: "My own project".into(),
            slug: None,
            description: String::new(),
            owner_person_id: Some(owner.id),
            start_date: None,
            target_date: None,
            status: None,
            priority: None,
        },
    )
    .unwrap();
    let parent = teams::list(&conn, false)
        .unwrap()
        .into_iter()
        .find(|t| t.parent_team_id.is_none())
        .unwrap();
    let team = teams::create(
        &mut conn,
        CreateTeam {
            name: "My own team".into(),
            description: String::new(),
            parent_team_id: Some(parent.id),
        },
    )
    .unwrap();
    let waiting = waiting_on::create(
        &mut conn,
        CreateWaitingOn {
            description: "My own waiting-on".into(),
            person_id: person.id,
            asked_on: None,
            expected_by: None,
            follow_up_on: None,
        },
    )
    .unwrap();

    let status = demo_remove::status(&conn).unwrap();
    assert_eq!(
        status.impact,
        DemoImpact {
            your_links: 1,
            tasks_to_inbox: 1,
            owners_cleared: 1,
            teams_unparented: 1,
            people_kept: 1,
        }
    );
    assert_eq!(status.items.people, 6, "the kept person is not removed");

    demo_remove::remove(&mut conn).unwrap();

    let task = tasks::get(&conn, task.id).unwrap();
    assert_eq!(
        (task.title.as_str(), task.project_id),
        ("My own task", None)
    );
    assert_eq!(projects::get(&conn, mine.id).unwrap().owner_person_id, None);
    assert_eq!(teams::get(&conn, team.id).unwrap().parent_team_id, None);
    assert_eq!(
        waiting_on::get(&conn, waiting.id).unwrap().person_id,
        person.id
    );
    assert_eq!(people::get(&conn, person.id).unwrap().id, person.id);
    // The link to the demo objective went with it.
    let contributes: u32 = conn
        .query_row(
            "SELECT COUNT(*) FROM edges WHERE edge_type = 'contributes_to'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(contributes, 0);
    assert_eq!(count(&conn, "tasks"), 1);
    assert_eq!(count(&conn, "projects"), 1);
    assert_eq!(count(&conn, "people"), 2, "me and the kept person");
    assert_eq!(count(&conn, "teams"), 1);
}

#[test]
fn demo_data_from_before_ids_were_recorded_is_recognised_by_its_titles() {
    let mut conn = seeded();
    meta::remove(&conn, meta::DEMO_ITEMS).unwrap();
    // The user's own task has a title of its own and is left alone.
    tasks::create(&mut conn, new_task("Plan the offsite", None)).unwrap();
    let status = demo_remove::status(&conn).unwrap();
    assert_eq!(status.source, Some(DemoSource::Titles));
    assert_eq!(status.items.tasks, 40);
    assert_eq!(status.items.projects, 3);

    demo_remove::remove(&mut conn).unwrap();
    assert_eq!(count(&conn, "tasks"), 1);
    assert_eq!(count(&conn, "projects"), 0);
    assert!(!demo_remove::status(&conn).unwrap().found);
}

#[test]
fn a_few_titles_in_common_are_not_taken_for_the_demo_data() {
    let conn = seeded();
    let titles: Vec<String> = tasks::list(&conn, false)
        .unwrap()
        .into_iter()
        .map(|t| t.title)
        .take(2)
        .collect();
    drop(conn);

    let mut conn = open_in_memory().unwrap();
    people::ensure_self(&mut conn, "Me").unwrap();
    for t in titles {
        tasks::create(&mut conn, new_task(&t, None)).unwrap();
    }
    assert!(!demo_remove::status(&conn).unwrap().found);
    assert!(demo_remove::remove(&mut conn).is_err());
    assert_eq!(count(&conn, "tasks"), 2);
}

#[test]
fn a_renamed_recorded_item_is_still_removed_and_an_archived_one_too() {
    let mut conn = seeded();
    let project = demo_project(&conn);
    projects::update(
        &mut conn,
        project.id,
        UpdateProject {
            title: Some("Renamed by me".into()),
            ..Default::default()
        },
    )
    .unwrap();
    nodes::archive(&mut conn, NodeRef::new(NodeType::Project, project.id)).unwrap();
    let status = demo_remove::status(&conn).unwrap();
    assert_eq!(
        status.items.projects, 3,
        "recorded ids do not depend on titles"
    );
    demo_remove::remove(&mut conn).unwrap();
    assert_eq!(count(&conn, "projects"), 0);
}
