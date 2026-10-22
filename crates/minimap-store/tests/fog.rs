//! Fog of war (spec 43): confirming an item is a logged, undoable update of `confirmed_on` that is
//! not an edit; the reads the fog rules need; the `fog` setting.

use minimap_core::undo::{plan, Plan};
use minimap_store::*;
use minimap_types::*;
use time::macros::date;

const TODAY: time::Date = date!(2027 - 03 - 03);

fn new_task(conn: &mut Connection, title: &str) -> Task {
    let __p = minimap_store::fixtures::project(conn);
    tasks::create(
        conn,
        CreateTask {
            links: Vec::new(),
            task_type: Some("build".into()),
            focus: None,
            backburner_since: None,
            start_minute: None,
            length_minutes: None,
            title: title.into(),
            assignee: AssigneeChoice::Nobody,
            description: String::new(),
            project_id: Some(__p),
            status: Some(TaskStatus::InProgress),
            estimate_days: None,
            start_date: None,
            due_date: None,
            priority: None,
            recurrence: None,
        },
    )
    .unwrap()
}

fn new_project(conn: &mut Connection, title: &str) -> Project {
    projects::create(
        conn,
        CreateProject {
            title: title.into(),
            slug: None,
            description: String::new(),
            owner_person_id: None,
            start_date: None,
            target_date: None,
            status: Some(ProjectStatus::Active),
            priority: None,
        },
    )
    .unwrap()
}

fn new_objective(conn: &mut Connection, title: &str) -> Objective {
    objectives::create(
        conn,
        CreateObjective {
            title: title.into(),
            description: String::new(),
            target_date: None,
            status: None,
            priority: None,
            ongoing: false,
            review_every_days: None,
        },
    )
    .unwrap()
}

/// What `f` wrote.
fn wrote(conn: &mut Connection, f: impl FnOnce(&mut Connection)) -> Vec<Activity> {
    let marker = activity::latest_rowid(conn).unwrap();
    f(conn);
    activity::since(conn, marker).unwrap()
}

fn undo_rows(conn: &mut Connection, rows: &[Activity]) -> Vec<Activity> {
    match plan(rows) {
        Plan::Steps(steps) => undo::apply(conn, &steps).unwrap(),
        other => panic!("expected steps, got {other:?}"),
    }
}

#[test]
fn confirming_a_task_is_one_logged_update_that_leaves_updated_at_alone() {
    let mut conn = open_in_memory().unwrap();
    let t = new_task(&mut conn, "Ship the thing");
    let rows = wrote(&mut conn, |c| {
        fog::confirm(c, NodeRef::new(NodeType::Task, t.id), TODAY).unwrap();
    });
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].action, ActivityAction::Updated);
    assert_eq!(rows[0].diff.as_object().map(|d| d.len()), Some(1));
    assert_eq!(
        rows[0].diff["confirmed_on"],
        serde_json::json!([null, "2027-03-03"])
    );
    let after = tasks::get(&conn, t.id).unwrap();
    assert_eq!(after.confirmed_on, Some(TODAY));
    // Not an edit: "last changed" still means a change.
    assert_eq!(after.updated_at, t.updated_at);
}

#[test]
fn a_real_edit_still_moves_updated_at() {
    let mut conn = open_in_memory().unwrap();
    let t = new_task(&mut conn, "Ship the thing");
    std::thread::sleep(std::time::Duration::from_millis(5));
    let edited = tasks::update(
        &mut conn,
        t.id,
        UpdateTask {
            title: Some("Ship the thing now".into()),
            confirmed_on: Patch::Set(TODAY),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(edited.updated_at > t.updated_at);
}

#[test]
fn confirming_again_on_the_same_day_writes_nothing() {
    let mut conn = open_in_memory().unwrap();
    let p = new_project(&mut conn, "Platform");
    fog::confirm(&mut conn, NodeRef::new(NodeType::Project, p.id), TODAY).unwrap();
    let rows = wrote(&mut conn, |c| {
        fog::confirm(c, NodeRef::new(NodeType::Project, p.id), TODAY).unwrap();
    });
    assert!(rows.is_empty());
}

#[test]
fn confirming_is_undone_and_redone_exactly_for_each_kind() {
    let mut conn = open_in_memory().unwrap();
    let t = new_task(&mut conn, "A task");
    let p = new_project(&mut conn, "A project");
    let o = new_objective(&mut conn, "An objective");
    for node in [
        NodeRef::new(NodeType::Task, t.id),
        NodeRef::new(NodeType::Project, p.id),
        NodeRef::new(NodeType::Objective, o.id),
    ] {
        let read = |conn: &Connection| match node.node_type {
            NodeType::Task => tasks::get(conn, node.id).unwrap().confirmed_on,
            NodeType::Project => projects::get(conn, node.id).unwrap().confirmed_on,
            _ => objectives::get(conn, node.id).unwrap().confirmed_on,
        };
        let updated = |conn: &Connection| match node.node_type {
            NodeType::Task => tasks::get(conn, node.id).unwrap().updated_at,
            NodeType::Project => projects::get(conn, node.id).unwrap().updated_at,
            _ => objectives::get(conn, node.id).unwrap().updated_at,
        };
        let stamp = updated(&conn);
        let rows = wrote(&mut conn, |c| fog::confirm(c, node, TODAY).unwrap());
        assert_eq!(read(&conn), Some(TODAY));
        let undone = undo_rows(&mut conn, &rows);
        assert_eq!(read(&conn), None, "{:?}", node.node_type);
        // The fog an undo brings back is the fog there was.
        assert_eq!(updated(&conn), stamp);
        let redone = undo_rows(&mut conn, &undone);
        assert_eq!(read(&conn), Some(TODAY));
        undo_rows(&mut conn, &redone);
        assert_eq!(read(&conn), None);
    }
}

#[test]
fn archived_items_and_other_nodes_are_refused() {
    let mut conn = open_in_memory().unwrap();
    let t = new_task(&mut conn, "Gone");
    let node = NodeRef::new(NodeType::Task, t.id);
    nodes::archive(&mut conn, node).unwrap();
    let err = fog::confirm(&mut conn, node, TODAY).unwrap_err();
    assert!(err.to_string().contains("archived"), "{err}");

    let n = notes::create(
        &mut conn,
        CreateNote {
            title: "A note".into(),
            body: String::new(),
            note_date: None,
            kind: None,
            recurrence: None,
        },
    )
    .unwrap();
    let err = fog::confirm(&mut conn, NodeRef::new(NodeType::Note, n.id), TODAY).unwrap_err();
    assert!(err.to_string().contains("task, a project or an objective"));
}

#[test]
fn the_newest_link_change_of_each_from_node_is_found() {
    let mut conn = open_in_memory().unwrap();
    let a = new_task(&mut conn, "A");
    let b = new_task(&mut conn, "B");
    assert!(views::last_link_changes(&conn).unwrap().is_empty());
    edges::add(
        &mut conn,
        NewEdge {
            edge_type: EdgeType::Blocks,
            from: NodeRef::new(NodeType::Task, a.id),
            to: NodeRef::new(NodeType::Task, b.id),
            attrs: serde_json::json!({}),
        },
    )
    .unwrap();
    let changes = views::last_link_changes(&conn).unwrap();
    // The log attaches a link to the node it starts from.
    assert!(changes.contains_key(&a.id));
    assert!(!changes.contains_key(&b.id));
}

#[test]
fn the_newest_note_that_mentions_a_node_is_found() {
    let mut conn = open_in_memory().unwrap();
    let t = new_task(&mut conn, "Ship");
    assert!(views::last_mentions(&conn).unwrap().is_empty());
    for (title, day) in [
        ("older", date!(2027 - 02 - 20)),
        ("newer", date!(2027 - 02 - 25)),
    ] {
        notes::create(
            &mut conn,
            CreateNote {
                title: title.into(),
                body: format!("About {}", mention_token("Ship", t.id)),
                note_date: Some(day),
                kind: None,
                recurrence: None,
            },
        )
        .unwrap();
    }
    let found = views::last_mentions(&conn).unwrap();
    assert_eq!(found.get(&t.id), Some(&date!(2027 - 02 - 25)));
    assert_eq!(found.len(), 1);
}

#[test]
fn the_fog_setting_has_defaults_validates_and_persists() {
    let mut conn = open_in_memory().unwrap();
    assert_eq!(settings::get(&conn).unwrap().fog, FogThresholds::default());
    let wanted = FogThresholds {
        task_haze: 30,
        task_fog: 45,
        ..Default::default()
    };
    let saved = settings::update(
        &mut conn,
        UpdateSettings {
            fog: Some(wanted),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(saved.fog, wanted);
    assert_eq!(settings::get(&conn).unwrap().fog, wanted);

    // A bad value is refused with a message and leaves the stored one.
    let err = settings::update(
        &mut conn,
        UpdateSettings {
            fog: Some(FogThresholds {
                project_haze: 40,
                project_fog: 40,
                ..Default::default()
            }),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("haze must come before fog"),
        "{err}"
    );
    assert_eq!(settings::get(&conn).unwrap().fog, wanted);
}

#[test]
fn fog_follows_you_across_devices_so_it_is_not_a_local_setting() {
    assert!(!settings::LOCAL_ONLY.contains(&"fog"));
}
