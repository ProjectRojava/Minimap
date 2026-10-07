//! Recurring items (spec 27): finishing a repeating task makes the next one; a repeating note
//! is made on its date.

use minimap_store::*;
use minimap_types::*;
use time::macros::date;
use uuid::Uuid;

fn db() -> Connection {
    open_in_memory().unwrap()
}

fn weekly_monday() -> Recurrence {
    Cadence::Weekly {
        every: 1,
        weekday: 0,
    }
    .into()
}

fn person(conn: &mut Connection, name: &str) -> Person {
    people::create(
        conn,
        CreatePerson {
            name: name.into(),
            role_title: String::new(),
            email: None,
            weekly_capacity_hours: None,
            is_self: false,
            notes: String::new(),
        },
    )
    .unwrap()
}

fn task_with(conn: &mut Connection, f: impl FnOnce(&mut CreateTask)) -> Task {
    let mut input = CreateTask {
        links: Vec::new(),
        title: "Board update".into(),
        assignee: AssigneeChoice::Nobody,
        description: "Numbers and risks".into(),
        project_id: None,
        status: None,
        estimate_days: Some(1.5),
        start_date: None,
        due_date: Some(date!(2027 - 03 - 01)),
        priority: Some(2),
        recurrence: Some(weekly_monday()),
    };
    f(&mut input);
    tasks::create(conn, input).unwrap()
}

fn finish(conn: &mut Connection, id: Uuid) {
    tasks::update(
        conn,
        id,
        UpdateTask {
            status: Some(TaskStatus::Done),
            ..Default::default()
        },
    )
    .unwrap();
}

fn all_tasks(conn: &Connection) -> Vec<Task> {
    tasks::list(conn, false).unwrap()
}

#[test]
fn finishing_a_weekly_task_makes_next_weeks_with_the_same_fields() {
    let mut conn = db();
    let pm = person(&mut conn, "Priya");
    let project = projects::create(
        &mut conn,
        CreateProject {
            title: "Reporting".into(),
            slug: None,
            description: String::new(),
            owner_person_id: None,
            start_date: None,
            target_date: None,
            status: None,
            priority: None,
        },
    )
    .unwrap();
    let objective = objectives::create(
        &mut conn,
        CreateObjective {
            title: "Be transparent".into(),
            description: String::new(),
            target_date: None,
            status: None,
            priority: None,
        },
    )
    .unwrap();
    let task = task_with(&mut conn, |t| {
        t.assignee = AssigneeChoice::Person(pm.id);
        t.project_id = Some(project.id);
        t.start_date = Some(date!(2027 - 02 - 26));
    });
    edges::add(
        &mut conn,
        NewEdge {
            edge_type: EdgeType::ContributesTo,
            from: NodeRef::new(NodeType::Task, task.id),
            to: NodeRef::new(NodeType::Objective, objective.id),
            attrs: serde_json::json!({"weight": 0.5}),
        },
    )
    .unwrap();
    // A blocker belongs to this instance only.
    let blocker = task_with(&mut conn, |t| {
        t.title = "Collect numbers".into();
        t.recurrence = None;
    });
    edges::add(
        &mut conn,
        NewEdge {
            edge_type: EdgeType::Blocks,
            from: NodeRef::new(NodeType::Task, blocker.id),
            to: NodeRef::new(NodeType::Task, task.id),
            attrs: serde_json::json!({}),
        },
    )
    .unwrap();

    finish(&mut conn, task.id);

    let all = all_tasks(&conn);
    assert_eq!(all.len(), 3, "the finished one, its blocker and the next");
    let done = all.iter().find(|t| t.id == task.id).unwrap();
    assert_eq!(done.status, TaskStatus::Done);
    assert_eq!(done.recurrence, None, "the rule moves to the next one");
    let next = all
        .iter()
        .find(|t| t.title == "Board update" && t.id != task.id)
        .unwrap();
    assert_eq!(next.status, TaskStatus::Todo);
    assert_eq!(next.recurrence, Some(weekly_monday()));
    assert_eq!(
        (
            next.description.as_str(),
            next.estimate_days,
            next.priority,
            next.project_id
        ),
        ("Numbers and risks", Some(1.5), 2, Some(project.id))
    );
    // Due the Monday after the one that was due (today is the real today, long after 2027-03-01,
    // so this is the first Monday that is not in the past).
    assert!(next.due_date.unwrap() >= today());
    assert_eq!(
        next.due_date.unwrap().weekday().number_days_from_monday(),
        0
    );
    // Start and due keep their gap.
    let gap = (done.due_date.unwrap() - done.start_date.unwrap()).whole_days();
    assert_eq!(
        (next.due_date.unwrap() - next.start_date.unwrap()).whole_days(),
        gap
    );
    // Assignee and objective come along (with the weight); the blocker does not.
    let links = edges::links_for_node(&conn, next.id).unwrap();
    let kinds: Vec<EdgeType> = links.iter().map(|l| l.edge.edge_type).collect();
    assert!(kinds.contains(&EdgeType::AssignedTo) && kinds.contains(&EdgeType::ContributesTo));
    assert!(!kinds.contains(&EdgeType::Blocks));
    let weight = links
        .iter()
        .find(|l| l.edge.edge_type == EdgeType::ContributesTo)
        .unwrap();
    assert_eq!(weight.edge.attrs["weight"], 0.5);
    assert_eq!(
        links
            .iter()
            .find(|l| l.edge.edge_type == EdgeType::AssignedTo)
            .unwrap()
            .other
            .node
            .id,
        pm.id
    );
}

#[test]
fn the_series_goes_on_and_follows_the_rule_not_the_calendar_of_completion() {
    let mut conn = db();
    // Due far in the future so the date is predictable: Monday 2099-01-05.
    let first = task_with(&mut conn, |t| t.due_date = Some(date!(2099 - 01 - 05)));
    finish(&mut conn, first.id);
    let second = all_tasks(&conn)
        .into_iter()
        .find(|t| t.id != first.id)
        .unwrap();
    assert_eq!(second.due_date, Some(date!(2099 - 01 - 12)));
    finish(&mut conn, second.id);
    let third = all_tasks(&conn)
        .into_iter()
        .find(|t| t.status == TaskStatus::Todo)
        .unwrap();
    assert_eq!(third.due_date, Some(date!(2099 - 01 - 19)));
    assert_eq!(all_tasks(&conn).len(), 3);
    // Every month on the 31st.
    let monthly = task_with(&mut conn, |t| {
        t.title = "Close the books".into();
        t.due_date = Some(date!(2099 - 01 - 31));
        t.recurrence = Some(Cadence::Monthly { day: 31 }.into());
    });
    finish(&mut conn, monthly.id);
    let next = all_tasks(&conn)
        .into_iter()
        .find(|t| t.title == "Close the books" && t.status == TaskStatus::Todo)
        .unwrap();
    assert_eq!(next.due_date, Some(date!(2099 - 02 - 28)));
}

#[test]
fn a_task_without_a_due_date_is_counted_from_today() {
    let mut conn = db();
    let t = task_with(&mut conn, |t| t.due_date = None);
    finish(&mut conn, t.id);
    let next = all_tasks(&conn).into_iter().find(|n| n.id != t.id).unwrap();
    let due = next.due_date.unwrap();
    assert!(due > today() && (due - today()).whole_days() <= 7);
    assert_eq!(due.weekday().number_days_from_monday(), 0);
}

#[test]
fn only_finishing_makes_the_next_one() {
    let mut conn = db();
    // A task that does not repeat.
    let plain = task_with(&mut conn, |t| t.recurrence = None);
    finish(&mut conn, plain.id);
    assert_eq!(all_tasks(&conn).len(), 1);

    // Cancelling a repeating one ends the series; so does starting it or editing it.
    let t = task_with(&mut conn, |t| t.title = "Repeats".into());
    for status in [
        TaskStatus::InProgress,
        TaskStatus::Blocked,
        TaskStatus::Cancelled,
    ] {
        tasks::update(
            &mut conn,
            t.id,
            UpdateTask {
                status: Some(status),
                priority: Some(1),
                ..Default::default()
            },
        )
        .unwrap();
    }
    assert_eq!(
        all_tasks(&conn)
            .iter()
            .filter(|x| x.title == "Repeats")
            .count(),
        1
    );

    // Already finished when made: nothing to hand on.
    task_with(&mut conn, |t| {
        t.title = "Already done".into();
        t.status = Some(TaskStatus::Done);
    });
    assert_eq!(
        all_tasks(&conn)
            .iter()
            .filter(|x| x.title == "Already done")
            .count(),
        1
    );

    // Finishing it again after reopening does not make a second next one: the rule went with
    // the first.
    let r = task_with(&mut conn, |t| t.title = "Reopened".into());
    finish(&mut conn, r.id);
    tasks::update(
        &mut conn,
        r.id,
        UpdateTask {
            status: Some(TaskStatus::Todo),
            ..Default::default()
        },
    )
    .unwrap();
    finish(&mut conn, r.id);
    assert_eq!(
        all_tasks(&conn)
            .iter()
            .filter(|x| x.title == "Reopened")
            .count(),
        2
    );
}

#[test]
fn a_project_archived_since_does_not_receive_the_next_task() {
    let mut conn = db();
    let project = projects::create(
        &mut conn,
        CreateProject {
            title: "Old".into(),
            slug: None,
            description: String::new(),
            owner_person_id: None,
            start_date: None,
            target_date: None,
            status: None,
            priority: None,
        },
    )
    .unwrap();
    let t = task_with(&mut conn, |t| t.project_id = Some(project.id));
    // Archive the project but keep the task where it is.
    nodes::archive(&mut conn, NodeRef::new(NodeType::Project, project.id)).unwrap();
    finish(&mut conn, t.id);
    let next = all_tasks(&conn).into_iter().find(|n| n.id != t.id).unwrap();
    assert_eq!(next.project_id, None, "it lands in the inbox");
}

#[test]
fn a_bad_rule_is_refused_and_writes_nothing() {
    let mut conn = db();
    let t = task_with(&mut conn, |t| t.recurrence = None);
    let before = activity::count(&conn).unwrap();
    for bad in [
        Cadence::Weekly {
            every: 0,
            weekday: 0,
        },
        Cadence::Weekly {
            every: 1,
            weekday: 9,
        },
        Cadence::Monthly { day: 40 },
    ] {
        let err = tasks::update(
            &mut conn,
            t.id,
            UpdateTask {
                recurrence: Patch::Set(bad.into()),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(matches!(err, StoreError::Invalid(_)), "{bad:?}");
    }
    assert_eq!(activity::count(&conn).unwrap(), before);
    assert!(tasks::get(&conn, t.id).unwrap().recurrence.is_none());
    // A good one is kept, shows in the activity, and can be cleared.
    let ok = tasks::update(
        &mut conn,
        t.id,
        UpdateTask {
            recurrence: Patch::Set(weekly_monday()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(ok.recurrence, Some(weekly_monday()));
    assert_eq!(
        tasks::get(&conn, t.id).unwrap().recurrence,
        Some(weekly_monday())
    );
    let cleared = tasks::update(
        &mut conn,
        t.id,
        UpdateTask {
            recurrence: Patch::Clear,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(cleared.recurrence, None);
}

#[test]
fn a_damaged_stored_rule_does_not_stop_the_task_loading() {
    let mut conn = db();
    let t = task_with(&mut conn, |_| {});
    conn.execute(
        "UPDATE tasks SET recurrence = '{\"cadence\":{\"kind\":\"yearly\"}}' WHERE id = ?1",
        [t.id.to_string()],
    )
    .unwrap();
    assert_eq!(tasks::get(&conn, t.id).unwrap().recurrence, None);
}

fn note_with(conn: &mut Connection, f: impl FnOnce(&mut CreateNote)) -> Note {
    let mut input = CreateNote {
        title: "1:1 with Priya".into(),
        body: String::new(),
        note_date: Some(date!(2027 - 03 - 01)),
        kind: Some(NoteKind::OneOnOne),
        recurrence: Some(Recurrence {
            cadence: Cadence::Weekly {
                every: 1,
                weekday: 0,
            },
            template: Some("## Agenda\n\n- ".into()),
        }),
    };
    f(&mut input);
    notes::create(conn, input).unwrap()
}

fn all_notes(conn: &Connection) -> Vec<Note> {
    let mut list = notes::list(conn, false).unwrap();
    list.sort_by_key(|n| n.note_date);
    list
}

#[test]
fn a_repeating_note_is_made_on_its_date_with_the_template_and_open_items() {
    let mut conn = db();
    let priya = person(&mut conn, "Priya");
    let head = note_with(&mut conn, |n| {
        n.body = format!(
            "With {}\n\n- [x] Done thing\n- [ ] Send the plan\n",
            mention_token("Priya", priya.id)
        );
    });
    // Before the next Monday: nothing.
    assert_eq!(
        notes::generate_due(&mut conn, date!(2027 - 03 - 03)).unwrap(),
        0
    );
    assert_eq!(
        notes::generate_due(&mut conn, date!(2027 - 03 - 07)).unwrap(),
        0
    );
    assert_eq!(all_notes(&conn).len(), 1);

    // On the day: one new note.
    assert_eq!(
        notes::generate_due(&mut conn, date!(2027 - 03 - 08)).unwrap(),
        1
    );
    let list = all_notes(&conn);
    assert_eq!(list.len(), 2);
    let new = &list[1];
    assert_eq!(new.note_date, date!(2027 - 03 - 08));
    assert_eq!(
        (new.title.as_str(), new.kind),
        ("1:1 with Priya", NoteKind::OneOnOne)
    );
    assert_eq!(
        new.body,
        "## Agenda\n\n-\n\n## Carried over\n\n- [ ] Send the plan\n"
    );
    // The rule moved to the new note; the old one is plain history.
    assert_eq!(
        new.recurrence.as_ref().map(|r| r.cadence),
        Some(Cadence::Weekly {
            every: 1,
            weekday: 0
        })
    );
    assert_eq!(notes::get(&conn, head.id).unwrap().recurrence, None);

    // Calling again, or on a later day of the same week, makes nothing more.
    assert_eq!(
        notes::generate_due(&mut conn, date!(2027 - 03 - 08)).unwrap(),
        0
    );
    assert_eq!(
        notes::generate_due(&mut conn, date!(2027 - 03 - 12)).unwrap(),
        0
    );
    assert_eq!(all_notes(&conn).len(), 2);
}

#[test]
fn mentions_in_the_template_become_links_of_the_new_note() {
    let mut conn = db();
    let priya = person(&mut conn, "Priya");
    note_with(&mut conn, |n| {
        n.recurrence = Some(Recurrence {
            cadence: Cadence::Weekly {
                every: 1,
                weekday: 0,
            },
            template: Some(format!("With {}", mention_token("Priya", priya.id))),
        });
    });
    notes::generate_due(&mut conn, date!(2027 - 03 - 08)).unwrap();
    let new = all_notes(&conn).remove(1);
    let links = edges::links_for_node(&conn, new.id).unwrap();
    assert!(
        links
            .iter()
            .any(|l| l.edge.edge_type == EdgeType::Mentions && l.other.node.id == priya.id),
        "the new 1:1 is linked to Priya"
    );
}

#[test]
fn being_away_makes_one_note_for_the_latest_date_not_a_pile() {
    let mut conn = db();
    note_with(&mut conn, |_| {});
    assert_eq!(
        notes::generate_due(&mut conn, date!(2027 - 04 - 14)).unwrap(),
        1
    );
    let list = all_notes(&conn);
    assert_eq!(list.len(), 2);
    assert_eq!(list[1].note_date, date!(2027 - 04 - 12));
}

#[test]
fn archived_and_plain_notes_are_left_alone() {
    let mut conn = db();
    let archived = note_with(&mut conn, |_| {});
    nodes::archive(&mut conn, NodeRef::new(NodeType::Note, archived.id)).unwrap();
    note_with(&mut conn, |n| {
        n.title = "Plain".into();
        n.recurrence = None;
    });
    assert_eq!(
        notes::generate_due(&mut conn, date!(2027 - 04 - 14)).unwrap(),
        0
    );
}

fn quick(conn: &mut Connection, text: &str) -> QuickResult {
    let directory = quick_add::directory(conn).unwrap();
    let out = minimap_core::quick_add::plan(
        text,
        &minimap_core::quick_add::Context {
            today: date!(2027 - 03 - 03),
            hours_per_day: 8.0,
            directory: &directory,
            choices: &[],
        },
    );
    assert!(
        out.preview.problems.is_empty(),
        "{:?}",
        out.preview.problems
    );
    quick_add::commit(conn, out.plan.unwrap()).unwrap()
}

#[test]
fn quick_add_every_makes_repeating_tasks_and_notes() {
    let mut conn = db();
    person(&mut conn, "Priya");
    let made = quick(&mut conn, "task Board update every:mon");
    let task = tasks::get(&conn, made.node.node.id).unwrap();
    assert_eq!(task.due_date, Some(date!(2027 - 03 - 08)));
    assert_eq!(task.recurrence, Some(weekly_monday()));

    // A repeating 1:1 keeps its person in the template, so every new one mentions them.
    let made = quick(&mut conn, "note 1:1 @priya every:mon");
    let note = notes::get(&conn, made.node.node.id).unwrap();
    assert_eq!(note.note_date, date!(2027 - 03 - 08));
    let rule = note.recurrence.unwrap();
    assert!(rule.template.as_deref().unwrap().contains("@[Priya](node:"));
    notes::generate_due(&mut conn, date!(2027 - 03 - 15)).unwrap();
    let next = all_notes(&conn).remove(1);
    assert_eq!(next.note_date, date!(2027 - 03 - 15));
    assert!(next.body.contains("@[Priya](node:"));
}
