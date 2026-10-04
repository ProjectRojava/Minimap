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
            follow_up_on: None,
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
            follow_up_on: None,
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
        assignee: AssigneeChoice::Nobody,
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

#[test]
fn self_person_cannot_be_archived_or_deleted() {
    let mut conn = db();
    let me = people::ensure_self(&mut conn, "  ").unwrap();
    assert_eq!(me.name, "Me");
    assert!(me.is_self);
    // Idempotent: a second call returns the same person.
    assert_eq!(
        people::ensure_self(&mut conn, "Someone else").unwrap().id,
        me.id
    );

    let r = NodeRef::new(NodeType::Person, me.id);
    assert!(matches!(
        nodes::archive(&mut conn, r),
        Err(StoreError::Invalid(_))
    ));
    assert!(matches!(
        nodes::delete(&mut conn, r),
        Err(StoreError::Invalid(_))
    ));
    assert!(people::get(&conn, me.id).unwrap().archived_at.is_none());
}

fn team(conn: &mut Connection, name: &str, parent: Option<Uuid>) -> Team {
    teams::create(
        conn,
        CreateTeam {
            name: name.into(),
            description: String::new(),
            parent_team_id: parent,
        },
    )
    .unwrap()
}

fn edge(from: NodeRef, to: NodeRef, edge_type: EdgeType, attrs: serde_json::Value) -> NewEdge {
    NewEdge {
        edge_type,
        from,
        to,
        attrs,
    }
}

#[test]
fn people_rows_show_teams_and_workload() {
    let mut conn = db();
    let (priya, raj) = (person(&mut conn, "priya"), person(&mut conn, "Raj"));
    let platform = team(&mut conn, "Platform", None);
    let (rp, rt) = (
        NodeRef::new(NodeType::Person, priya.id),
        NodeRef::new(NodeType::Team, platform.id),
    );
    edges::add(
        &mut conn,
        edge(rp, rt, EdgeType::MemberOf, json!({"role": "lead"})),
    )
    .unwrap();

    let (t1, t2, t3) = (
        task(&mut conn, "a"),
        task(&mut conn, "b"),
        task(&mut conn, "c"),
    );
    for t in [&t1, &t2, &t3] {
        edges::add(
            &mut conn,
            edge(
                NodeRef::new(NodeType::Task, t.id),
                rp,
                EdgeType::AssignedTo,
                json!({}),
            ),
        )
        .unwrap();
    }
    // Done tasks and archived tasks don't count as active.
    tasks::update(
        &mut conn,
        t2.id,
        UpdateTask {
            status: Some(TaskStatus::Done),
            ..Default::default()
        },
    )
    .unwrap();
    waiting_on::create(
        &mut conn,
        CreateWaitingOn {
            description: "x".into(),
            person_id: raj.id,
            asked_on: None,
            expected_by: None,
            follow_up_on: None,
        },
    )
    .unwrap();
    let resolved = waiting_on::create(
        &mut conn,
        CreateWaitingOn {
            description: "y".into(),
            person_id: raj.id,
            asked_on: None,
            expected_by: None,
            follow_up_on: None,
        },
    )
    .unwrap();
    waiting_on::update(
        &mut conn,
        resolved.id,
        UpdateWaitingOn {
            resolved_on: Patch::Set(timefmt::parse_date("2027-01-01").unwrap()),
            ..Default::default()
        },
    )
    .unwrap();

    let rows = views::people_rows(&conn).unwrap();
    assert_eq!(
        rows.iter()
            .map(|r| r.person.name.as_str())
            .collect::<Vec<_>>(),
        vec!["priya", "Raj"]
    ); // case-insensitive order
    let p = &rows[0];
    assert_eq!(p.active_task_count, 2);
    assert_eq!(
        p.teams.iter().map(|t| t.label.as_str()).collect::<Vec<_>>(),
        vec!["Platform"]
    );
    assert_eq!(rows[1].open_waiting_on_count, 1);
    assert_eq!(rows[1].active_task_count, 0);

    // Archiving the team removes it from the person's row.
    nodes::archive(&mut conn, rt).unwrap();
    assert!(views::people_rows(&conn).unwrap()[0].teams.is_empty());
}

#[test]
fn person_detail_has_teams_manager_reports_and_waiting_ons() {
    let mut conn = db();
    let (me, boss, report) = (
        person(&mut conn, "Me"),
        person(&mut conn, "Boss"),
        person(&mut conn, "Report"),
    );
    let t = team(&mut conn, "Platform", None);
    let r = |p: &Person| NodeRef::new(NodeType::Person, p.id);
    edges::add(
        &mut conn,
        edge(
            r(&me),
            NodeRef::new(NodeType::Team, t.id),
            EdgeType::MemberOf,
            json!({}),
        ),
    )
    .unwrap();
    edges::add(
        &mut conn,
        edge(r(&me), r(&boss), EdgeType::ReportsTo, json!({})),
    )
    .unwrap();
    edges::add(
        &mut conn,
        edge(r(&report), r(&me), EdgeType::ReportsTo, json!({})),
    )
    .unwrap();
    waiting_on::create(
        &mut conn,
        CreateWaitingOn {
            description: "sign-off".into(),
            person_id: me.id,
            asked_on: None,
            expected_by: None,
            follow_up_on: None,
        },
    )
    .unwrap();

    let d = views::person_detail(&conn, me.id).unwrap();
    assert_eq!(d.memberships.len(), 1);
    assert_eq!(d.memberships[0].role, "member"); // default
    assert_eq!(d.manager.as_ref().unwrap().node.label, "Boss");
    assert_eq!(
        d.reports
            .iter()
            .map(|n| n.label.as_str())
            .collect::<Vec<_>>(),
        vec!["Report"]
    );
    assert_eq!(d.waiting_ons.len(), 1);
}

#[test]
fn archive_preview_lists_active_assigned_tasks() {
    let mut conn = db();
    let p = person(&mut conn, "Priya");
    let (open, done) = (task(&mut conn, "open one"), task(&mut conn, "finished"));
    let rp = NodeRef::new(NodeType::Person, p.id);
    for t in [&open, &done] {
        edges::add(
            &mut conn,
            edge(
                NodeRef::new(NodeType::Task, t.id),
                rp,
                EdgeType::AssignedTo,
                json!({}),
            ),
        )
        .unwrap();
    }
    tasks::update(
        &mut conn,
        done.id,
        UpdateTask {
            status: Some(TaskStatus::Done),
            ..Default::default()
        },
    )
    .unwrap();
    let preview = views::person_archive_preview(&conn, p.id).unwrap();
    assert_eq!(preview.assigned_tasks.len(), 1);
    assert_eq!(preview.assigned_tasks[0].label, "open one");

    // Archiving really does archive the assignment edges.
    nodes::archive(&mut conn, rp).unwrap();
    assert!(edges::list_active(&conn).unwrap().is_empty());
}

#[test]
fn team_rows_are_a_tree_and_detail_lists_members() {
    let mut conn = db();
    let eng = team(&mut conn, "Engineering", None);
    let _sales = team(&mut conn, "Sales", None);
    let platform = team(&mut conn, "Platform", Some(eng.id));
    let infra = team(&mut conn, "Infra", Some(platform.id));
    let apps = team(&mut conn, "apps", Some(eng.id));

    let rows = views::team_rows(&conn).unwrap();
    let shape: Vec<(&str, u32)> = rows
        .iter()
        .map(|r| (r.team.name.as_str(), r.depth))
        .collect();
    assert_eq!(
        shape,
        vec![
            ("Engineering", 0),
            ("apps", 1),
            ("Platform", 1),
            ("Infra", 2),
            ("Sales", 0)
        ]
    );

    let p = person(&mut conn, "Priya");
    edges::add(
        &mut conn,
        edge(
            NodeRef::new(NodeType::Person, p.id),
            NodeRef::new(NodeType::Team, infra.id),
            EdgeType::MemberOf,
            json!({"role": "lead"}),
        ),
    )
    .unwrap();
    let d = views::team_detail(&conn, infra.id).unwrap();
    assert_eq!(d.parent.as_ref().unwrap().label, "Platform");
    assert_eq!((d.members.len(), d.members[0].role.as_str()), (1, "lead"));
    let rows = views::team_rows(&conn).unwrap();
    assert_eq!(
        rows.iter()
            .find(|r| r.team.id == infra.id)
            .unwrap()
            .member_count,
        1
    );
    let eng_detail = views::team_detail(&conn, eng.id).unwrap();
    assert_eq!(
        eng_detail
            .children
            .iter()
            .map(|c| c.label.as_str())
            .collect::<Vec<_>>(),
        vec!["apps", "Platform"]
    );
    let _ = apps;

    // Archiving a parent promotes its active children to the top level in the tree.
    nodes::archive(&mut conn, NodeRef::new(NodeType::Team, eng.id)).unwrap();
    let shape: Vec<(String, u32)> = views::team_rows(&conn)
        .unwrap()
        .into_iter()
        .map(|r| (r.team.name, r.depth))
        .collect();
    assert!(shape.contains(&("Platform".to_string(), 0)));
}

fn objective(conn: &mut Connection, title: &str, target: Option<&str>) -> Objective {
    objectives::create(
        conn,
        CreateObjective {
            title: title.into(),
            description: String::new(),
            target_date: target.map(|d| timefmt::parse_date(d).unwrap()),
            status: None,
            priority: None,
        },
    )
    .unwrap()
}

fn project(conn: &mut Connection, title: &str) -> Project {
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

fn contributes(from: NodeRef, to: &Objective, attrs: serde_json::Value) -> NewEdge {
    edge(
        from,
        NodeRef::new(NodeType::Objective, to.id),
        EdgeType::ContributesTo,
        attrs,
    )
}

#[test]
fn objective_rows_count_contributors() {
    let mut conn = db();
    let (o1, o2) = (
        objective(&mut conn, "Launch EU", Some("2027-03-31")),
        objective(&mut conn, "Cut churn", None),
    );
    let (p1, p2) = (
        project(&mut conn, "EU region"),
        project(&mut conn, "Billing"),
    );
    let t = task(&mut conn, "Ship it");
    for (n, o) in [
        (NodeRef::new(NodeType::Project, p1.id), &o1),
        (NodeRef::new(NodeType::Task, t.id), &o1),
        (NodeRef::new(NodeType::Project, p2.id), &o2),
    ] {
        edges::add(&mut conn, contributes(n, o, json!({"weight": 0.5}))).unwrap();
    }
    let rows = views::objective_rows(&conn).unwrap();
    let count = |id| {
        rows.iter()
            .find(|r| r.objective.id == id)
            .unwrap()
            .contribution_count
    };
    assert_eq!((count(o1.id), count(o2.id)), (2, 1));

    // An archived contributor stops counting.
    nodes::archive(&mut conn, NodeRef::new(NodeType::Task, t.id)).unwrap();
    let rows = views::objective_rows(&conn).unwrap();
    assert_eq!(
        rows.iter()
            .find(|r| r.objective.id == o1.id)
            .unwrap()
            .contribution_count,
        1
    );
    // Archived objectives are not listed.
    nodes::archive(&mut conn, NodeRef::new(NodeType::Objective, o2.id)).unwrap();
    assert_eq!(views::objective_rows(&conn).unwrap().len(), 1);
}

#[test]
fn objective_detail_lists_contributors_with_status_and_weight() {
    let mut conn = db();
    let o = objective(&mut conn, "Launch EU", None);
    let p = project(&mut conn, "EU region");
    let t = task(&mut conn, "alpha task");
    let e = edges::add(
        &mut conn,
        contributes(NodeRef::new(NodeType::Task, t.id), &o, json!({})),
    )
    .unwrap();
    edges::add(
        &mut conn,
        contributes(
            NodeRef::new(NodeType::Project, p.id),
            &o,
            json!({"weight": 0.7}),
        ),
    )
    .unwrap();

    let d = views::objective_detail(&conn, o.id).unwrap();
    let shown: Vec<_> = d
        .contributions
        .iter()
        .map(|c| (c.node.label.as_str(), c.status.as_str(), c.weight))
        .collect();
    // Projects first, then tasks; unset weight is None.
    assert_eq!(
        shown,
        vec![
            ("EU region", "active", Some(0.7)),
            ("alpha task", "todo", None)
        ]
    );
    assert_eq!(d.contributions[1].edge_id, e.id);
}

#[test]
fn update_attrs_changes_the_edge_and_logs_once() {
    let mut conn = db();
    let o = objective(&mut conn, "Launch EU", None);
    let p = project(&mut conn, "EU region");
    let rp = NodeRef::new(NodeType::Project, p.id);
    let e = edges::add(&mut conn, contributes(rp, &o, json!({"weight": 1.0}))).unwrap();
    let before = activity::count(&conn).unwrap();

    let u = edges::update_attrs(&mut conn, e.id, json!({"weight": 0.4})).unwrap();
    assert_eq!(u.attrs, json!({"weight": 0.4}));
    assert_eq!(
        edges::get(&conn, e.id).unwrap().attrs,
        json!({"weight": 0.4})
    );
    assert_eq!(activity::count(&conn).unwrap(), before + 1);
    let latest = &history(&conn, p.id)[0];
    assert_eq!(latest.action, ActivityAction::Updated);
    assert_eq!(
        latest.diff,
        json!({"contributes_to link": [{"weight": 1.0}, {"weight": 0.4}]})
    );

    // Same attrs: no write. Removed edge: error.
    edges::update_attrs(&mut conn, e.id, json!({"weight": 0.4})).unwrap();
    assert_eq!(activity::count(&conn).unwrap(), before + 1);
    edges::remove(&mut conn, e.id).unwrap();
    assert!(matches!(
        edges::update_attrs(&mut conn, e.id, json!({})),
        Err(StoreError::EdgeNotFound(_))
    ));
}

#[test]
fn list_summaries_are_active_only_and_sorted_by_label() {
    let mut conn = db();
    let (b, a) = (project(&mut conn, "beta"), project(&mut conn, "Alpha"));
    let gone = project(&mut conn, "gone");
    nodes::archive(&mut conn, NodeRef::new(NodeType::Project, gone.id)).unwrap();
    let list = nodes::list_summaries(&conn, NodeType::Project).unwrap();
    assert_eq!(
        list.iter().map(|s| s.label.as_str()).collect::<Vec<_>>(),
        vec!["Alpha", "beta"]
    );
    assert_eq!(list[0].node, NodeRef::new(NodeType::Project, a.id));
    let _ = b;
    // Person uses its name column.
    person(&mut conn, "Priya");
    assert_eq!(
        nodes::list_summaries(&conn, NodeType::Person).unwrap()[0].label,
        "Priya"
    );
}

// ------------------------------------------------------------------ projects

fn project_with(
    conn: &mut Connection,
    title: &str,
    slug: Option<&str>,
) -> minimap_store::Result<Project> {
    projects::create(
        conn,
        CreateProject {
            title: title.into(),
            slug: slug.map(String::from),
            description: String::new(),
            owner_person_id: None,
            start_date: None,
            target_date: None,
            status: None,
            priority: None,
        },
    )
}

#[test]
fn project_handles_are_generated_unique_and_validated() {
    let mut conn = db();
    let a = project_with(&mut conn, "API Launch", None).unwrap();
    let b = project_with(&mut conn, "API launch!", None).unwrap();
    assert_eq!(
        (a.slug.as_str(), b.slug.as_str()),
        ("api-launch", "api-launch-2")
    );

    // Explicit handles are validated and must be free.
    assert_eq!(
        project_with(&mut conn, "Other", Some("custom"))
            .unwrap()
            .slug,
        "custom"
    );
    assert!(
        matches!(project_with(&mut conn, "Dup", Some("custom")), Err(StoreError::Invalid(m)) if m.contains("already used"))
    );
    assert!(matches!(
        project_with(&mut conn, "Bad", Some("Not Valid")),
        Err(StoreError::Invalid(_))
    ));
    // Blank explicit handle falls back to the title.
    assert_eq!(
        project_with(&mut conn, "Fallback", Some("  "))
            .unwrap()
            .slug,
        "fallback"
    );
    // A rejected create writes nothing.
    assert_eq!(projects::list(&conn, false).unwrap().len(), 4);
}

#[test]
fn project_handle_is_editable_and_freed_by_archiving() {
    let mut conn = db();
    let a = project_with(&mut conn, "Alpha", None).unwrap();
    let b = project_with(&mut conn, "Beta", None).unwrap();

    // Renaming a project keeps its handle; the handle itself is editable.
    let renamed = projects::update(
        &mut conn,
        a.id,
        UpdateProject {
            title: Some("Alpha Two".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(renamed.slug, "alpha");
    assert!(matches!(
        projects::update(
            &mut conn,
            b.id,
            UpdateProject {
                slug: Some("alpha".into()),
                ..Default::default()
            }
        ),
        Err(StoreError::Invalid(_))
    ));
    projects::update(
        &mut conn,
        a.id,
        UpdateProject {
            slug: Some("alpha".into()),
            ..Default::default()
        },
    )
    .unwrap(); // own handle: no-op
    projects::update(
        &mut conn,
        a.id,
        UpdateProject {
            slug: Some("a2".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        history(&conn, a.id)[0].diff,
        json!({"slug": ["alpha", "a2"]})
    );

    // Archiving frees the handle for reuse.
    projects::archive(&mut conn, b.id, TaskDisposition::Inbox).unwrap();
    assert_eq!(
        project_with(&mut conn, "Beta again", Some("beta"))
            .unwrap()
            .slug,
        "beta"
    );
    // ...and restoring the archived one then hits the unique index.
    assert!(matches!(
        nodes::unarchive(&mut conn, NodeRef::new(NodeType::Project, b.id)),
        Err(StoreError::Constraint(_))
    ));
}

fn task_in(conn: &mut Connection, title: &str, project: Uuid) -> Task {
    tasks::create(
        conn,
        CreateTask {
            title: title.into(),
            assignee: AssigneeChoice::Nobody,
            description: String::new(),
            project_id: Some(project),
            status: None,
            estimate_days: None,
            start_date: None,
            due_date: None,
            priority: None,
        },
    )
    .unwrap()
}

#[test]
fn archiving_a_project_can_archive_its_tasks() {
    let mut conn = db();
    let p = project_with(&mut conn, "Alpha", None).unwrap();
    let (t1, t2) = (
        task_in(&mut conn, "one", p.id),
        task_in(&mut conn, "two", p.id),
    );
    let other = project_with(&mut conn, "Other", None).unwrap();
    let t3 = task_in(&mut conn, "elsewhere", other.id);
    // A link on a task goes down with it.
    edges::add(&mut conn, blocks(&t1, &t3)).unwrap();

    projects::archive(&mut conn, p.id, TaskDisposition::Archive).unwrap();
    assert!(projects::get(&conn, p.id).unwrap().archived_at.is_some());
    for t in [&t1, &t2] {
        assert!(tasks::get(&conn, t.id).unwrap().archived_at.is_some());
        assert!(actions(&conn, t.id).contains(&ActivityAction::Archived));
    }
    assert!(tasks::get(&conn, t3.id).unwrap().archived_at.is_none());
    assert!(edges::list_active(&conn).unwrap().is_empty());
    assert!(matches!(
        projects::archive(&mut conn, p.id, TaskDisposition::Archive),
        Err(StoreError::AlreadyArchived { .. })
    ));
}

#[test]
fn archiving_a_project_can_move_its_tasks_to_the_inbox() {
    let mut conn = db();
    let p = project_with(&mut conn, "Alpha", None).unwrap();
    let t = task_in(&mut conn, "one", p.id);
    let done = task_in(&mut conn, "finished", p.id);
    tasks::update(
        &mut conn,
        done.id,
        UpdateTask {
            status: Some(TaskStatus::Done),
            ..Default::default()
        },
    )
    .unwrap();

    projects::archive(&mut conn, p.id, TaskDisposition::Inbox).unwrap();
    for id in [t.id, done.id] {
        let after = tasks::get(&conn, id).unwrap();
        assert_eq!(after.project_id, None);
        assert!(after.archived_at.is_none());
        let latest = &history(&conn, id)[0];
        assert_eq!(latest.action, ActivityAction::Updated);
        assert_eq!(latest.diff, json!({"project_id": [p.id, null]}));
    }
    assert!(projects::get(&conn, p.id).unwrap().archived_at.is_some());
}

#[test]
fn project_archive_is_atomic() {
    let mut conn = db();
    let missing = projects::archive(&mut conn, Uuid::now_v7(), TaskDisposition::Archive);
    assert!(matches!(missing, Err(StoreError::NotFound { .. })));
}

#[test]
fn project_rows_and_detail() {
    let mut conn = db();
    let me = person(&mut conn, "Priya");
    let (eu, growth) = (
        objective(&mut conn, "Launch EU", None),
        objective(&mut conn, "Grow ARR", None),
    );
    let p = project_with(&mut conn, "EU region", None).unwrap();
    projects::update(
        &mut conn,
        p.id,
        UpdateProject {
            owner_person_id: Patch::Set(me.id),
            ..Default::default()
        },
    )
    .unwrap();
    let rp = NodeRef::new(NodeType::Project, p.id);
    edges::add(&mut conn, contributes(rp, &growth, json!({"weight": 0.5}))).unwrap();
    edges::add(&mut conn, contributes(rp, &eu, json!({}))).unwrap();
    let dep = project_with(&mut conn, "Billing", None).unwrap();
    edges::add(
        &mut conn,
        edge(
            rp,
            NodeRef::new(NodeType::Project, dep.id),
            EdgeType::DependsOn,
            json!({}),
        ),
    )
    .unwrap();
    let (t1, t2) = (task_in(&mut conn, "a", p.id), task_in(&mut conn, "b", p.id));
    tasks::update(
        &mut conn,
        t1.id,
        UpdateTask {
            status: Some(TaskStatus::Done),
            ..Default::default()
        },
    )
    .unwrap();
    let _ = t2;

    let rows = views::project_rows(&conn).unwrap();
    let row = rows.iter().find(|r| r.project.id == p.id).unwrap();
    assert_eq!(row.owner.as_ref().unwrap().label, "Priya");
    assert_eq!(
        row.objectives
            .iter()
            .map(|o| o.label.as_str())
            .collect::<Vec<_>>(),
        vec!["Grow ARR", "Launch EU"]
    );
    assert_eq!((row.task_count, row.done_task_count), (2, 1));
    let billing = rows.iter().find(|r| r.project.id == dep.id).unwrap();
    assert_eq!(
        (
            billing.owner.is_none(),
            billing.objectives.len(),
            billing.task_count
        ),
        (true, 0, 0)
    );

    let d = views::project_detail(&conn, p.id).unwrap();
    assert_eq!(d.owner.unwrap().label, "Priya");
    assert_eq!(d.objectives.len(), 2);
    assert_eq!(
        (
            d.objectives[0].node.label.as_str(),
            d.objectives[0].weight,
            d.objectives[0].status.as_str()
        ),
        ("Grow ARR", Some(0.5), "on_track")
    );
    assert_eq!(d.depends_on[0].node.label, "Billing");
    assert_eq!(d.tasks.len(), 2);
    let dd = views::project_detail(&conn, dep.id).unwrap();
    assert_eq!(
        dd.needed_by
            .iter()
            .map(|n| n.label.as_str())
            .collect::<Vec<_>>(),
        vec!["EU region"]
    );

    let preview = views::project_archive_preview(&conn, p.id).unwrap();
    assert_eq!(preview.tasks.len(), 2);
    // Archived projects drop out of the rows.
    projects::archive(&mut conn, dep.id, TaskDisposition::Inbox).unwrap();
    assert_eq!(views::project_rows(&conn).unwrap().len(), 1);
}

// --------------------------------------------------------------------- tasks

fn new_task(title: &str, assignee: AssigneeChoice) -> CreateTask {
    CreateTask {
        title: title.into(),
        assignee,
        description: String::new(),
        project_id: None,
        status: None,
        estimate_days: None,
        start_date: None,
        due_date: None,
        priority: None,
    }
}

fn assignee_of(conn: &Connection, task: Uuid) -> Option<Uuid> {
    edges::list_active_of_type(conn, EdgeType::AssignedTo)
        .unwrap()
        .into_iter()
        .find(|e| e.from_id == task)
        .map(|e| e.to_id)
}

#[test]
fn new_tasks_are_assigned_to_me_by_default() {
    let mut conn = db();
    // Before first-run setup there is nobody to assign to.
    let early = tasks::create(&mut conn, new_task("early", AssigneeChoice::Me)).unwrap();
    assert_eq!(assignee_of(&conn, early.id), None);

    let me = people::ensure_self(&mut conn, "Me").unwrap();
    let raj = person(&mut conn, "Raj");
    let mine = tasks::create(&mut conn, new_task("mine", AssigneeChoice::Me)).unwrap();
    let his = tasks::create(&mut conn, new_task("his", AssigneeChoice::Person(raj.id))).unwrap();
    let nobody = tasks::create(&mut conn, new_task("nobody", AssigneeChoice::Nobody)).unwrap();
    assert_eq!(assignee_of(&conn, mine.id), Some(me.id));
    assert_eq!(assignee_of(&conn, his.id), Some(raj.id));
    assert_eq!(assignee_of(&conn, nobody.id), None);
    // The assignment is part of the create: activity shows both.
    assert_eq!(
        actions(&conn, mine.id),
        vec![ActivityAction::Created, ActivityAction::EdgeAdded]
    );
    // An unknown person fails the whole create.
    let before = tasks::list(&conn, true).unwrap().len();
    let ghost = tasks::create(
        &mut conn,
        new_task("ghost", AssigneeChoice::Person(Uuid::now_v7())),
    );
    assert!(ghost.is_err());
    assert_eq!(tasks::list(&conn, true).unwrap().len(), before);
}

#[test]
fn create_many_is_all_or_nothing() {
    let mut conn = db();
    let me = people::ensure_self(&mut conn, "Me").unwrap();
    let made = tasks::create_many(
        &mut conn,
        vec![
            new_task("a", AssigneeChoice::Me),
            new_task("b", AssigneeChoice::Me),
            new_task("c", AssigneeChoice::Me),
        ],
    )
    .unwrap();
    assert_eq!(made.len(), 3);
    assert!(made.iter().all(|t| assignee_of(&conn, t.id) == Some(me.id)));

    let count = activity::count(&conn).unwrap();
    let failed = tasks::create_many(
        &mut conn,
        vec![
            new_task("ok", AssigneeChoice::Me),
            new_task("   ", AssigneeChoice::Me),
        ],
    );
    assert!(matches!(failed, Err(StoreError::Invalid(_))));
    assert_eq!(
        tasks::list(&conn, true).unwrap().len(),
        3,
        "no partial batch"
    );
    assert_eq!(
        activity::count(&conn).unwrap(),
        count,
        "no partial activity"
    );
}

#[test]
fn set_assignee_replaces_the_assignment() {
    let mut conn = db();
    let (me, raj) = (
        people::ensure_self(&mut conn, "Me").unwrap(),
        person(&mut conn, "Raj"),
    );
    let t = tasks::create(&mut conn, new_task("t", AssigneeChoice::Me)).unwrap();
    assert_eq!(assignee_of(&conn, t.id), Some(me.id));

    tasks::set_assignee(&mut conn, t.id, Some(raj.id)).unwrap();
    assert_eq!(assignee_of(&conn, t.id), Some(raj.id));
    let after_change = activity::count(&conn).unwrap();
    tasks::set_assignee(&mut conn, t.id, Some(raj.id)).unwrap(); // same person: nothing
    assert_eq!(activity::count(&conn).unwrap(), after_change);

    tasks::set_assignee(&mut conn, t.id, None).unwrap();
    assert_eq!(assignee_of(&conn, t.id), None);
    // Back to a previous assignee revives the archived edge.
    tasks::set_assignee(&mut conn, t.id, Some(me.id)).unwrap();
    assert_eq!(assignee_of(&conn, t.id), Some(me.id));
    assert_eq!(edges::list_for_node(&conn, t.id, false).unwrap().len(), 1);
    // Unknown task / person.
    assert!(matches!(
        tasks::set_assignee(&mut conn, Uuid::now_v7(), None),
        Err(StoreError::NotFound { .. })
    ));
    assert!(tasks::set_assignee(&mut conn, t.id, Some(Uuid::now_v7())).is_err());
    assert_eq!(
        assignee_of(&conn, t.id),
        Some(me.id),
        "a failed change keeps the old assignee"
    );
}

#[test]
fn task_rows_and_detail_include_project_and_assignee() {
    let mut conn = db();
    let raj = person(&mut conn, "Raj");
    let p = project_with(&mut conn, "API Launch", None).unwrap();
    let mut input = new_task("Fix login", AssigneeChoice::Person(raj.id));
    input.project_id = Some(p.id);
    let t = tasks::create(&mut conn, input).unwrap();
    let loose = tasks::create(&mut conn, new_task("loose", AssigneeChoice::Nobody)).unwrap();

    let rows = views::task_rows(&conn).unwrap();
    let row = rows.iter().find(|r| r.task.id == t.id).unwrap();
    assert_eq!(row.project.as_ref().unwrap().label, "API Launch");
    assert_eq!(row.assignee.as_ref().unwrap().label, "Raj");
    let row = rows.iter().find(|r| r.task.id == loose.id).unwrap();
    assert!(row.project.is_none() && row.assignee.is_none());

    let d = views::task_detail(&conn, t.id).unwrap();
    assert_eq!(
        (
            d.project.unwrap().label.as_str(),
            d.assignee.unwrap().label.as_str()
        ),
        ("API Launch", "Raj")
    );
    // Archived tasks drop out of the rows.
    nodes::archive(&mut conn, NodeRef::new(NodeType::Task, loose.id)).unwrap();
    assert_eq!(views::task_rows(&conn).unwrap().len(), 1);
}

#[test]
fn settings_have_defaults_and_are_validated() {
    let mut conn = db();
    assert_eq!(settings::get(&conn).unwrap().hours_per_day, 8.0);
    let s = settings::update(
        &mut conn,
        UpdateSettings {
            hours_per_day: Some(6.5),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(s.hours_per_day, 6.5);
    assert_eq!(settings::get(&conn).unwrap().hours_per_day, 6.5);
    settings::update(
        &mut conn,
        UpdateSettings {
            hours_per_day: Some(7.0),
            ..Default::default()
        },
    )
    .unwrap(); // overwrite
    assert_eq!(settings::get(&conn).unwrap().hours_per_day, 7.0);
    settings::update(&mut conn, UpdateSettings::default()).unwrap(); // nothing to change
    assert_eq!(settings::get(&conn).unwrap().hours_per_day, 7.0);
    for bad in [0.0, -1.0, 25.0, f64::NAN, f64::INFINITY] {
        assert!(
            matches!(
                settings::update(
                    &mut conn,
                    UpdateSettings {
                        hours_per_day: Some(bad),
                        ..Default::default()
                    }
                ),
                Err(StoreError::Invalid(_))
            ),
            "{bad}"
        );
    }
    assert_eq!(settings::get(&conn).unwrap().hours_per_day, 7.0);
}

#[test]
fn theme_setting_defaults_to_dark_and_is_validated() {
    let mut conn = db();
    assert_eq!(settings::get(&conn).unwrap().theme, "minimap-dark");
    let s = settings::update(
        &mut conn,
        UpdateSettings {
            theme: Some("catppuccin-mocha".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(s.theme, "catppuccin-mocha");
    assert_eq!(settings::get(&conn).unwrap().theme, "catppuccin-mocha");
    // Other settings are untouched by a theme change, and vice versa.
    assert_eq!(s.hours_per_day, 8.0);
    settings::update(
        &mut conn,
        UpdateSettings {
            theme: Some("system".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(settings::get(&conn).unwrap().theme, "system");
    for bad in ["", "Nord", "my theme", "a_b", &"x".repeat(41)] {
        let r = settings::update(
            &mut conn,
            UpdateSettings {
                theme: Some(bad.into()),
                ..Default::default()
            },
        );
        assert!(matches!(r, Err(StoreError::Invalid(_))), "{bad:?}");
    }
    // A bad value in a combined update changes nothing.
    let r = settings::update(
        &mut conn,
        UpdateSettings {
            hours_per_day: Some(5.0),
            theme: Some("Bad Id".into()),
            ..Default::default()
        },
    );
    assert!(r.is_err());
    assert_eq!(settings::get(&conn).unwrap().hours_per_day, 8.0);
    assert_eq!(settings::get(&conn).unwrap().theme, "system");
}

// ---------------------------------------------------------------- waiting-on

fn waiting(conn: &mut Connection, person: Uuid, description: &str) -> WaitingOn {
    waiting_on::create(
        conn,
        CreateWaitingOn {
            description: description.into(),
            person_id: person,
            asked_on: None,
            expected_by: None,
            follow_up_on: None,
        },
    )
    .unwrap()
}

#[test]
fn follow_up_date_is_saved_snoozes_and_logs_changes() {
    let mut conn = db();
    let raj = person(&mut conn, "Raj");
    let w = waiting(&mut conn, raj.id, "Security review");
    assert_eq!(w.follow_up_on, None);
    let until = timefmt::parse_date("2027-02-10").unwrap();
    let w = waiting_on::update(
        &mut conn,
        w.id,
        UpdateWaitingOn {
            follow_up_on: Patch::Set(until),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        waiting_on::get(&conn, w.id).unwrap().follow_up_on,
        Some(until)
    );
    assert_eq!(
        history(&conn, w.id)[0].diff,
        json!({"follow_up_on": [null, "2027-02-10"]})
    );
    waiting_on::update(
        &mut conn,
        w.id,
        UpdateWaitingOn {
            follow_up_on: Patch::Clear,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(waiting_on::get(&conn, w.id).unwrap().follow_up_on, None);
    // It can also be set when creating.
    let created = waiting_on::create(
        &mut conn,
        CreateWaitingOn {
            description: "x".into(),
            person_id: raj.id,
            asked_on: None,
            expected_by: None,
            follow_up_on: Some(until),
        },
    )
    .unwrap();
    assert_eq!(created.follow_up_on, Some(until));
}

#[test]
fn waiting_on_items_name_the_person_and_the_target() {
    let mut conn = db();
    let raj = person(&mut conn, "Raj");
    let p = project_with(&mut conn, "API Launch", None).unwrap();
    let t = task(&mut conn, "Fix login");
    let (w1, w2, w3) = (
        waiting(&mut conn, raj.id, "one"),
        waiting(&mut conn, raj.id, "two"),
        waiting(&mut conn, raj.id, "gone"),
    );
    let about = |w: &WaitingOn, to: NodeRef| {
        edge(
            NodeRef::new(NodeType::WaitingOn, w.id),
            to,
            EdgeType::About,
            json!({}),
        )
    };
    edges::add(&mut conn, about(&w1, NodeRef::new(NodeType::Project, p.id))).unwrap();
    edges::add(&mut conn, about(&w2, NodeRef::new(NodeType::Task, t.id))).unwrap();
    nodes::archive(&mut conn, NodeRef::new(NodeType::WaitingOn, w3.id)).unwrap();

    let items = views::waiting_on_items(&conn).unwrap();
    assert_eq!(items.len(), 2, "archived ones are not listed");
    let find = |id| items.iter().find(|i| i.waiting.id == id).unwrap();
    assert_eq!(find(w1.id).person.label, "Raj");
    assert_eq!(find(w1.id).about.as_ref().unwrap().label, "API Launch");
    assert_eq!(find(w2.id).about.as_ref().unwrap().label, "Fix login");
    // Removing the link clears the target.
    let link = edges::list_active_of_type(&conn, EdgeType::About)
        .unwrap()
        .remove(0);
    edges::remove(&mut conn, link.id).unwrap();
    assert!(views::waiting_on_items(&conn)
        .unwrap()
        .iter()
        .all(|i| i.about.is_none() || i.waiting.id != link.from_id));
}

#[test]
fn stale_threshold_setting_defaults_to_seven_and_is_validated() {
    let mut conn = db();
    assert_eq!(settings::get(&conn).unwrap().stale_waiting_days, 7);
    let s = settings::update(
        &mut conn,
        UpdateSettings {
            stale_waiting_days: Some(14),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!((s.stale_waiting_days, s.hours_per_day), (14, 8.0));
    assert_eq!(settings::get(&conn).unwrap().stale_waiting_days, 14);
    for bad in [0u32, 366, 10_000] {
        let r = settings::update(
            &mut conn,
            UpdateSettings {
                stale_waiting_days: Some(bad),
                ..Default::default()
            },
        );
        assert!(matches!(r, Err(StoreError::Invalid(_))), "{bad}");
    }
    // A bad value alongside a good one changes nothing.
    let r = settings::update(
        &mut conn,
        UpdateSettings {
            stale_waiting_days: Some(0),
            hours_per_day: Some(6.0),
            ..Default::default()
        },
    );
    assert!(r.is_err());
    assert_eq!(settings::get(&conn).unwrap().hours_per_day, 8.0);
    assert_eq!(settings::get(&conn).unwrap().stale_waiting_days, 14);
}

// --------------------------------------------------------------------- notes

fn make_note(conn: &mut Connection, title: &str, body: &str) -> Note {
    notes::create(
        conn,
        CreateNote {
            title: title.into(),
            body: body.into(),
            note_date: None,
            kind: None,
        },
    )
    .unwrap()
}

fn token(label: &str, id: Uuid) -> String {
    format!("@[{label}](node:{id})")
}

fn mention_targets(conn: &Connection, note: Uuid) -> Vec<Uuid> {
    let mut v: Vec<Uuid> = edges::list_active_of_type(conn, EdgeType::Mentions)
        .unwrap()
        .into_iter()
        .filter(|e| e.from_id == note)
        .map(|e| e.to_id)
        .collect();
    v.sort();
    v
}

#[test]
fn nodes_can_be_found_by_id() {
    let mut conn = db();
    let p = person(&mut conn, "Priya");
    let t = task(&mut conn, "t");
    assert_eq!(
        nodes::find(&conn, p.id).unwrap(),
        Some(NodeRef::new(NodeType::Person, p.id))
    );
    assert_eq!(
        nodes::find(&conn, t.id).unwrap(),
        Some(NodeRef::new(NodeType::Task, t.id))
    );
    assert_eq!(nodes::find(&conn, Uuid::now_v7()).unwrap(), None);
}

#[test]
fn mentions_in_the_body_become_links_and_follow_edits() {
    let mut conn = db();
    let (priya, raj) = (person(&mut conn, "Priya"), person(&mut conn, "Raj"));
    let t = task(&mut conn, "Fix login");
    let n = make_note(
        &mut conn,
        "1:1",
        &format!("Chat with {} today", token("Priya", priya.id)),
    );
    assert_eq!(mention_targets(&conn, n.id), vec![priya.id]);
    assert_eq!(
        actions(&conn, n.id),
        vec![ActivityAction::Created, ActivityAction::EdgeAdded]
    );

    // Add two more, keep one.
    let body = format!(
        "{} {} {}",
        token("Priya", priya.id),
        token("Raj", raj.id),
        token("Fix login", t.id)
    );
    notes::update(
        &mut conn,
        n.id,
        UpdateNote {
            body: Some(body),
            ..Default::default()
        },
    )
    .unwrap();
    let mut want = vec![priya.id, raj.id, t.id];
    want.sort();
    assert_eq!(mention_targets(&conn, n.id), want);

    // Removing the text removes the link; renaming the label alone does not touch it.
    let body = format!(
        "{} {}",
        token("Priya S.", priya.id),
        token("Fix login", t.id)
    );
    notes::update(
        &mut conn,
        n.id,
        UpdateNote {
            body: Some(body),
            ..Default::default()
        },
    )
    .unwrap();
    let mut want = vec![priya.id, t.id];
    want.sort();
    assert_eq!(mention_targets(&conn, n.id), want);
    assert_eq!(
        edges::list_for_node(&conn, raj.id, true)
            .unwrap()
            .iter()
            .filter(|e| e.archived_at.is_some())
            .count(),
        1
    );

    // The note shows on the person's side (incoming mention).
    let links = edges::links_for_node(&conn, priya.id).unwrap();
    assert!(links
        .iter()
        .any(|l| !l.outgoing && l.edge.edge_type == EdgeType::Mentions && l.other.label == "1:1"));

    // Mentioning again after removal revives the link.
    let again = token("Raj", raj.id);
    notes::update(
        &mut conn,
        n.id,
        UpdateNote {
            body: Some(again),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(mention_targets(&conn, n.id), vec![raj.id]);
}

#[test]
fn mentions_of_missing_archived_or_own_nodes_are_ignored() {
    let mut conn = db();
    let gone = person(&mut conn, "Gone");
    nodes::archive(&mut conn, NodeRef::new(NodeType::Person, gone.id)).unwrap();
    let n = make_note(&mut conn, "n", "");
    let body = format!(
        "{} {} {} @priya @[bad](node:nope)",
        token("Ghost", Uuid::now_v7()),
        token("Gone", gone.id),
        token("Me", n.id)
    );
    notes::update(
        &mut conn,
        n.id,
        UpdateNote {
            body: Some(body.clone()),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(mention_targets(&conn, n.id).is_empty());
    assert_eq!(
        notes::get(&conn, n.id).unwrap().body,
        body,
        "the text is kept as written"
    );
}

#[test]
fn archiving_a_note_archives_its_mentions() {
    let mut conn = db();
    let p = person(&mut conn, "Priya");
    let n = make_note(&mut conn, "n", &token("Priya", p.id));
    nodes::archive(&mut conn, NodeRef::new(NodeType::Note, n.id)).unwrap();
    assert!(edges::list_active(&conn).unwrap().is_empty());
}

#[test]
fn activity_records_the_size_of_a_body_not_its_text() {
    let mut conn = db();
    let n = make_note(&mut conn, "Secret plans", "very private text");
    assert_eq!(
        history(&conn, n.id)[0].diff["body"],
        json!([null, "17 chars"])
    );
    notes::update(
        &mut conn,
        n.id,
        UpdateNote {
            body: Some("even more private text".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let latest = &history(&conn, n.id)[0];
    assert_eq!(latest.action, ActivityAction::Updated);
    assert_eq!(latest.diff, json!({"body": ["17 chars", "22 chars"]}));
    for a in history(&conn, n.id) {
        assert!(!a.diff.to_string().contains("private"), "{:?}", a.diff);
    }
}

#[test]
fn autosaves_in_one_session_share_one_activity_row() {
    let mut conn = db();
    let n = make_note(&mut conn, "Draft", "");
    for body in ["a", "ab", "abc", "abcd"] {
        notes::update(
            &mut conn,
            n.id,
            UpdateNote {
                body: Some(body.into()),
                ..Default::default()
            },
        )
        .unwrap();
    }
    assert_eq!(
        actions(&conn, n.id),
        vec![ActivityAction::Created, ActivityAction::Updated]
    );
    assert_eq!(
        history(&conn, n.id)[0].diff,
        json!({"body": ["0 chars", "4 chars"]})
    );

    // Another field joins the same row; a change that nets out to nothing removes the row.
    notes::update(
        &mut conn,
        n.id,
        UpdateNote {
            title: Some("Final".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(actions(&conn, n.id).len(), 2);
    assert_eq!(
        history(&conn, n.id)[0].diff,
        json!({"body": ["0 chars", "4 chars"], "title": ["Draft", "Final"]})
    );
    let quiet = make_note(&mut conn, "Quiet", "x");
    notes::update(
        &mut conn,
        quiet.id,
        UpdateNote {
            body: Some("xy".into()),
            ..Default::default()
        },
    )
    .unwrap();
    notes::update(
        &mut conn,
        quiet.id,
        UpdateNote {
            body: Some("x".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        actions(&conn, quiet.id),
        vec![ActivityAction::Created],
        "typed and undone: nothing to show"
    );
}

#[test]
fn an_edge_change_between_saves_starts_a_new_row() {
    let mut conn = db();
    let p = person(&mut conn, "Priya");
    let n = make_note(&mut conn, "n", "a");
    notes::update(
        &mut conn,
        n.id,
        UpdateNote {
            body: Some("ab".into()),
            ..Default::default()
        },
    )
    .unwrap();
    // Mentioning someone logs an edge row; the next save is a separate session entry.
    notes::update(
        &mut conn,
        n.id,
        UpdateNote {
            body: Some(token("Priya", p.id)),
            ..Default::default()
        },
    )
    .unwrap();
    notes::update(
        &mut conn,
        n.id,
        UpdateNote {
            title: Some("t".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let kinds = actions(&conn, n.id);
    assert_eq!(
        kinds
            .iter()
            .filter(|a| **a == ActivityAction::EdgeAdded)
            .count(),
        1
    );
    assert!(
        kinds
            .iter()
            .filter(|a| **a == ActivityAction::Updated)
            .count()
            >= 2
    );
}

#[test]
fn checklist_lines_convert_to_tasks_atomically() {
    let mut conn = db();
    let me = people::ensure_self(&mut conn, "Me").unwrap();
    let priya = person(&mut conn, "Priya");
    let project = project_with(&mut conn, "API Launch", None).unwrap();
    let body = format!(
        "# 1:1\n[ ] Send budget to {}\n- [ ] Plan {}\n[ ] Plain item\n- [x] done",
        token("Priya", priya.id),
        token("API Launch", project.id)
    );
    let n = make_note(&mut conn, "1:1", &body);

    // Person mentioned: assigned to them.
    let t = notes::convert_checklist_item(&mut conn, n.id, 1, "Send budget to Priya").unwrap();
    assert_eq!(t.title, "Send budget to Priya");
    assert_eq!(assignee_of(&conn, t.id), Some(priya.id));
    let after = notes::get(&conn, n.id).unwrap().body;
    assert!(
        after.contains(&format!("- [x] @[Send budget to Priya](node:{})", t.id)),
        "{after}"
    );
    // The note now mentions the task as well.
    assert!(mention_targets(&conn, n.id).contains(&t.id));

    // Project mentioned: the task is filed there, and (no person) goes to me.
    let t2 = notes::convert_checklist_item(&mut conn, n.id, 2, "Plan API Launch").unwrap();
    assert_eq!(t2.project_id, Some(project.id));
    assert_eq!(assignee_of(&conn, t2.id), Some(me.id));

    // No mentions: default assignee, no project.
    let t3 = notes::convert_checklist_item(&mut conn, n.id, 3, "Plain item").unwrap();
    assert_eq!(
        (t3.project_id, assignee_of(&conn, t3.id)),
        (None, Some(me.id))
    );

    // A converted line is no longer an item; nor are checked or missing lines.
    for line in [1usize, 4, 99] {
        let before = tasks::list(&conn, true).unwrap().len();
        assert!(matches!(
            notes::convert_checklist_item(&mut conn, n.id, line, "anything"),
            Err(StoreError::Invalid(_))
        ));
        assert_eq!(
            tasks::list(&conn, true).unwrap().len(),
            before,
            "no task for a refused line"
        );
    }
}

#[test]
fn converting_a_missing_note_changes_nothing() {
    let mut conn = db();
    let raj = person(&mut conn, "Raj");
    let n = make_note(&mut conn, "n", &format!("[ ] Ask {}", token("Raj", raj.id)));
    // The conversion is one transaction; an unknown note fails before touching anything.
    assert!(matches!(
        notes::convert_checklist_item(&mut conn, Uuid::now_v7(), 0, "x"),
        Err(StoreError::NotFound { .. })
    ));
    assert!(tasks::list(&conn, true).unwrap().is_empty());
    assert_eq!(
        notes::get(&conn, n.id).unwrap().body,
        format!("[ ] Ask {}", token("Raj", raj.id))
    );
}

#[test]
fn note_items_carry_their_mentions() {
    let mut conn = db();
    let p = person(&mut conn, "Priya");
    let a = make_note(&mut conn, "with", &token("Priya", p.id));
    let b = make_note(&mut conn, "without", "plain");
    let items = views::note_items(&conn).unwrap();
    let find = |id| items.iter().find(|i| i.note.id == id).unwrap();
    assert_eq!(
        find(a.id)
            .mentions
            .iter()
            .map(|m| m.label.as_str())
            .collect::<Vec<_>>(),
        vec!["Priya"]
    );
    assert!(find(b.id).mentions.is_empty());
    assert_eq!(views::note_item(&conn, a.id).unwrap().mentions.len(), 1);
    nodes::archive(&mut conn, NodeRef::new(NodeType::Note, b.id)).unwrap();
    assert_eq!(views::note_items(&conn).unwrap().len(), 1);
}

#[test]
fn a_changed_line_is_not_converted_by_a_stale_request() {
    let mut conn = db();
    let n = make_note(&mut conn, "n", "[ ] Send budget\n[ ] Book room");
    // The user inserted a line above after the checklist was loaded: line 1 is now different.
    notes::update(
        &mut conn,
        n.id,
        UpdateNote {
            body: Some("intro\n[ ] Send budget\n[ ] Book room".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let stale = notes::convert_checklist_item(&mut conn, n.id, 1, "Book room");
    assert!(matches!(stale, Err(StoreError::Invalid(m)) if m.contains("changed")));
    assert!(tasks::list(&conn, true).unwrap().is_empty());
    // With the current text it works.
    let ok = notes::convert_checklist_item(&mut conn, n.id, 2, "Book room").unwrap();
    assert_eq!(ok.title, "Book room");
}

fn make_decision(conn: &mut Connection, title: &str, status: Option<DecisionStatus>) -> Decision {
    decisions::create(
        conn,
        CreateDecision {
            title: title.into(),
            context: String::new(),
            decision: String::new(),
            rationale: String::new(),
            decided_on: None,
            status,
        },
    )
    .unwrap()
}

#[test]
fn deciding_stamps_a_date_unless_one_is_given() {
    let mut conn = db();
    let draft = make_decision(&mut conn, "Postgres", None);
    assert_eq!(draft.status, DecisionStatus::Proposed);
    assert_eq!(draft.decided_on, None);

    let decided = decisions::update(
        &mut conn,
        draft.id,
        UpdateDecision {
            status: Some(DecisionStatus::Decided),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(decided.decided_on, Some(today()));

    // Created as decided: dated too. An explicit date is kept.
    assert_eq!(
        make_decision(&mut conn, "x", Some(DecisionStatus::Decided)).decided_on,
        Some(today())
    );
    let dated = decisions::create(
        &mut conn,
        CreateDecision {
            title: "y".into(),
            context: String::new(),
            decision: String::new(),
            rationale: String::new(),
            decided_on: Some(time::macros::date!(2027 - 01 - 05)),
            status: Some(DecisionStatus::Decided),
        },
    )
    .unwrap();
    assert_eq!(dated.decided_on, Some(time::macros::date!(2027 - 01 - 05)));

    // Saving the same status again writes nothing.
    let again = decisions::update(
        &mut conn,
        decided.id,
        UpdateDecision {
            status: Some(DecisionStatus::Decided),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(again.updated_at, decided.updated_at, "no-op writes nothing");
}

#[test]
fn superseding_links_and_marks_in_one_transaction() {
    let mut conn = db();
    let old = make_decision(&mut conn, "Old", Some(DecisionStatus::Decided));
    let new = make_decision(&mut conn, "New", Some(DecisionStatus::Decided));
    decisions::supersede(&mut conn, new.id, old.id).unwrap();

    assert_eq!(
        decisions::get(&conn, old.id).unwrap().status,
        DecisionStatus::Superseded
    );
    let links = edges::links_for_node(&conn, new.id).unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].edge.edge_type, EdgeType::Supersedes);
    assert!(links[0].outgoing);
    // One activity row for the edge on the new decision, one status change on the old one.
    assert!(actions(&conn, new.id).contains(&ActivityAction::EdgeAdded));
    let old_history = history(&conn, old.id);
    assert!(old_history
        .iter()
        .any(|a| a.diff.get("status").is_some_and(|s| s[1] == "superseded")));

    // Repeating it fails and changes nothing further.
    let before = history(&conn, old.id).len();
    assert!(matches!(
        decisions::supersede(&mut conn, new.id, old.id),
        Err(StoreError::DuplicateEdge)
    ));
    assert_eq!(history(&conn, old.id).len(), before);

    // An archived decision can't be superseded, and the failed attempt leaves no half-state.
    let other = make_decision(&mut conn, "Other", Some(DecisionStatus::Decided));
    nodes::archive(&mut conn, NodeRef::new(NodeType::Decision, other.id)).unwrap();
    assert!(decisions::supersede(&mut conn, new.id, other.id).is_err());
    assert_eq!(
        decisions::get(&conn, other.id).unwrap().status,
        DecisionStatus::Decided
    );
}

#[test]
fn decision_items_carry_what_they_affect_and_replace() {
    let mut conn = db();
    let old = make_decision(&mut conn, "Old", Some(DecisionStatus::Decided));
    let new = make_decision(&mut conn, "New", Some(DecisionStatus::Decided));
    let project = projects::create(
        &mut conn,
        CreateProject {
            title: "API".into(),
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
    edges::add(
        &mut conn,
        NewEdge {
            edge_type: EdgeType::Affects,
            from: NodeRef::new(NodeType::Decision, new.id),
            to: NodeRef::new(NodeType::Project, project.id),
            attrs: json!({}),
        },
    )
    .unwrap();
    decisions::supersede(&mut conn, new.id, old.id).unwrap();

    let items = views::decision_items(&conn).unwrap();
    let of = |id| items.iter().find(|i| i.decision.id == id).unwrap();
    assert_eq!(of(new.id).affects[0].label, "API");
    assert_eq!(of(old.id).superseded_by.as_ref().unwrap().label, "New");
    assert!(of(new.id).superseded_by.is_none());

    // Archiving the replacement archives its link, so the old decision no longer points at it.
    nodes::archive(&mut conn, NodeRef::new(NodeType::Decision, new.id)).unwrap();
    let items = views::decision_items(&conn).unwrap();
    assert_eq!(items.len(), 1);
    assert!(items[0].superseded_by.is_none());
}

#[test]
fn health_thresholds_default_are_configurable_and_validated() {
    let mut conn = db();
    let defaults = HealthThresholds::default();
    assert_eq!(settings::get(&conn).unwrap().health, defaults);

    let custom = HealthThresholds {
        late_amber_days: 2,
        late_red_days: 10,
        risky_amber_pct: 20,
        risky_red_pct: 50,
        unestimated_amber_pct: 60,
        unestimated_red_pct: 90,
    };
    let s = settings::update(
        &mut conn,
        UpdateSettings {
            health: Some(custom),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(s.health, custom);
    assert_eq!(settings::get(&conn).unwrap().health, custom);
    // Other settings are untouched.
    assert_eq!(s.hours_per_day, 8.0);

    for bad in [
        HealthThresholds {
            late_amber_days: 0,
            ..custom
        },
        HealthThresholds {
            late_amber_days: 11,
            late_red_days: 10,
            ..custom
        },
        HealthThresholds {
            risky_red_pct: 101,
            ..custom
        },
        HealthThresholds {
            unestimated_amber_pct: 95,
            unestimated_red_pct: 90,
            ..custom
        },
        HealthThresholds {
            late_red_days: 366,
            ..custom
        },
    ] {
        let r = settings::update(
            &mut conn,
            UpdateSettings {
                health: Some(bad),
                hours_per_day: Some(6.0),
                ..Default::default()
            },
        );
        assert!(matches!(r, Err(StoreError::Invalid(_))), "{bad:?}");
    }
    // A bad value alongside a good one changed nothing.
    let after = settings::get(&conn).unwrap();
    assert_eq!((after.health, after.hours_per_day), (custom, 8.0));
}
