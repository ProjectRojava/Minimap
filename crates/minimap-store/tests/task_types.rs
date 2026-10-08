//! Task types (spec 32): the list in Settings, the type on a task, what is allowed on the way in.

use minimap_store::*;
use minimap_types::*;
use time::macros::date;

fn db() -> Connection {
    open_in_memory().unwrap()
}

fn task(conn: &mut Connection, task_type: Option<&str>) -> Result<Task> {
    tasks::create(
        conn,
        CreateTask {
            links: Vec::new(),
            task_type: task_type.map(str::to_owned),
            title: "Pick the data store".into(),
            assignee: AssigneeChoice::Nobody,
            description: String::new(),
            project_id: None,
            status: None,
            estimate_days: None,
            start_date: None,
            due_date: Some(date!(2027 - 03 - 01)),
            priority: None,
            recurrence: None,
        },
    )
}

fn set_types(conn: &mut Connection, list: Vec<TaskType>) -> Result<Settings> {
    settings::update(
        conn,
        UpdateSettings {
            task_types: Some(list),
            ..Default::default()
        },
    )
}

fn set_type(conn: &mut Connection, id: uuid::Uuid, t: Patch<String>) -> Result<Task> {
    tasks::update(
        conn,
        id,
        UpdateTask {
            task_type: t,
            ..Default::default()
        },
    )
}

#[test]
fn a_new_installation_has_the_default_types() {
    let conn = db();
    let s = settings::get(&conn).unwrap();
    assert_eq!(s.task_types, default_task_types());
}

#[test]
fn a_task_has_a_type_or_none() {
    let mut conn = db();
    assert_eq!(task(&mut conn, None).unwrap().task_type, None);
    let t = task(&mut conn, Some("decision")).unwrap();
    assert_eq!(t.task_type.as_deref(), Some("decision"));
    assert_eq!(
        tasks::get(&conn, t.id).unwrap().task_type.as_deref(),
        Some("decision")
    );
    let t = set_type(&mut conn, t.id, Patch::Set("design".into())).unwrap();
    assert_eq!(t.task_type.as_deref(), Some("design"));
    let t = set_type(&mut conn, t.id, Patch::Clear).unwrap();
    assert_eq!(t.task_type, None);
}

#[test]
fn a_type_that_is_not_in_the_list_is_refused_and_nothing_is_written() {
    let mut conn = db();
    let err = task(&mut conn, Some("nonsense")).unwrap_err();
    assert!(err.to_string().contains("not in the list"), "{err}");
    assert!(tasks::list(&conn, true).unwrap().is_empty());
    let t = task(&mut conn, None).unwrap();
    assert!(set_type(&mut conn, t.id, Patch::Set("nonsense".into())).is_err());
    assert_eq!(tasks::get(&conn, t.id).unwrap().task_type, None);
}

#[test]
fn an_archived_type_stays_on_its_tasks_but_cannot_be_given_again() {
    let mut conn = db();
    let t = task(&mut conn, Some("bug")).unwrap();
    let mut list = default_task_types();
    list.iter_mut().find(|x| x.id == "bug").unwrap().archived = true;
    set_types(&mut conn, list).unwrap();
    // The task keeps it and can still be edited.
    let t = tasks::update(
        &mut conn,
        t.id,
        UpdateTask {
            title: Some("Fix the crash".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(t.task_type.as_deref(), Some("bug"));
    // Nobody else can get it.
    let err = task(&mut conn, Some("bug")).unwrap_err();
    assert!(err.to_string().contains("archived"), "{err}");
    let other = task(&mut conn, None).unwrap();
    assert!(set_type(&mut conn, other.id, Patch::Set("bug".into())).is_err());
}

#[test]
fn the_list_can_be_extended_renamed_and_recoloured_but_not_shortened() {
    let mut conn = db();
    let mut list = default_task_types();
    list[0].name = "UX design".into();
    list[0].hue = 340;
    list.push(TaskType {
        id: String::new(),
        name: "Legal review".into(),
        hue: 75,
        archived: false,
    });
    let s = set_types(&mut conn, list).unwrap();
    assert_eq!(s.task_types[0].id, "design");
    assert_eq!(s.task_types[0].name, "UX design");
    assert_eq!(s.task_types[7].id, "legal-review");
    // It survives a reload and a new type can be used at once.
    assert_eq!(settings::get(&conn).unwrap().task_types, s.task_types);
    task(&mut conn, Some("legal-review")).unwrap();

    let mut fewer = s.task_types.clone();
    fewer.remove(0);
    let err = set_types(&mut conn, fewer).unwrap_err();
    assert!(err.to_string().contains("only archived"), "{err}");
    assert_eq!(settings::get(&conn).unwrap().task_types, s.task_types);
}

#[test]
fn a_bad_list_changes_nothing_else_in_the_same_update() {
    let mut conn = db();
    let mut list = default_task_types();
    list.push(TaskType {
        id: String::new(),
        name: " ".into(),
        hue: 10,
        archived: false,
    });
    let err = settings::update(
        &mut conn,
        UpdateSettings {
            hours_per_day: Some(6.0),
            task_types: Some(list),
            ..Default::default()
        },
    );
    assert!(err.is_err());
    assert_eq!(settings::get(&conn).unwrap().hours_per_day, 8.0);
}

#[test]
fn a_stored_list_that_no_longer_reads_falls_back_to_the_defaults() {
    let conn = db();
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('task_types', '[{\"id\":\"\",\"name\":\"x\",\"hue\":1}]')",
        [],
    )
    .unwrap();
    assert_eq!(settings::task_types(&conn).unwrap(), default_task_types());
}

#[test]
fn a_repeating_task_hands_its_type_on() {
    let mut conn = db();
    let t = tasks::create(
        &mut conn,
        CreateTask {
            links: Vec::new(),
            task_type: Some("admin".into()),
            title: "Access review".into(),
            assignee: AssigneeChoice::Nobody,
            description: String::new(),
            project_id: None,
            status: None,
            estimate_days: None,
            start_date: None,
            due_date: Some(date!(2027 - 03 - 01)),
            priority: None,
            recurrence: Some(Recurrence::from(Cadence::Daily)),
        },
    )
    .unwrap();
    tasks::update(
        &mut conn,
        t.id,
        UpdateTask {
            status: Some(TaskStatus::Done),
            ..Default::default()
        },
    )
    .unwrap();
    let all = tasks::list(&conn, false).unwrap();
    assert_eq!(all.len(), 2);
    assert!(all.iter().all(|t| t.task_type.as_deref() == Some("admin")));
}

#[test]
fn a_linked_task_can_be_given_a_type() {
    let mut conn = db();
    let source = task(&mut conn, None).unwrap();
    let made = tasks::create_linked(
        &mut conn,
        source.id,
        LinkRelation::BlockedBy,
        "Decide".into(),
        Some("decision".into()),
    )
    .unwrap();
    assert_eq!(made.task_type.as_deref(), Some("decision"));
    assert!(tasks::create_linked(
        &mut conn,
        source.id,
        LinkRelation::BlockedBy,
        "Decide".into(),
        Some("nonsense".into()),
    )
    .is_err());
}

#[test]
fn the_due_date_history_is_in_the_activity_log() {
    let mut conn = db();
    let t = task(&mut conn, Some("decision")).unwrap();
    for due in [date!(2027 - 03 - 08), date!(2027 - 03 - 15)] {
        tasks::update(
            &mut conn,
            t.id,
            UpdateTask {
                due_date: Patch::Set(due),
                ..Default::default()
            },
        )
        .unwrap();
    }
    let rows = activity::list_for_node(&conn, t.id).unwrap();
    let history = plan_history(&rows);
    assert_eq!(history.first, Some(date!(2027 - 03 - 01)));
    assert_eq!(history.moves, 2);
}
