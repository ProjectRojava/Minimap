use minimap_store::*;
use minimap_types::*;
use serde_json::json;
use uuid::Uuid;

fn db() -> Connection {
    open_in_memory().unwrap()
}

fn history(conn: &Connection, id: Uuid) -> Vec<Activity> {
    activity::list_for_node(conn, id).unwrap()
}

fn actions(conn: &Connection, id: Uuid) -> Vec<ActivityAction> {
    let mut a: Vec<_> = history(conn, id).into_iter().map(|a| a.action).collect();
    a.reverse(); // oldest first
    a
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

fn task(conn: &mut Connection, title: &str) -> Task {
    tasks::create(
        conn,
        CreateTask {
            title: title.into(),
            description: String::new(),
            project_id: None,
            status: None,
            estimate_days: None,
            start_date: None,
            due_date: None,
            priority: None,
        },
    )
    .unwrap()
}

fn blocks(from: &Task, to: &Task) -> NewEdge {
    NewEdge {
        edge_type: EdgeType::Blocks,
        from: NodeRef::new(NodeType::Task, from.id),
        to: NodeRef::new(NodeType::Task, to.id),
        attrs: json!({ "lag_days": 0 }),
    }
}

/// One of each node type, created through the public API.
fn one_of_each(conn: &mut Connection) -> Vec<NodeRef> {
    let p = person(conn, "Priya");
    let o = objectives::create(
        conn,
        CreateObjective {
            title: "Launch EU".into(),
            description: String::new(),
            target_date: None,
            status: None,
            priority: None,
        },
    )
    .unwrap();
    let pr = projects::create(
        conn,
        CreateProject {
            title: "Q1 EU region".into(),
            description: String::new(),
            owner_person_id: None,
            start_date: None,
            target_date: None,
            status: None,
            priority: None,
        },
    )
    .unwrap();
    let t = task(conn, "Fix login");
    let team = teams::create(
        conn,
        CreateTeam {
            name: "Platform".into(),
            description: String::new(),
            parent_team_id: None,
        },
    )
    .unwrap();
    let n = notes::create(
        conn,
        CreateNote {
            title: "1:1 Priya".into(),
            body: "hello".into(),
            note_date: None,
            kind: Some(NoteKind::OneOnOne),
        },
    )
    .unwrap();
    let d = decisions::create(
        conn,
        CreateDecision {
            title: "Postgres".into(),
            context: String::new(),
            decision: String::new(),
            rationale: String::new(),
            decided_on: None,
            status: None,
        },
    )
    .unwrap();
    let w = waiting_on::create(
        conn,
        CreateWaitingOn {
            description: "Security sign-off".into(),
            person_id: p.id,
            asked_on: None,
            expected_by: None,
        },
    )
    .unwrap();
    vec![
        NodeRef::new(NodeType::Person, p.id),
        NodeRef::new(NodeType::Objective, o.id),
        NodeRef::new(NodeType::Project, pr.id),
        NodeRef::new(NodeType::Task, t.id),
        NodeRef::new(NodeType::Team, team.id),
        NodeRef::new(NodeType::Note, n.id),
        NodeRef::new(NodeType::Decision, d.id),
        NodeRef::new(NodeType::WaitingOn, w.id),
    ]
}

#[test]
fn create_writes_one_created_activity_for_every_type() {
    let mut conn = db();
    let nodes = one_of_each(&mut conn);
    assert_eq!(nodes.len(), NodeType::ALL.len());
    for n in &nodes {
        assert_eq!(
            actions(&conn, n.id),
            vec![ActivityAction::Created],
            "{}",
            n.node_type
        );
    }
    assert_eq!(activity::count(&conn).unwrap(), nodes.len() as i64);
}

#[test]
fn archive_and_delete_every_type() {
    let mut conn = db();
    let mut nodes = one_of_each(&mut conn);
    // The waiting-on references the person by foreign key; delete it first.
    nodes.sort_by_key(|n| n.node_type != NodeType::WaitingOn);
    for n in nodes {
        // Not archived yet: delete refused.
        assert!(matches!(
            nodes::delete(&mut conn, n),
            Err(StoreError::NotArchived { .. })
        ));
        nodes::archive(&mut conn, n).unwrap();
        assert!(matches!(
            nodes::archive(&mut conn, n),
            Err(StoreError::AlreadyArchived { .. })
        ));
        nodes::delete(&mut conn, n).unwrap();
        assert_eq!(
            actions(&conn, n.id),
            vec![
                ActivityAction::Created,
                ActivityAction::Archived,
                ActivityAction::Deleted
            ],
            "history survives delete for {}",
            n.node_type
        );
    }
}

#[test]
fn delete_of_referenced_node_is_a_constraint_error() {
    let mut conn = db();
    let p = person(&mut conn, "Raj");
    waiting_on::create(
        &mut conn,
        CreateWaitingOn {
            description: "x".into(),
            person_id: p.id,
            asked_on: None,
            expected_by: None,
        },
    )
    .unwrap();
    let r = NodeRef::new(NodeType::Person, p.id);
    nodes::archive(&mut conn, r).unwrap();
    assert!(matches!(
        nodes::delete(&mut conn, r),
        Err(StoreError::Constraint(_))
    ));
    // The failed delete rolled back: the person is still there.
    assert!(people::get(&conn, p.id).is_ok());
}

#[test]
fn missing_node_is_not_found() {
    let mut conn = db();
    let r = NodeRef::new(NodeType::Task, Uuid::now_v7());
    assert!(matches!(
        tasks::get(&conn, r.id),
        Err(StoreError::NotFound { .. })
    ));
    assert!(matches!(
        nodes::archive(&mut conn, r),
        Err(StoreError::NotFound { .. })
    ));
}

#[test]
fn update_records_only_changed_fields() {
    let mut conn = db();
    let t = task(&mut conn, "Fix login");
    let due = timefmt::parse_date("2027-01-15").unwrap();
    let u = tasks::update(
        &mut conn,
        t.id,
        UpdateTask {
            title: Some("Fix login timeout".into()),
            due_date: Patch::Set(due),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(u.due_date, Some(due));
    assert!(u.updated_at >= t.updated_at);

    let latest = &history(&conn, t.id)[0];
    assert_eq!(latest.action, ActivityAction::Updated);
    assert_eq!(
        latest.diff,
        json!({
            "title": ["Fix login", "Fix login timeout"],
            "due_date": [null, "2027-01-15"],
        })
    );
    assert_eq!(tasks::get(&conn, t.id).unwrap(), u);
}

#[test]
fn noop_update_writes_no_activity() {
    let mut conn = db();
    let t = task(&mut conn, "Fix login");
    let before = activity::count(&conn).unwrap();
    let same = tasks::update(
        &mut conn,
        t.id,
        UpdateTask {
            title: Some("Fix login".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(same, t);
    assert_eq!(
        tasks::update(&mut conn, t.id, UpdateTask::default()).unwrap(),
        t
    );
    assert_eq!(activity::count(&conn).unwrap(), before);
}

#[test]
fn patch_can_clear_nullable_fields() {
    let mut conn = db();
    let t = task(&mut conn, "x");
    let d = timefmt::parse_date("2027-01-15").unwrap();
    tasks::update(
        &mut conn,
        t.id,
        UpdateTask {
            due_date: Patch::Set(d),
            ..Default::default()
        },
    )
    .unwrap();
    let cleared = tasks::update(
        &mut conn,
        t.id,
        UpdateTask {
            due_date: Patch::Clear,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(cleared.due_date, None);
}

#[test]
fn completed_at_follows_status() {
    let mut conn = db();
    let t = task(&mut conn, "x");
    assert!(t.completed_at.is_none());
    let done = tasks::update(
        &mut conn,
        t.id,
        UpdateTask {
            status: Some(TaskStatus::Done),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(done.completed_at.is_some());
    let reopened = tasks::update(
        &mut conn,
        t.id,
        UpdateTask {
            status: Some(TaskStatus::Todo),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(reopened.completed_at.is_none());
}

#[test]
fn update_every_type() {
    let mut conn = db();
    let nodes = one_of_each(&mut conn);
    let id = |t: NodeType| nodes.iter().find(|n| n.node_type == t).unwrap().id;

    objectives::update(
        &mut conn,
        id(NodeType::Objective),
        UpdateObjective {
            status: Some(ObjectiveStatus::AtRisk),
            priority: Some(1),
            ..Default::default()
        },
    )
    .unwrap();
    projects::update(
        &mut conn,
        id(NodeType::Project),
        UpdateProject {
            status: Some(ProjectStatus::Active),
            owner_person_id: Patch::Set(id(NodeType::Person)),
            ..Default::default()
        },
    )
    .unwrap();
    people::update(
        &mut conn,
        id(NodeType::Person),
        UpdatePerson {
            weekly_capacity_hours: Some(32.0),
            email: Patch::Set("p@x.io".into()),
            ..Default::default()
        },
    )
    .unwrap();
    teams::update(
        &mut conn,
        id(NodeType::Team),
        UpdateTeam {
            description: Some("core".into()),
            ..Default::default()
        },
    )
    .unwrap();
    notes::update(
        &mut conn,
        id(NodeType::Note),
        UpdateNote {
            body: Some("changed".into()),
            ..Default::default()
        },
    )
    .unwrap();
    decisions::update(
        &mut conn,
        id(NodeType::Decision),
        UpdateDecision {
            status: Some(DecisionStatus::Decided),
            ..Default::default()
        },
    )
    .unwrap();
    let resolved = timefmt::parse_date("2027-02-01").unwrap();
    waiting_on::update(
        &mut conn,
        id(NodeType::WaitingOn),
        UpdateWaitingOn {
            resolved_on: Patch::Set(resolved),
            ..Default::default()
        },
    )
    .unwrap();

    for n in &nodes {
        if n.node_type == NodeType::Task {
            continue;
        }
        assert_eq!(
            actions(&conn, n.id),
            vec![ActivityAction::Created, ActivityAction::Updated],
            "{}",
            n.node_type
        );
    }
    assert_eq!(
        projects::get(&conn, id(NodeType::Project))
            .unwrap()
            .owner_person_id,
        Some(id(NodeType::Person))
    );
    assert_eq!(
        waiting_on::get(&conn, id(NodeType::WaitingOn))
            .unwrap()
            .resolved_on,
        Some(resolved)
    );
}

#[test]
fn list_hides_archived_unless_asked() {
    let mut conn = db();
    let a = task(&mut conn, "a");
    let b = task(&mut conn, "b");
    nodes::archive(&mut conn, NodeRef::new(NodeType::Task, a.id)).unwrap();
    let active: Vec<_> = tasks::list(&conn, false)
        .unwrap()
        .into_iter()
        .map(|t| t.id)
        .collect();
    assert_eq!(active, vec![b.id]);
    assert_eq!(tasks::list(&conn, true).unwrap().len(), 2);
}

#[test]
fn invalid_input_is_rejected_and_writes_nothing() {
    let mut conn = db();
    let bad = |title: &str, priority| CreateTask {
        title: title.into(),
        description: String::new(),
        project_id: None,
        status: None,
        estimate_days: None,
        start_date: None,
        due_date: None,
        priority,
    };
    assert!(matches!(
        tasks::create(&mut conn, bad("  ", None)),
        Err(StoreError::Invalid(_))
    ));
    assert!(matches!(
        tasks::create(&mut conn, bad("x", Some(9))),
        Err(StoreError::Invalid(_))
    ));
    // Unknown project id violates the foreign key.
    let mut input = bad("x", None);
    input.project_id = Some(Uuid::now_v7());
    assert!(matches!(
        tasks::create(&mut conn, input),
        Err(StoreError::Constraint(_))
    ));
    assert_eq!(activity::count(&conn).unwrap(), 0);
}

#[test]
fn exactly_one_self_person() {
    let mut conn = db();
    assert!(people::get_self(&conn).unwrap().is_none());
    let me = people::create(
        &mut conn,
        CreatePerson {
            name: "Me".into(),
            role_title: String::new(),
            email: None,
            weekly_capacity_hours: None,
            is_self: true,
            notes: String::new(),
        },
    )
    .unwrap();
    assert_eq!(people::get_self(&conn).unwrap().unwrap().id, me.id);
    assert_eq!(me.weekly_capacity_hours, 40.0);
    let second = people::create(
        &mut conn,
        CreatePerson {
            name: "Other".into(),
            role_title: String::new(),
            email: None,
            weekly_capacity_hours: None,
            is_self: true,
            notes: String::new(),
        },
    );
    assert!(matches!(second, Err(StoreError::Invalid(_))));
    // The partial unique index backs this up at the SQL level.
    let raw = conn.execute(
        "INSERT INTO people (id, name, is_self, created_at, updated_at) VALUES ('x','X',1,'t','t')",
        [],
    );
    assert!(raw.is_err());
}

#[test]
fn add_and_remove_edge_write_activity() {
    let mut conn = db();
    let (a, b) = (task(&mut conn, "a"), task(&mut conn, "b"));
    let e = edges::add(&mut conn, blocks(&a, &b)).unwrap();
    assert_eq!(edges::get(&conn, e.id).unwrap(), e);
    assert_eq!(
        actions(&conn, a.id),
        vec![ActivityAction::Created, ActivityAction::EdgeAdded]
    );
    assert_eq!(edges::list_for_node(&conn, b.id, false).unwrap().len(), 1);

    edges::remove(&mut conn, e.id).unwrap();
    assert_eq!(
        actions(&conn, a.id).last(),
        Some(&ActivityAction::EdgeRemoved)
    );
    assert!(edges::list_for_node(&conn, b.id, false).unwrap().is_empty());
    assert_eq!(edges::list_for_node(&conn, b.id, true).unwrap().len(), 1);
    assert!(matches!(
        edges::remove(&mut conn, e.id),
        Err(StoreError::EdgeNotFound(_))
    ));
}

#[test]
fn duplicate_edge_rejected_but_archived_one_is_revived() {
    let mut conn = db();
    let (a, b) = (task(&mut conn, "a"), task(&mut conn, "b"));
    let e = edges::add(&mut conn, blocks(&a, &b)).unwrap();
    assert!(matches!(
        edges::add(&mut conn, blocks(&a, &b)),
        Err(StoreError::DuplicateEdge)
    ));
    edges::remove(&mut conn, e.id).unwrap();

    let mut again = blocks(&a, &b);
    again.attrs = json!({ "lag_days": 2 });
    let revived = edges::add(&mut conn, again).unwrap();
    assert_eq!(revived.id, e.id);
    assert_eq!(revived.attrs, json!({ "lag_days": 2 }));
    assert!(revived.archived_at.is_none());
    assert_eq!(
        edges::get(&conn, e.id).unwrap().attrs,
        json!({ "lag_days": 2 })
    );
}

#[test]
fn edge_endpoints_must_exist_and_be_active() {
    let mut conn = db();
    let a = task(&mut conn, "a");
    let ghost = NewEdge {
        edge_type: EdgeType::Blocks,
        from: NodeRef::new(NodeType::Task, a.id),
        to: NodeRef::new(NodeType::Task, Uuid::now_v7()),
        attrs: json!({}),
    };
    assert!(matches!(
        edges::add(&mut conn, ghost),
        Err(StoreError::NotFound { .. })
    ));

    let b = task(&mut conn, "b");
    nodes::archive(&mut conn, NodeRef::new(NodeType::Task, b.id)).unwrap();
    assert!(matches!(
        edges::add(&mut conn, blocks(&a, &b)),
        Err(StoreError::Invalid(_))
    ));
}

#[test]
fn archiving_a_node_archives_its_edges_and_unarchive_restores_them() {
    let mut conn = db();
    let (a, b, c) = (
        task(&mut conn, "a"),
        task(&mut conn, "b"),
        task(&mut conn, "c"),
    );
    let ab = edges::add(&mut conn, blocks(&a, &b)).unwrap();
    let bc = edges::add(&mut conn, blocks(&b, &c)).unwrap();
    // An edge removed before the archive must stay removed after the restore.
    let ac = edges::add(&mut conn, blocks(&a, &c)).unwrap();
    edges::remove(&mut conn, ac.id).unwrap();

    let before = activity::count(&conn).unwrap();
    let b_ref = NodeRef::new(NodeType::Task, b.id);
    nodes::archive(&mut conn, b_ref).unwrap();
    // One row for the node + one per archived edge.
    assert_eq!(activity::count(&conn).unwrap(), before + 1 + 2);
    assert!(edges::get(&conn, ab.id).unwrap().archived_at.is_some());
    assert!(edges::get(&conn, bc.id).unwrap().archived_at.is_some());
    assert!(edges::list_active(&conn).unwrap().is_empty());

    nodes::unarchive(&mut conn, b_ref).unwrap();
    let active: Vec<_> = edges::list_active(&conn)
        .unwrap()
        .into_iter()
        .map(|e| e.id)
        .collect();
    assert_eq!(active.len(), 2);
    assert!(active.contains(&ab.id) && active.contains(&bc.id));
    assert!(edges::get(&conn, ac.id).unwrap().archived_at.is_some());
    assert!(tasks::get(&conn, b.id).unwrap().archived_at.is_none());
    assert!(actions(&conn, b.id).contains(&ActivityAction::Unarchived));
    assert!(matches!(
        nodes::unarchive(&mut conn, b_ref),
        Err(StoreError::NotArchivedYet { .. })
    ));
}

#[test]
fn unarchive_leaves_edge_archived_while_other_end_is_archived() {
    let mut conn = db();
    let (a, b) = (task(&mut conn, "a"), task(&mut conn, "b"));
    let e = edges::add(&mut conn, blocks(&a, &b)).unwrap();
    let (ra, rb) = (
        NodeRef::new(NodeType::Task, a.id),
        NodeRef::new(NodeType::Task, b.id),
    );
    nodes::archive(&mut conn, ra).unwrap(); // archives the edge at t1
    nodes::archive(&mut conn, rb).unwrap(); // edge already archived: untouched
    nodes::unarchive(&mut conn, ra).unwrap(); // b still archived -> edge stays archived
    assert!(edges::get(&conn, e.id).unwrap().archived_at.is_some());
}

#[test]
fn hard_delete_removes_edges_but_keeps_activity() {
    let mut conn = db();
    let (a, b) = (task(&mut conn, "a"), task(&mut conn, "b"));
    let e = edges::add(&mut conn, blocks(&a, &b)).unwrap();
    let ra = NodeRef::new(NodeType::Task, a.id);
    nodes::archive(&mut conn, ra).unwrap();
    nodes::delete(&mut conn, ra).unwrap();
    assert!(matches!(
        edges::get(&conn, e.id),
        Err(StoreError::EdgeNotFound(_))
    ));
    assert!(matches!(
        tasks::get(&conn, a.id),
        Err(StoreError::NotFound { .. })
    ));
    assert!(!history(&conn, a.id).is_empty());
}

mod props {
    use super::*;
    use proptest::prelude::*;

    #[derive(Debug, Clone)]
    enum Op {
        Create,
        Update(usize, u8),
        Archive(usize),
        Unarchive(usize),
        Link(usize, usize),
    }

    fn op() -> impl Strategy<Value = Op> {
        prop_oneof![
            Just(Op::Create),
            (0..8usize, 1..=5u8).prop_map(|(i, p)| Op::Update(i, p)),
            (0..8usize).prop_map(Op::Archive),
            (0..8usize).prop_map(Op::Unarchive),
            (0..8usize, 0..8usize).prop_map(|(a, b)| Op::Link(a, b)),
        ]
    }

    proptest! {
        /// Activity rows == rows we expect from the accepted writes, whatever the sequence.
        #[test]
        fn activity_count_matches_writes(ops in proptest::collection::vec(op(), 1..40)) {
            let mut conn = open_in_memory().unwrap();
            let mut tasks_made: Vec<Uuid> = Vec::new();
            let mut expected: i64 = 0;

            for op in ops {
                match op {
                    Op::Create => {
                        let t = task(&mut conn, "t");
                        tasks_made.push(t.id);
                        expected += 1;
                    }
                    Op::Update(i, p) if !tasks_made.is_empty() => {
                        let id = tasks_made[i % tasks_made.len()];
                        let before = tasks::get(&conn, id).unwrap();
                        tasks::update(&mut conn, id, UpdateTask { priority: Some(p), ..Default::default() }).unwrap();
                        if before.priority != p { expected += 1; }
                    }
                    Op::Archive(i) if !tasks_made.is_empty() => {
                        let id = tasks_made[i % tasks_made.len()];
                        let active_edges = edges::list_for_node(&conn, id, false).unwrap().len() as i64;
                        if nodes::archive(&mut conn, NodeRef::new(NodeType::Task, id)).is_ok() {
                            expected += 1 + active_edges;
                        }
                    }
                    Op::Unarchive(i) if !tasks_made.is_empty() => {
                        let id = tasks_made[i % tasks_made.len()];
                        let before: Vec<_> = edges::list_active(&conn).unwrap().into_iter().map(|e| e.id).collect();
                        if nodes::unarchive(&mut conn, NodeRef::new(NodeType::Task, id)).is_ok() {
                            let after = edges::list_active(&conn).unwrap();
                            let restored = after.iter().filter(|e| !before.contains(&e.id)).count() as i64;
                            expected += 1 + restored;
                        }
                    }
                    Op::Link(a, b) if tasks_made.len() >= 2 => {
                        let (a, b) = (tasks_made[a % tasks_made.len()], tasks_made[b % tasks_made.len()]);
                        let new = NewEdge {
                            edge_type: EdgeType::RelatesTo,
                            from: NodeRef::new(NodeType::Task, a),
                            to: NodeRef::new(NodeType::Task, b),
                            attrs: json!({}),
                        };
                        if edges::add(&mut conn, new).is_ok() { expected += 1; }
                    }
                    _ => {}
                }
                prop_assert_eq!(activity::count(&conn).unwrap(), expected);
            }
        }
    }
}

#[test]
fn summary_and_links_name_the_other_end() {
    let mut conn = db();
    let (a, b) = (task(&mut conn, "Write spec"), task(&mut conn, "Ship it"));
    edges::add(&mut conn, blocks(&a, &b)).unwrap();
    let s = nodes::summary(&conn, NodeRef::new(NodeType::Task, a.id)).unwrap();
    assert_eq!((s.label.as_str(), s.archived), ("Write spec", false));

    let links = edges::links_for_node(&conn, b.id).unwrap();
    assert_eq!(links.len(), 1);
    assert!(!links[0].outgoing);
    assert_eq!(links[0].other.label, "Write spec");

    // Person/team/waiting-on use their own label column.
    let p = person(&mut conn, "Priya");
    assert_eq!(
        nodes::summary(&conn, NodeRef::new(NodeType::Person, p.id))
            .unwrap()
            .label,
        "Priya"
    );
    nodes::archive(&mut conn, NodeRef::new(NodeType::Task, a.id)).unwrap();
    assert!(
        nodes::summary(&conn, NodeRef::new(NodeType::Task, a.id))
            .unwrap()
            .archived
    );
}
