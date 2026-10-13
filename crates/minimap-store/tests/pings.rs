//! Pings (docs/product-design/05-pings.md, ADR-0020): one live ping per item, clearing, hiding
//! with the item, undo and redo, hard delete, export.

use minimap_core::undo::{plan, Plan};
use minimap_store::*;
use minimap_types::*;
use time::{macros::date, Duration};

const TODAY: time::Date = date!(2027 - 03 - 03);

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

fn on(p: &Project) -> NodeRef {
    NodeRef::new(NodeType::Project, p.id)
}

fn make(conn: &mut Connection, target: NodeRef, note: &str, days: i64) -> Ping {
    pings::create(
        conn,
        TODAY,
        CreatePing {
            target,
            note: note.into(),
            on_date: TODAY + Duration::days(days),
        },
    )
    .unwrap()
}

fn wrote(conn: &mut Connection, f: impl FnOnce(&mut Connection)) -> Vec<Activity> {
    let marker = activity::latest_rowid(conn).unwrap();
    f(conn);
    activity::since(conn, marker).unwrap()
}

fn undo(conn: &mut Connection, rows: &[Activity]) -> Vec<Activity> {
    let Plan::Steps(steps) = plan(rows) else {
        panic!("expected steps");
    };
    undo::apply(conn, &steps).unwrap()
}

fn live(conn: &Connection) -> Vec<(String, String)> {
    let mut rows: Vec<(String, String)> = pings::list_live(conn)
        .unwrap()
        .into_iter()
        .map(|r| {
            (
                r.target.label,
                format!("{}|{}", r.ping.note, r.ping.on_date),
            )
        })
        .collect();
    rows.sort();
    rows
}

#[test]
fn a_ping_has_a_trimmed_note_and_a_day() {
    let mut conn = open_in_memory().unwrap();
    let p = project(&mut conn, "EU");
    let ping = make(&mut conn, on(&p), "  check it ", 2);
    assert_eq!(ping.note, "check it");
    assert_eq!(ping.on_date, TODAY + Duration::days(2));
    assert_eq!(pings::live_for(&conn, p.id).unwrap().unwrap().id, ping.id);
}

#[test]
fn a_past_day_and_a_long_note_are_refused() {
    let mut conn = open_in_memory().unwrap();
    let p = project(&mut conn, "EU");
    let past = pings::create(
        &mut conn,
        TODAY,
        CreatePing {
            target: on(&p),
            note: String::new(),
            on_date: TODAY - Duration::days(1),
        },
    );
    assert!(matches!(past, Err(StoreError::Invalid(m)) if m == "Pick today or later"));
    let long = pings::create(
        &mut conn,
        TODAY,
        CreatePing {
            target: on(&p),
            note: "x".repeat(201),
            on_date: TODAY,
        },
    );
    assert!(matches!(long, Err(StoreError::Invalid(_))));
    assert!(pings::list_all(&conn).unwrap().is_empty());
}

#[test]
fn pinging_an_item_that_has_a_live_ping_changes_that_ping() {
    let mut conn = open_in_memory().unwrap();
    let p = project(&mut conn, "EU");
    let first = make(&mut conn, on(&p), "first", 1);
    let second = make(&mut conn, on(&p), "second", 5);
    assert_eq!(first.id, second.id);
    assert_eq!(second.note, "second");
    assert_eq!(pings::list_all(&conn).unwrap().len(), 1);
    // Two creates inside one transaction (quick-add, say) still make one.
    let tx = conn.transaction().unwrap();
    drop(tx);
    let q = project(&mut conn, "Billing");
    make(&mut conn, on(&q), "a", 0);
    make(&mut conn, on(&q), "b", 0);
    assert_eq!(pings::list_all(&conn).unwrap().len(), 2);
}

#[test]
fn a_ping_cannot_be_put_on_an_archived_item() {
    let mut conn = open_in_memory().unwrap();
    let p = project(&mut conn, "EU");
    nodes::archive(&mut conn, on(&p)).unwrap();
    let err = pings::create(
        &mut conn,
        TODAY,
        CreatePing {
            target: on(&p),
            note: String::new(),
            on_date: TODAY,
        },
    );
    assert!(matches!(err, Err(StoreError::Invalid(_))));
}

#[test]
fn clearing_keeps_the_first_day_and_a_new_ping_starts_fresh() {
    let mut conn = open_in_memory().unwrap();
    let p = project(&mut conn, "EU");
    let ping = make(&mut conn, on(&p), "x", 0);
    let cleared = pings::clear(&mut conn, ping.id, TODAY).unwrap();
    assert_eq!(cleared.cleared_on, Some(TODAY));
    let again = pings::clear(&mut conn, ping.id, TODAY + Duration::days(3)).unwrap();
    assert_eq!(again.cleared_on, Some(TODAY));
    assert!(pings::live_for(&conn, p.id).unwrap().is_none());
    assert!(live(&conn).is_empty());
    let fresh = make(&mut conn, on(&p), "y", 1);
    assert_ne!(fresh.id, ping.id);
    // The cleared one can't come back while the item has another.
    assert!(pings::reopen(&mut conn, ping.id, TODAY).is_err());
    assert_eq!(pings::list_all(&conn).unwrap().len(), 2);
}

#[test]
fn a_ping_hides_with_its_archived_item_and_returns_with_it() {
    let mut conn = open_in_memory().unwrap();
    let p = project(&mut conn, "EU");
    make(&mut conn, on(&p), "x", 0);
    assert_eq!(live(&conn).len(), 1);
    nodes::archive(&mut conn, on(&p)).unwrap();
    assert!(live(&conn).is_empty());
    assert_eq!(pings::list_all(&conn).unwrap().len(), 1);
    nodes::unarchive(&mut conn, on(&p)).unwrap();
    assert_eq!(live(&conn).len(), 1);
}

#[test]
fn hard_deleting_an_item_deletes_its_pings_and_leaves_tombstones() {
    let mut conn = open_in_memory().unwrap();
    let p = project(&mut conn, "EU");
    let ping = make(&mut conn, on(&p), "x", 0);
    nodes::archive(&mut conn, on(&p)).unwrap();
    nodes::delete(&mut conn, on(&p)).unwrap();
    assert!(pings::list_all(&conn).unwrap().is_empty());
    let tombstones: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tombstones WHERE kind = 'ping' AND id = ?1",
            [ping.id.to_string()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(tombstones, 1);
}

#[test]
fn a_ping_does_not_touch_its_item() {
    let mut conn = open_in_memory().unwrap();
    let p = project(&mut conn, "EU");
    let before = projects::get(&conn, p.id).unwrap();
    make(&mut conn, on(&p), "x", 0);
    assert_eq!(projects::get(&conn, p.id).unwrap(), before);
}

#[test]
fn every_write_is_logged_on_the_item_and_a_no_op_writes_nothing() {
    let mut conn = open_in_memory().unwrap();
    let p = project(&mut conn, "EU");
    let ping = make(&mut conn, on(&p), "x", 0);
    let rows = wrote(&mut conn, |c| {
        pings::update(c, ping.id, TODAY, UpdatePing::default()).unwrap();
    });
    assert!(rows.is_empty());
    let rows = wrote(&mut conn, |c| {
        pings::update(
            c,
            ping.id,
            TODAY,
            UpdatePing {
                note: Some("z".into()),
                ..Default::default()
            },
        )
        .unwrap();
    });
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].node_id, p.id);
    assert_eq!(rows[0].action, ActivityAction::Updated);
    assert!(rows[0].diff.get("ping").is_some());
    // Changing only the note of a ping whose day has passed is fine.
    let later = TODAY + Duration::days(9);
    pings::update(
        &mut conn,
        ping.id,
        later,
        UpdatePing {
            note: Some("old".into()),
            ..Default::default()
        },
    )
    .unwrap();
}

#[test]
fn undo_and_redo_are_exact_for_create_update_clear_and_archive() {
    let mut conn = open_in_memory().unwrap();
    let p = project(&mut conn, "EU");
    let snapshot = |c: &Connection| pings::list_all(c).unwrap();
    let cycle = |conn: &mut Connection, op: &dyn Fn(&mut Connection)| {
        let before = snapshot(conn);
        let rows = wrote(conn, |c| op(c));
        let after = snapshot(conn);
        assert_ne!(strip(&before), strip(&after));
        let undone = undo(conn, &rows);
        assert_eq!(strip(&snapshot(conn)), strip(&before));
        let redone = undo(conn, &undone);
        assert_eq!(strip(&snapshot(conn)), strip(&after));
        undo(conn, &redone);
        assert_eq!(strip(&snapshot(conn)), strip(&before));
    };
    // The comparison ignores the bookkeeping times, which a restore moves forward, and removed
    // pings (undoing a make removes it, as undoing a create archives a node).
    fn strip(p: &[Ping]) -> Vec<(Uuid, String, time::Date, Option<time::Date>)> {
        p.iter()
            .filter(|p| p.archived_at.is_none())
            .map(|p| (p.id, p.note.clone(), p.on_date, p.cleared_on))
            .collect()
    }
    let target = on(&p);
    cycle(&mut conn, &|c| {
        make(c, target, "first", 2);
    });
    // The cycle ends with the first ping removed again; make the one the rest work on.
    let id = make(&mut conn, target, "first", 2).id;
    cycle(&mut conn, &|c| {
        make(c, target, "changed", 4);
    });
    cycle(&mut conn, &|c| {
        pings::clear(c, id, TODAY).unwrap();
    });
    cycle(&mut conn, &|c| {
        pings::archive(c, id).unwrap();
    });
}

#[test]
fn undo_is_refused_when_the_ping_changed_since() {
    let mut conn = open_in_memory().unwrap();
    let p = project(&mut conn, "EU");
    let rows = wrote(&mut conn, |c| {
        make(c, on(&p), "a", 1);
    });
    make(&mut conn, on(&p), "b", 2);
    let Plan::Steps(steps) = plan(&rows) else {
        panic!()
    };
    assert!(undo::apply(&mut conn, &steps).is_err());
    assert_eq!(live(&conn).len(), 1);
}

#[test]
fn the_export_has_every_ping_cleared_ones_too() {
    let mut conn = open_in_memory().unwrap();
    let p = project(&mut conn, "EU");
    let ping = make(&mut conn, on(&p), "a", 0);
    pings::clear(&mut conn, ping.id, TODAY).unwrap();
    make(&mut conn, on(&p), "b", 1);
    let data = export::collect(&conn).unwrap();
    assert_eq!(data.pings.len(), 2);
    assert_eq!(data.counts()["pings"], 2);
}

#[test]
fn quick_add_commits_a_ping_and_the_directory_lists_decisions() {
    let mut conn = open_in_memory().unwrap();
    let p = project(&mut conn, "EU");
    let d = decisions::create(
        &mut conn,
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
    let dir = quick_add::directory(&conn).unwrap();
    assert!(dir.iter().any(|e| e.node.id == d.id));
    let result = quick_add::commit(
        &mut conn,
        QuickPlan {
            new_nodes: Vec::new(),
            main: QuickMain::Ping {
                target: Ref::Existing(on(&p)),
                note: "look".into(),
                on_date: today(),
            },
        },
    )
    .unwrap();
    assert_eq!(result.node.node.id, p.id);
    assert_eq!(pings::live_for(&conn, p.id).unwrap().unwrap().note, "look");
}
