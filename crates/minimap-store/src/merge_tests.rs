use minimap_types::{
    AssigneeChoice, CreatePerson, CreateProject, CreateTask, CreateTeam, EdgeType, NewEdge,
    NodeRef, NodeType, Patch, UpdateSettings, UpdateTask,
};
use proptest::prelude::*;
use rusqlite::Connection;
use uuid::Uuid;

use super::*;
use crate::{edges, nodes, people, projects, settings, tasks, teams};

fn device() -> Connection {
    crate::open_in_memory().unwrap()
}

fn task(conn: &mut Connection, title: &str) -> Uuid {
    tasks::create(
        conn,
        CreateTask {
            links: Vec::new(),
            task_type: None,
            focus: None,
            start_minute: None,
            length_minutes: None,
            title: title.into(),
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
    .unwrap()
    .id
}

fn person(conn: &mut Connection, name: &str, is_self: bool) -> Uuid {
    people::create(
        conn,
        CreatePerson {
            name: name.into(),
            role_title: String::new(),
            email: None,
            weekly_capacity_hours: None,
            is_self,
            notes: String::new(),
        },
    )
    .unwrap()
    .id
}

fn project(conn: &mut Connection, title: &str) -> Uuid {
    projects::create(
        conn,
        CreateProject {
            title: title.into(),
            slug: None,
            description: String::new(),
            owner_person_id: None,
            start_date: None,
            target_date: None,
            status: None,
            priority: None,
        },
    )
    .unwrap()
    .id
}

fn team(conn: &mut Connection, name: &str, parent: Option<Uuid>) -> Uuid {
    teams::create(
        conn,
        CreateTeam {
            name: name.into(),
            description: String::new(),
            parent_team_id: parent,
        },
    )
    .unwrap()
    .id
}

fn link(conn: &mut Connection, kind: EdgeType, from: NodeRef, to: NodeRef) {
    edges::add(
        conn,
        NewEdge {
            edge_type: kind,
            from,
            to,
            attrs: serde_json::json!({}),
        },
    )
    .unwrap();
}

fn t(id: Uuid) -> NodeRef {
    NodeRef::new(NodeType::Task, id)
}

fn retitle(conn: &mut Connection, id: Uuid, title: &str, stamp: &str) {
    tasks::update(
        conn,
        id,
        UpdateTask {
            title: Some(title.into()),
            ..Default::default()
        },
    )
    .unwrap();
    conn.execute(
        "UPDATE tasks SET updated_at = ?2 WHERE id = ?1",
        rusqlite::params![id.to_string(), stamp],
    )
    .unwrap();
}

fn m(into: &mut Connection, from: &Connection) -> MergeSummary {
    merge(into, from, "Other", MergeMode::Apply).unwrap()
}

/// Both directions, then once more so the first side sees the second's merged result.
fn sync(a: &mut Connection, b: &mut Connection) {
    m(a, b);
    m(b, a);
    m(a, b);
}

/// Everything that should be the same on two devices that have synced: every row of every
/// synced table (edges without their device-chosen ids; "me" is each device's own).
fn view(conn: &Connection) -> String {
    let mut out = String::new();
    for (table, skip) in [
        ("objectives", ""),
        ("projects", ""),
        ("tasks", ""),
        ("people", "is_self"),
        ("teams", ""),
        ("notes", ""),
        ("decisions", ""),
        ("waiting_on", ""),
        ("attachments", ""),
        ("edges", "id"),
        ("activity", ""),
        ("tombstones", ""),
    ] {
        let cols: Vec<String> = columns(conn, table)
            .unwrap()
            .into_iter()
            .filter(|c| c != skip)
            .collect();
        let rows = read_rows(conn, table, &cols).unwrap();
        let mut lines: Vec<String> = rows
            .iter()
            .map(|r| {
                cols.iter()
                    .map(|c| format!("{:?}", r[c]))
                    .collect::<Vec<_>>()
                    .join("|")
            })
            .collect();
        lines.sort();
        out.push_str(&format!("{table}:\n{}\n", lines.join("\n")));
    }
    let mut synced: Vec<String> = conn
        .prepare("SELECT key || '=' || value || '@' || updated_at FROM settings")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    synced.retain(|s| !LOCAL_ONLY.iter().any(|k| s.starts_with(&format!("{k}="))));
    synced.sort();
    out.push_str(&format!("settings:\n{}\n", synced.join("\n")));
    out
}

fn title_of(conn: &Connection, id: Uuid) -> String {
    tasks::get(conn, id).unwrap().title
}

#[test]
fn new_rows_are_added_and_a_second_merge_changes_nothing() {
    let (mut a, mut b) = (device(), device());
    task(&mut a, "Only on A");
    let on_b = task(&mut b, "Only on B");
    let first = m(&mut a, &b);
    assert_eq!(first.added, 1);
    assert!(first.changed_anything());
    assert_eq!(title_of(&a, on_b), "Only on B");
    assert_eq!(tasks::list(&a, false).unwrap().len(), 2);

    let again = m(&mut a, &b);
    assert!(!again.changed_anything(), "{again:?}");
    // The search index followed the merge.
    let hits = crate::search::run(&a, "\"B\"*", &[], false, 10).unwrap();
    assert_eq!(hits.len(), 1);
}

#[test]
fn the_later_write_wins_in_both_directions_and_the_devices_converge() {
    for (a_stamp, b_stamp, winner) in [
        (
            "2027-03-01T10:00:00.000Z",
            "2027-03-01T10:00:05.000Z",
            "From B",
        ),
        (
            "2027-03-01T10:00:09.000Z",
            "2027-03-01T10:00:05.000Z",
            "From A",
        ),
    ] {
        let (mut a, mut b) = (device(), device());
        let id = task(&mut a, "Original");
        m(&mut b, &a);
        retitle(&mut a, id, "From A", a_stamp);
        retitle(&mut b, id, "From B", b_stamp);
        sync(&mut a, &mut b);
        assert_eq!(title_of(&a, id), winner);
        assert_eq!(title_of(&b, id), winner);
        assert_eq!(view(&a), view(&b));
    }
}

#[test]
fn identical_timestamps_still_give_both_devices_the_same_winner() {
    let (mut a, mut b) = (device(), device());
    let id = task(&mut a, "Original");
    m(&mut b, &a);
    let stamp = "2027-03-01T10:00:00.000Z";
    retitle(&mut a, id, "Alpha", stamp);
    retitle(&mut b, id, "Beta", stamp);
    sync(&mut a, &mut b);
    assert_eq!(title_of(&a, id), title_of(&b, id));
    assert_eq!(view(&a), view(&b));
}

#[test]
fn work_on_different_items_is_all_kept() {
    let (mut a, mut b) = (device(), device());
    let one = task(&mut a, "One");
    let two = task(&mut a, "Two");
    m(&mut b, &a);
    retitle(&mut a, one, "One (A)", "2027-03-01T10:00:00.000Z");
    retitle(&mut b, two, "Two (B)", "2027-03-01T10:00:01.000Z");
    task(&mut b, "Three");
    sync(&mut a, &mut b);
    for conn in [&a, &b] {
        assert_eq!(title_of(conn, one), "One (A)");
        assert_eq!(title_of(conn, two), "Two (B)");
        assert_eq!(tasks::list(conn, false).unwrap().len(), 3);
    }
    assert_eq!(view(&a), view(&b));
}

#[test]
fn archiving_travels_and_a_later_unarchive_beats_it() {
    let (mut a, mut b) = (device(), device());
    let id = task(&mut a, "Soon gone");
    m(&mut b, &a);
    nodes::archive(&mut a, t(id)).unwrap();
    sync(&mut a, &mut b);
    assert!(tasks::get(&b, id).unwrap().archived_at.is_some());
    std::thread::sleep(std::time::Duration::from_millis(5));
    nodes::unarchive(&mut b, t(id)).unwrap();
    sync(&mut a, &mut b);
    assert!(tasks::get(&a, id).unwrap().archived_at.is_none());
    assert_eq!(view(&a), view(&b));
}

#[test]
fn a_deleted_item_stays_deleted_everywhere() {
    let (mut a, mut b) = (device(), device());
    let doomed = task(&mut a, "Doomed");
    let kept = task(&mut a, "Kept");
    m(&mut b, &a);
    nodes::archive(&mut b, t(doomed)).unwrap();
    nodes::delete(&mut b, t(doomed)).unwrap();

    let summary = m(&mut a, &b);
    assert_eq!(summary.deleted, 1);
    assert!(tasks::get(&a, doomed).is_err());
    assert_eq!(title_of(&a, kept), "Kept");
    // A still holds the old copy in its own history: merging A back never resurrects it.
    m(&mut b, &a);
    assert!(tasks::get(&b, doomed).is_err());
    // Even when A edited it after B deleted it (the device didn't know).
    let (mut c, mut d) = (device(), device());
    let id = task(&mut c, "Edited late");
    m(&mut d, &c);
    nodes::archive(&mut d, t(id)).unwrap();
    nodes::delete(&mut d, t(id)).unwrap();
    retitle(
        &mut c,
        id,
        "Edited after the delete",
        "2999-01-01T00:00:00.000Z",
    );
    sync(&mut c, &mut d);
    assert!(tasks::get(&c, id).is_err() && tasks::get(&d, id).is_err());
    assert_eq!(view(&c), view(&d));
}

#[test]
fn links_follow_their_items_and_the_same_link_made_twice_is_one_link() {
    let (mut a, mut b) = (device(), device());
    let (x, y) = (task(&mut a, "X"), task(&mut a, "Y"));
    m(&mut b, &a);
    link(&mut a, EdgeType::Blocks, t(x), t(y));
    link(&mut b, EdgeType::Blocks, t(x), t(y)); // the same link, made independently
    sync(&mut a, &mut b);
    for conn in [&a, &b] {
        let active = edges::list_active_of_type(conn, EdgeType::Blocks).unwrap();
        assert_eq!(active.len(), 1);
    }
    assert_eq!(view(&a), view(&b));
    // Removing it on one device removes it on both.
    let id = edges::list_active(&a).unwrap()[0].id;
    std::thread::sleep(std::time::Duration::from_millis(5));
    edges::remove(&mut a, id).unwrap();
    sync(&mut a, &mut b);
    assert!(edges::list_active_of_type(&b, EdgeType::Blocks)
        .unwrap()
        .is_empty());
    assert_eq!(view(&a), view(&b));
}

#[test]
fn loops_made_by_two_devices_together_are_broken_the_same_way_on_both() {
    let (mut a, mut b) = (device(), device());
    let (x, y) = (task(&mut a, "X"), task(&mut a, "Y"));
    m(&mut b, &a);
    link(&mut a, EdgeType::Blocks, t(x), t(y));
    std::thread::sleep(std::time::Duration::from_millis(5));
    link(&mut b, EdgeType::Blocks, t(y), t(x)); // newer; closes the loop with A's link
    let first = m(&mut a, &b);
    assert!(
        first.repairs.iter().any(|r| r.contains("loop")),
        "{:?}",
        first.repairs
    );
    sync(&mut a, &mut b);
    for conn in [&a, &b] {
        let active = edges::list_active_of_type(conn, EdgeType::Blocks).unwrap();
        assert_eq!(active.len(), 1, "one of the two links went");
        assert_eq!(active[0].from_id, x, "the older link stays");
    }
    assert_eq!(view(&a), view(&b));
}

#[test]
fn teams_put_inside_each_other_by_two_devices_are_untangled() {
    let (mut a, mut b) = (device(), device());
    let (one, two) = (team(&mut a, "One", None), team(&mut a, "Two", None));
    m(&mut b, &a);
    teams::update(
        &mut a,
        one,
        minimap_types::UpdateTeam {
            parent_team_id: Patch::Set(two),
            ..Default::default()
        },
    )
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    teams::update(
        &mut b,
        two,
        minimap_types::UpdateTeam {
            parent_team_id: Patch::Set(one),
            ..Default::default()
        },
    )
    .unwrap();
    sync(&mut a, &mut b);
    for conn in [&a, &b] {
        let parents: Vec<Option<Uuid>> = [one, two]
            .iter()
            .map(|id| teams::get(conn, *id).unwrap().parent_team_id)
            .collect();
        assert_eq!(
            parents,
            vec![Some(two), None],
            "the newer nesting was undone"
        );
    }
}

#[test]
fn two_projects_with_the_same_handle_end_up_with_different_ones_on_both_devices() {
    let (mut a, mut b) = (device(), device());
    let first = project(&mut a, "Launch");
    std::thread::sleep(std::time::Duration::from_millis(5));
    let second = project(&mut b, "Launch");
    assert_eq!(projects::get(&a, first).unwrap().slug, "launch");
    assert_eq!(projects::get(&b, second).unwrap().slug, "launch");
    let summary = m(&mut a, &b);
    let expected = format!("launch-{}", &second.simple().to_string()[26..]);
    assert!(
        summary.repairs.iter().any(|r| r.contains(&expected)),
        "{:?}",
        summary.repairs
    );
    sync(&mut a, &mut b);
    for conn in [&a, &b] {
        assert_eq!(
            projects::get(conn, first).unwrap().slug,
            "launch",
            "the older keeps it"
        );
        assert_eq!(projects::get(conn, second).unwrap().slug, expected);
    }
    assert!(minimap_core::slug::validate(&expected).is_ok());
    assert_eq!(view(&a), view(&b));
    // Settled: nothing left to repair or change.
    assert!(!m(&mut a, &b).changed_anything());
    assert!(!m(&mut b, &a).changed_anything());
}

#[test]
fn many_projects_with_the_same_handle_get_the_same_handles_whatever_the_order() {
    // Four projects with one handle, spread over three devices, merged in every order.
    let mut devices: Vec<Connection> = (0..3).map(|_| device()).collect();
    let mut ids = Vec::new();
    for (i, d) in [0usize, 1, 2, 1].iter().enumerate() {
        std::thread::sleep(std::time::Duration::from_millis(3));
        ids.push(project(&mut devices[*d], "Roadmap"));
        let _ = i;
    }
    let orders: [[usize; 3]; 3] = [[0, 1, 2], [2, 1, 0], [1, 2, 0]];
    let mut results = Vec::new();
    for order in orders {
        let mut target = device();
        for i in order {
            m(&mut target, &devices[i]);
        }
        let mut handles: Vec<String> = ids
            .iter()
            .map(|id| projects::get(&target, *id).unwrap().slug)
            .collect();
        handles.sort();
        results.push(handles);
    }
    assert_eq!(results[0], results[1]);
    assert_eq!(results[1], results[2]);
    let distinct: std::collections::HashSet<&String> = results[0].iter().collect();
    assert_eq!(
        distinct.len(),
        4,
        "every project has its own handle: {:?}",
        results[0]
    );
}

#[test]
fn each_device_keeps_its_own_me() {
    let (mut a, mut b) = (device(), device());
    let me_a = person(&mut a, "Me (laptop)", true);
    let me_b = person(&mut b, "Me (desktop)", true);
    let summary = m(&mut a, &b);
    assert!(summary
        .repairs
        .iter()
        .any(|r| r.contains("ordinary person")));
    sync(&mut a, &mut b);
    assert_eq!(people::get_self(&a).unwrap().unwrap().id, me_a);
    assert_eq!(people::get_self(&b).unwrap().unwrap().id, me_b);
    assert_eq!(people::list(&a, false).unwrap().len(), 2);
    assert_eq!(view(&a), view(&b));
}

#[test]
fn a_device_without_a_me_takes_the_one_that_arrives() {
    let (mut a, mut b) = (device(), device());
    let me_b = person(&mut b, "Me", true);
    m(&mut a, &b);
    assert_eq!(people::get_self(&a).unwrap().unwrap().id, me_b);
}

#[test]
fn work_settings_sync_but_per_device_ones_do_not() {
    let (mut a, mut b) = (device(), device());
    settings::update(
        &mut a,
        UpdateSettings {
            hours_per_day: Some(6.0),
            stale_waiting_days: Some(3),
            theme: Some("nord".into()),
            backup_folder: Some("/tmp/a-backups".into()),
            auto_backup: Some(false),
            ..Default::default()
        },
    )
    .unwrap();
    sync(&mut a, &mut b);
    let got = settings::get(&b).unwrap();
    assert_eq!(got.hours_per_day, 6.0);
    assert_eq!(got.stale_waiting_days, 3);
    assert_ne!(got.theme, "nord");
    assert_eq!(got.backup_folder, None);
    assert!(got.auto_backup);
    // A newer change on B wins; deleting a setting (the template reset) travels too.
    std::thread::sleep(std::time::Duration::from_millis(5));
    settings::update(
        &mut b,
        UpdateSettings {
            hours_per_day: Some(7.5),
            ..Default::default()
        },
    )
    .unwrap();
    sync(&mut a, &mut b);
    assert_eq!(settings::get(&a).unwrap().hours_per_day, 7.5);
    settings::update(
        &mut a,
        UpdateSettings {
            stale_waiting_days: Some(30),
            ..Default::default()
        },
    )
    .unwrap();
    sync(&mut a, &mut b);
    assert_eq!(settings::get(&b).unwrap().stale_waiting_days, 30);
    assert_eq!(view(&a), view(&b));
}

#[test]
fn things_pointing_at_a_deleted_item_are_repaired() {
    let (mut a, mut b) = (device(), device());
    person(&mut a, "Me", true);
    let p = project(&mut a, "Doomed project");
    let owner = person(&mut a, "Owner", false);
    let child = task(&mut a, "In the project");
    tasks::update(
        &mut a,
        child,
        UpdateTask {
            project_id: Patch::Set(p),
            ..Default::default()
        },
    )
    .unwrap();
    projects::update(
        &mut a,
        p,
        minimap_types::UpdateProject {
            owner_person_id: Patch::Set(owner),
            ..Default::default()
        },
    )
    .unwrap();
    link(
        &mut a,
        EdgeType::RelatesTo,
        t(child),
        NodeRef::new(NodeType::Project, p),
    );
    m(&mut b, &a);
    // B lets go of everything that used the project and the person, then deletes both for good.
    tasks::update(
        &mut b,
        child,
        UpdateTask {
            project_id: Patch::Clear,
            ..Default::default()
        },
    )
    .unwrap();
    projects::update(
        &mut b,
        p,
        minimap_types::UpdateProject {
            owner_person_id: Patch::Clear,
            ..Default::default()
        },
    )
    .unwrap();
    for node in [
        NodeRef::new(NodeType::Project, p),
        NodeRef::new(NodeType::Person, owner),
    ] {
        nodes::archive(&mut b, node).unwrap();
        nodes::delete(&mut b, node).unwrap();
    }
    // Meanwhile A, who hasn't heard, keeps working on the task: its row is newer and still
    // says it is in the project.
    retitle(
        &mut a,
        child,
        "Still in the project, as far as A knows",
        "2999-01-01T00:00:00.000Z",
    );

    let summary = m(&mut a, &b);
    assert_eq!(tasks::get(&a, child).unwrap().project_id, None);
    assert!(projects::get(&a, p).is_err());
    assert!(
        edges::list_active(&a).unwrap().iter().all(|e| e.to_id != p),
        "links to the deleted project are gone"
    );
    assert!(
        summary.repairs.iter().any(|r| r.contains("inbox")),
        "{:?}",
        summary.repairs
    );
    sync(&mut a, &mut b);
    assert_eq!(view(&a), view(&b));
    assert_eq!(tasks::get(&b, child).unwrap().project_id, None);
}

#[test]
fn a_preview_says_what_would_change_and_changes_nothing() {
    let (mut a, mut b) = (device(), device());
    task(&mut a, "Mine");
    task(&mut b, "Theirs");
    let before = view(&a);
    let preview = merge(&mut a, &b, "Other", MergeMode::Preview).unwrap();
    assert_eq!(preview.added, 1);
    assert_eq!(view(&a), before);
    assert!(meta::get(&a, meta::MERGING).unwrap().is_none());
    // Applying then does exactly that.
    let applied = merge(&mut a, &b, "Other", MergeMode::Apply).unwrap();
    assert_eq!(applied.added, 1);
    assert_ne!(view(&a), before);
}

#[test]
fn a_failed_merge_changes_nothing() {
    let (mut a, mut b) = (device(), device());
    task(&mut a, "Mine");
    task(&mut b, "Theirs");
    // Break the remote so the merge fails part-way: a task row pointing at a missing column
    // value that the local CHECK constraint refuses.
    b.execute("PRAGMA ignore_check_constraints = ON", [])
        .unwrap();
    b.execute("UPDATE tasks SET status = 'nonsense'", [])
        .unwrap();
    let before = view(&a);
    assert!(merge(&mut a, &b, "Other", MergeMode::Apply).is_err());
    assert_eq!(view(&a), before);
    assert!(meta::get(&a, meta::MERGING).unwrap().is_none());
}

#[test]
fn recovering_brings_back_old_versions_and_deleted_items_and_keeps_newer_ones() {
    let (mut live, mut old) = (device(), device());
    let changed = task(&mut live, "Original title");
    let deleted = task(&mut live, "Deleted later");
    let removed_link = task(&mut live, "Link target");
    m(&mut old, &live); // the checkpoint
    retitle(
        &mut live,
        changed,
        "Changed after the checkpoint",
        "2027-03-01T10:00:00.000Z",
    );
    nodes::archive(&mut live, t(deleted)).unwrap();
    nodes::delete(&mut live, t(deleted)).unwrap();
    let newer = task(&mut live, "Made after the checkpoint");
    let _ = removed_link;

    let result = merge(&mut live, &old, "Checkpoint", MergeMode::Recover).unwrap();
    assert_eq!(result.added, 1, "the deleted task is back");
    assert_eq!(result.updated, 1, "the changed one is as it was");
    assert_eq!(title_of(&live, changed), "Original title");
    assert_eq!(title_of(&live, deleted), "Deleted later");
    assert_eq!(title_of(&live, newer), "Made after the checkpoint");
    // The tombstone is lifted, and the recovered rows are newer than anything on other devices.
    let deleted_for_good: i64 = live
        .query_row(
            &format!("SELECT COUNT(*) FROM tombstones WHERE {ACTIVE}"),
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(deleted_for_good, 0);
    let stamp = tasks::get(&live, changed).unwrap().updated_at;
    assert!(stamp > time::OffsetDateTime::now_utc() - time::Duration::minutes(1));
}

#[test]
fn a_recovery_reaches_the_other_devices_and_a_later_delete_beats_it() {
    let (mut a, mut b, mut old) = (device(), device(), device());
    let id = task(&mut a, "Precious");
    sync(&mut a, &mut b);
    m(&mut old, &a); // a checkpoint taken while it existed
    nodes::archive(&mut a, t(id)).unwrap();
    nodes::delete(&mut a, t(id)).unwrap();
    sync(&mut a, &mut b);
    assert!(tasks::get(&b, id).is_err(), "deleted everywhere");

    // A recovers it from the checkpoint; the other device gets it back too.
    std::thread::sleep(std::time::Duration::from_millis(5));
    merge(&mut a, &old, "Checkpoint", MergeMode::Recover).unwrap();
    assert_eq!(title_of(&a, id), "Precious");
    sync(&mut a, &mut b);
    assert_eq!(title_of(&b, id), "Precious", "the recovery travelled");
    assert_eq!(view(&a), view(&b));

    // Deleting it again afterwards sticks, on both.
    std::thread::sleep(std::time::Duration::from_millis(5));
    nodes::archive(&mut b, t(id)).unwrap();
    nodes::delete(&mut b, t(id)).unwrap();
    sync(&mut a, &mut b);
    assert!(tasks::get(&a, id).is_err() && tasks::get(&b, id).is_err());
    assert_eq!(view(&a), view(&b));
}

// ------------------------------------------------------------------ convergence

#[derive(Debug, Clone)]
enum Op {
    New(bool, u8),
    Retitle(bool, u8, u8),
    Archive(bool, u8),
    Unarchive(bool, u8),
    Delete(bool, u8),
    Link(bool, u8, u8),
    Block(bool, u8, u8),
    NewProject(bool, u8),
    Setting(bool, u8),
    Sync,
}

fn op_strategy() -> impl Strategy<Value = Op> {
    all_ops().prop_filter("", |_| true)
}

/// Like [`op_strategy`] without loop-making links: which link of a loop is dropped depends on
/// what has been merged by then, so only the two-device convergence is promised for those.
fn ops_without_loops() -> impl Strategy<Value = Op> {
    all_ops().prop_filter("no blocks links", |op| !matches!(op, Op::Block(..)))
}

fn all_ops() -> impl Strategy<Value = Op> {
    prop_oneof![
        (any::<bool>(), any::<u8>()).prop_map(|(d, n)| Op::New(d, n)),
        (any::<bool>(), any::<u8>(), any::<u8>()).prop_map(|(d, i, n)| Op::Retitle(d, i, n)),
        (any::<bool>(), any::<u8>()).prop_map(|(d, i)| Op::Archive(d, i)),
        (any::<bool>(), any::<u8>()).prop_map(|(d, i)| Op::Unarchive(d, i)),
        (any::<bool>(), any::<u8>()).prop_map(|(d, i)| Op::Delete(d, i)),
        (any::<bool>(), any::<u8>(), any::<u8>()).prop_map(|(d, i, j)| Op::Link(d, i, j)),
        (any::<bool>(), any::<u8>(), any::<u8>()).prop_map(|(d, i, j)| Op::Block(d, i, j)),
        (any::<bool>(), 0u8..3).prop_map(|(d, k)| Op::NewProject(d, k)),
        (any::<bool>(), 1u8..24).prop_map(|(d, h)| Op::Setting(d, h)),
        Just(Op::Sync),
    ]
}

fn pick(conn: &Connection, i: u8) -> Option<Uuid> {
    let all = tasks::list(conn, true).unwrap();
    (!all.is_empty()).then(|| all[usize::from(i) % all.len()].id)
}

fn run(op: &Op, a: &mut Connection, b: &mut Connection) {
    let which = |first: &bool| if *first { 0 } else { 1 };
    let target: &mut Connection = match op {
        Op::New(d, _)
        | Op::Retitle(d, _, _)
        | Op::Archive(d, _)
        | Op::Unarchive(d, _)
        | Op::Delete(d, _)
        | Op::Link(d, _, _)
        | Op::Block(d, _, _)
        | Op::NewProject(d, _)
        | Op::Setting(d, _) => {
            if which(d) == 0 {
                a
            } else {
                b
            }
        }
        Op::Sync => {
            sync(a, b);
            return;
        }
    };
    // A 1 ms pause keeps "later" meaning later without making the test slow.
    std::thread::sleep(std::time::Duration::from_millis(1));
    match op {
        Op::New(_, n) => {
            task(target, &format!("task {n}"));
        }
        Op::Retitle(_, i, n) => {
            if let Some(id) = pick(target, *i) {
                let _ = tasks::update(
                    target,
                    id,
                    UpdateTask {
                        title: Some(format!("title {n}")),
                        ..Default::default()
                    },
                );
            }
        }
        Op::Archive(_, i) => {
            if let Some(id) = pick(target, *i) {
                let _ = nodes::archive(target, t(id));
            }
        }
        Op::Unarchive(_, i) => {
            if let Some(id) = pick(target, *i) {
                let _ = nodes::unarchive(target, t(id));
            }
        }
        Op::Delete(_, i) => {
            if let Some(id) = pick(target, *i) {
                let _ = nodes::archive(target, t(id));
                let _ = nodes::delete(target, t(id));
            }
        }
        Op::Link(_, i, j) => {
            if let (Some(x), Some(y)) = (pick(target, *i), pick(target, *j)) {
                if x != y {
                    let _ = edges::add(
                        target,
                        NewEdge {
                            edge_type: EdgeType::RelatesTo,
                            from: t(x),
                            to: t(y),
                            attrs: serde_json::json!({}),
                        },
                    );
                }
            }
        }
        Op::Block(_, i, j) => {
            // The store doesn't police loops (the command layer does), so these can make them.
            if let (Some(x), Some(y)) = (pick(target, *i), pick(target, *j)) {
                if x != y {
                    let _ = edges::add(
                        target,
                        NewEdge {
                            edge_type: EdgeType::Blocks,
                            from: t(x),
                            to: t(y),
                            attrs: serde_json::json!({}),
                        },
                    );
                }
            }
        }
        Op::NewProject(_, k) => {
            project(target, &format!("Project {k}"));
        }
        Op::Setting(_, h) => {
            let _ = settings::update(
                target,
                UpdateSettings {
                    hours_per_day: Some(f64::from(*h)),
                    ..Default::default()
                },
            );
        }
        Op::Sync => unreachable!(),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(40))]

    /// Whatever two devices did, in whatever order, a full sync leaves them with identical
    /// data, and merging again changes nothing.
    #[test]
    fn two_devices_always_converge(ops in prop::collection::vec(op_strategy(), 1..40)) {
        let (mut a, mut b) = (device(), device());
        for op in &ops {
            run(op, &mut a, &mut b);
        }
        sync(&mut a, &mut b);
        prop_assert_eq!(view(&a), view(&b));
        prop_assert!(!m(&mut a, &b).changed_anything());
        prop_assert!(!m(&mut b, &a).changed_anything());
    }

    /// Merging is independent of which device syncs first.
    #[test]
    fn the_order_of_syncing_does_not_matter(ops in prop::collection::vec(ops_without_loops(), 1..30)) {
        let (mut a1, mut b1) = (device(), device());
        for op in &ops {
            run(op, &mut a1, &mut b1);
        }
        // Copy the state of both devices: merge each into a clean device in both orders.
        let (mut x, mut y) = (device(), device());
        m(&mut x, &a1);
        m(&mut x, &b1);
        m(&mut y, &b1);
        m(&mut y, &a1);
        prop_assert_eq!(view(&x), view(&y));
    }
}
