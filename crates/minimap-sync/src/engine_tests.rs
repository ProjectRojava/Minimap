//! Two simulated devices syncing through a folder, driving the whole engine.

use std::{
    fs,
    path::PathBuf,
    sync::{atomic::AtomicU32, Mutex},
};

use minimap_store::{attachments, nodes, people, security::Key, snapshot, tasks};
use minimap_types::{AssigneeChoice, CreateTask, NodeRef, NodeType, SyncState, UpdateTask};
use uuid::Uuid;

use super::*;
use crate::folder::{temp_dir, FolderRemote};

struct TestHost {
    conn: Mutex<Connection>,
    dir: PathBuf,
    changed: AtomicU32,
}

impl Host for TestHost {
    fn with_db(&self, f: &mut dyn FnMut(&mut Connection, &Key)) -> Result<()> {
        let mut conn = self.conn.lock().map_err(|_| SyncError::Locked)?;
        f(&mut conn, &Key::None);
        Ok(())
    }

    fn data_dir(&self) -> PathBuf {
        self.dir.clone()
    }

    fn data_changed(&self) {
        self.changed
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

struct Device {
    host: Arc<TestHost>,
    engine: Arc<Engine>,
    id: String,
}

fn device(root: &std::path::Path, name: &str, with_me: bool) -> Device {
    let dir = root.join(name);
    fs::create_dir_all(&dir).unwrap();
    let mut conn = minimap_store::open(&dir.join("minimap.db")).unwrap();
    if with_me {
        people::ensure_self(&mut conn, "Me").unwrap();
    }
    meta::set(&conn, meta::DEVICE_NAME, name).unwrap();
    let id = meta::device_id(&conn).unwrap();
    let host = Arc::new(TestHost {
        conn: Mutex::new(conn),
        dir,
        changed: AtomicU32::new(0),
    });
    let engine = Engine::new(host.clone());
    engine.load().unwrap();
    Device { host, engine, id }
}

impl Device {
    /// A command that writes, as the app runs one: the engine hears about it.
    fn write<T>(&self, f: impl FnOnce(&mut Connection) -> T) -> T {
        let mut conn = self.host.conn.lock().unwrap();
        let out = f(&mut conn);
        self.engine.note_change(&conn);
        out
    }

    fn read<T>(&self, f: impl FnOnce(&Connection) -> T) -> T {
        f(&self.host.conn.lock().unwrap())
    }

    fn task(&self, title: &str) -> Uuid {
        self.write(|c| {
            tasks::create(
                c,
                CreateTask {
                    links: Vec::new(),
                    task_type: None,
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
        })
    }

    fn titles(&self) -> Vec<String> {
        let mut v: Vec<String> = self.read(|c| {
            tasks::list(c, false)
                .unwrap()
                .into_iter()
                .map(|t| t.title)
                .collect()
        });
        v.sort();
        v
    }

    fn status(&self) -> SyncStatus {
        self.engine.status(None)
    }

    /// The remote's record of this device's saves.
    fn saves(&self, remote: &FolderRemote) -> u64 {
        remote
            .list_devices()
            .unwrap()
            .into_iter()
            .find(|d| d.device_id == self.id)
            .map_or(0, |d| d.seq)
    }

    /// Lets time pass and runs a tick.
    fn pass(&self, secs: u64) {
        self.engine.advance(Duration::from_secs(secs));
        self.engine.tick();
    }
}

fn shared_remote(root: &std::path::Path) -> Arc<FolderRemote> {
    Arc::new(FolderRemote::new(root.join("drive")))
}

/// A has data and has started a new Drive; B is a fresh install that joined it.
fn two_devices(root: &std::path::Path) -> (Device, Device, Arc<FolderRemote>, RawKey) {
    let remote = shared_remote(root);
    let a = device(root, "Laptop", true);
    a.task("First task");
    a.task("Second task");
    let key = a.engine.start_new(remote.clone()).unwrap();
    let b = device(root, "Desktop", true);
    let outcome = b.engine.join_existing(remote.clone(), key.clone()).unwrap();
    assert_eq!(
        outcome,
        ConnectOutcome::Joined {
            adopted: true,
            devices: 1
        }
    );
    (a, b, remote, key)
}

#[test]
fn a_new_drive_gets_the_data_a_vault_file_and_nothing_readable() {
    let dir = temp_dir("engine-new");
    let remote = shared_remote(&dir);
    let a = device(&dir, "Laptop", true);
    a.task("Confidential roadmap");
    assert_eq!(Engine::probe(&*remote).unwrap(), Probe::Empty);
    assert_eq!(a.status().state, SyncState::LocalOnly);

    let key = a.engine.start_new(remote.clone()).unwrap();
    assert_eq!(
        Engine::probe(&*remote).unwrap(),
        Probe::Existing { devices: 1 }
    );
    vault::verify(&remote.read_vault().unwrap().unwrap(), &key).unwrap();
    // The snapshot on "Drive" is encrypted: no SQLite header, no text.
    let snapshot_file = fs::read(
        remote
            .root()
            .join("devices")
            .join(format!("{}.db.enc", a.id)),
    )
    .unwrap();
    assert!(!snapshot_file.starts_with(b"SQLite format 3"));
    assert!(!snapshot_file.windows(12).any(|w| w == b"Confidential"));
    let status = a.status();
    assert_eq!(status.state, SyncState::Saved);
    assert!(status.connected && !status.unsaved_changes);
    assert!(
        status.summary.starts_with("Saved to Drive"),
        "{}",
        status.summary
    );
    assert_eq!(status.account.as_deref(), Some("test@example.com"));
    assert_eq!(a.saves(&remote), 1);
    // The vault key is on this device and is not among the Drive files in the clear.
    let hex = key.to_hex();
    for entry in walk(remote.root()) {
        let bytes = fs::read(&entry).unwrap();
        assert!(
            !bytes.windows(64).any(|w| w == hex.as_bytes()),
            "key text in {entry:?}"
        );
    }
    fs::remove_dir_all(&dir).unwrap();
}

fn walk(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in fs::read_dir(dir).unwrap().flatten() {
        if e.path().is_dir() {
            out.extend(walk(&e.path()));
        } else {
            out.push(e.path());
        }
    }
    out
}

#[test]
fn a_second_device_adopts_the_data_with_the_recovery_key_and_a_wrong_key_changes_nothing() {
    let dir = temp_dir("engine-join");
    let remote = shared_remote(&dir);
    let a = device(&dir, "Laptop", true);
    a.task("Shared one");
    let key = a.engine.start_new(remote.clone()).unwrap();

    let b = device(&dir, "Desktop", true);
    let before = b.titles();
    let err = b
        .engine
        .join_existing(remote.clone(), RawKey::generate().unwrap())
        .unwrap_err();
    assert!(matches!(err, SyncError::WrongKey), "{err:?}");
    assert_eq!(b.titles(), before);
    assert!(!b.engine.is_connected());
    assert!(b.read(|c| meta::vault_key(c).unwrap()).is_none());

    let outcome = b.engine.join_existing(remote.clone(), key.clone()).unwrap();
    assert!(matches!(
        outcome,
        ConnectOutcome::Joined { adopted: true, .. }
    ));
    assert_eq!(b.titles(), vec!["Shared one"]);
    assert!(b.read(|c| meta::vault_key(c).unwrap().unwrap()) == key);
    assert_eq!(b.status().state, SyncState::Saved);
    // B is on Drive now too, under its own name.
    let names: Vec<String> = remote
        .list_devices()
        .unwrap()
        .into_iter()
        .map(|d| d.device_name)
        .collect();
    assert!(names.contains(&"Desktop".to_owned()) && names.contains(&"Laptop".to_owned()));
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_device_with_its_own_data_merges_instead_of_adopting() {
    let dir = temp_dir("engine-merge-join");
    let remote = shared_remote(&dir);
    let a = device(&dir, "Laptop", true);
    a.task("From the laptop");
    let key = a.engine.start_new(remote.clone()).unwrap();
    let b = device(&dir, "Desktop", true);
    b.task("From the desktop");
    let outcome = b.engine.join_existing(remote.clone(), key).unwrap();
    assert!(matches!(
        outcome,
        ConnectOutcome::Joined { adopted: false, .. }
    ));
    assert_eq!(b.titles(), vec!["From the desktop", "From the laptop"]);
    // A safety backup was taken before the first merge.
    let backups = minimap_store::backup::list(&b.host.dir.join("backups")).unwrap();
    assert!(
        backups.iter().any(|e| e.kind == BackupKind::PreSync),
        "{backups:?}"
    );
    // And A learns of the desktop's task.
    a.pass(20);
    assert_eq!(a.titles(), vec!["From the desktop", "From the laptop"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_change_on_one_device_reaches_the_other_and_then_everything_goes_quiet() {
    let dir = temp_dir("engine-flow");
    let (a, b, remote, _key) = two_devices(&dir);
    a.task("Made on the laptop");
    assert!(a.status().unsaved_changes);
    a.pass(6); // idle long enough: saved
    assert_eq!(a.saves(&remote), 2);
    assert!(!a.status().unsaved_changes);

    let revision = b.status().data_revision;
    b.pass(16); // the next look at the remote
    assert_eq!(
        b.titles(),
        vec!["First task", "Made on the laptop", "Second task"]
    );
    assert!(
        b.status().data_revision > revision,
        "screens are told to reload"
    );
    assert!(b.host.changed.load(std::sync::atomic::Ordering::SeqCst) >= 1);
    assert_eq!(b.status().last_merge.unwrap().from_device, "Laptop");

    // B saves the merged result (once), A looks at it, and then nothing more happens: no
    // endless ping-pong of saves.
    b.pass(6);
    let (sa, sb) = (a.saves(&remote), b.saves(&remote));
    for _ in 0..6 {
        a.pass(16);
        b.pass(16);
    }
    assert_eq!(a.titles(), b.titles());
    assert_eq!(a.saves(&remote), sa, "A had nothing new to save");
    assert!(b.saves(&remote) <= sb + 1);
    assert!(!a.status().unsaved_changes && !b.status().unsaved_changes);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn saving_waits_for_a_quiet_moment_but_never_longer_than_thirty_seconds() {
    let dir = temp_dir("engine-debounce");
    let (a, _b, remote, _key) = two_devices(&dir);
    let start = a.saves(&remote);
    a.task("one");
    a.pass(3);
    assert_eq!(a.saves(&remote), start, "still typing");
    a.pass(3);
    assert_eq!(a.saves(&remote), start + 1, "5 s without a change: saved");

    // Changes every 4 s never leave a 5 s gap, but the 30 s cap still saves.
    let before = a.saves(&remote);
    let mut saved_after = None;
    for step in 1..=12u64 {
        a.task(&format!("typing {step}"));
        a.pass(4);
        if a.saves(&remote) > before && saved_after.is_none() {
            saved_after = Some(step * 4);
        }
    }
    let at = saved_after.expect("saved during steady typing");
    assert!((28..=36).contains(&at), "saved after {at} s");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn edits_made_offline_on_both_devices_merge_when_they_reconnect() {
    let dir = temp_dir("engine-offline");
    let (a, b, remote, _key) = two_devices(&dir);
    remote.set_offline(true);
    let on_b = a.read(|c| tasks::list(c, false).unwrap()[0].id);
    a.task("Laptop, offline");
    b.task("Desktop, offline");
    a.write(|c| {
        tasks::update(
            c,
            on_b,
            UpdateTask {
                title: Some("Renamed on the laptop".into()),
                ..Default::default()
            },
        )
        .unwrap();
    });
    a.pass(6);
    b.pass(6);
    for d in [&a, &b] {
        let s = d.status();
        assert_eq!(s.state, SyncState::Offline);
        assert_eq!(s.summary, "Offline · changes waiting");
        assert!(s.unsaved_changes);
    }

    remote.set_offline(false);
    // Retries back off 5 s, 30 s, 2 min, then every 10 min; a long wait covers them.
    for _ in 0..6 {
        a.pass(700);
        b.pass(700);
        a.pass(20);
        b.pass(20);
    }
    let want = vec![
        "Desktop, offline",
        "Laptop, offline",
        "Renamed on the laptop",
        "Second task",
    ];
    assert_eq!(a.titles(), want);
    assert_eq!(b.titles(), want);
    assert_eq!(a.status().state, SyncState::Saved);
    assert_eq!(b.status().state, SyncState::Saved);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn an_interrupted_upload_keeps_the_old_snapshot_and_the_next_try_succeeds() {
    let dir = temp_dir("engine-interrupt");
    let (a, b, remote, _key) = two_devices(&dir);
    let before = a.saves(&remote);
    a.task("Half-sent");
    remote.interrupt_next_uploads(1);
    a.pass(6);
    assert!(a.status().last_error.is_some());
    assert!(a.status().unsaved_changes, "still marked unsaved");
    assert_eq!(
        a.saves(&remote),
        before,
        "the remote still has the complete old snapshot"
    );
    // The old snapshot is intact and usable by the other device.
    b.pass(16);
    assert!(!b.titles().contains(&"Half-sent".to_owned()));
    assert!(b.status().last_error.is_none());

    a.pass(10); // past the first back-off
    assert_eq!(a.saves(&remote), before + 1);
    assert!(a.status().last_error.is_none() && !a.status().unsaved_changes);
    b.pass(16);
    assert!(b.titles().contains(&"Half-sent".to_owned()));
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn unsaved_changes_survive_a_restart_and_go_up_at_the_next_start() {
    let dir = temp_dir("engine-restart");
    let (a, _b, remote, _key) = two_devices(&dir);
    let before = a.saves(&remote);
    a.task("Written just before quitting");
    // The app is closed before the idle timer fires. (Dropping the engine = quitting.)
    drop(a.engine.clone());
    let again = Engine::new(a.host.clone());
    again.load().unwrap();
    again.set_remote(Some(remote.clone()));
    again.load().unwrap();
    assert!(
        again.status(None).unsaved_changes,
        "remembered across the restart"
    );
    again.advance(Duration::from_secs(6));
    again.tick();
    assert_eq!(a.saves(&remote), before + 1);
    // Quitting with changes saves them straight away.
    let c = a.task("Last words");
    let _ = c;
    again.note_change(&a.host.conn.lock().unwrap());
    again.flush();
    assert_eq!(a.saves(&remote), before + 2);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_wrong_vault_key_stops_syncing_and_asks_for_the_user() {
    let dir = temp_dir("engine-wrongkey");
    let (a, b, remote, _key) = two_devices(&dir);
    a.task("New");
    a.pass(6);
    // B's key no longer matches (for example it was replaced by mistake).
    b.read(|c| meta::set_vault_key(c, &RawKey::generate().unwrap()).unwrap());
    b.pass(16);
    let s = b.status();
    assert_eq!(s.state, SyncState::NeedsAttention);
    assert!(!b.titles().contains(&"New".to_owned()));
    // It doesn't keep hammering Drive: further ticks do nothing until the user acts.
    let listed_before = remote.list_devices().unwrap().len();
    for _ in 0..3 {
        b.pass(60);
    }
    assert_eq!(b.status().state, SyncState::NeedsAttention);
    assert_eq!(remote.list_devices().unwrap().len(), listed_before);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_snapshot_from_a_newer_version_is_skipped_with_a_warning() {
    let dir = temp_dir("engine-newer");
    let (a, _b, remote, key) = two_devices(&dir);
    // A device running a future version saved something.
    let future = minimap_store::open_in_memory().unwrap();
    let file = dir.join("future.db.enc");
    snapshot::create(&future, &file, &key).unwrap();
    minimap_store::security::connect_for_tests(&file, &Key::Raw(key.clone()))
        .unwrap()
        .pragma_update(None, "user_version", minimap_store::LATEST_SCHEMA + 1)
        .unwrap();
    remote
        .upload_device(
            &DeviceMeta {
                device_id: "dddddddd-0000-0000-0000-000000000004".into(),
                device_name: "Future laptop".into(),
                saved_at: "2999-01-01T00:00:00.000Z".into(),
                seq: 1,
            },
            &file,
        )
        .unwrap();
    let before = a.titles();
    a.pass(16);
    let s = a.status();
    assert_eq!(a.titles(), before);
    assert!(
        s.warning.unwrap().contains("Future laptop"),
        "named in the warning"
    );
    assert_ne!(s.state, SyncState::Error);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn deleting_an_item_for_good_deletes_it_on_the_other_device() {
    let dir = temp_dir("engine-delete");
    let (a, b, _remote, _key) = two_devices(&dir);
    let id = a.read(|c| tasks::list(c, false).unwrap()[0].id);
    a.write(|c| {
        nodes::archive(c, NodeRef::new(NodeType::Task, id)).unwrap();
        nodes::delete(c, NodeRef::new(NodeType::Task, id)).unwrap();
    });
    a.pass(6);
    b.pass(16);
    assert_eq!(b.titles(), vec!["Second task"]);
    b.pass(6);
    a.pass(16);
    assert_eq!(a.titles(), vec!["Second task"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn checkpoints_are_kept_hourly_pruned_and_only_this_devices_are_touched() {
    let dir = temp_dir("engine-checkpoints");
    let (a, _b, remote, _key) = two_devices(&dir);
    // Another device's checkpoint is on the remote.
    let other = "eeeeeeee-0000-0000-0000-000000000005";
    let theirs = checkpoint_name(other, time::macros::datetime!(2020-01-01 00:00 UTC));
    let f = dir.join("theirs");
    fs::write(&f, b"x").unwrap();
    remote.upload_checkpoint(&theirs, &f).unwrap();

    // Sixty hours of hourly saves.
    for i in 0..60 {
        a.task(&format!("hour {i}"));
        a.pass(3600 + 10);
    }
    let all = remote.list_checkpoints().unwrap();
    let mine: Vec<&CheckpointFile> = all.iter().filter(|c| c.name.starts_with(&a.id)).collect();
    assert!(
        (24..=27).contains(&mine.len()),
        "24 hourly plus a few older days: {}",
        mine.len()
    );
    assert!(
        all.iter().any(|c| c.name == theirs),
        "someone else's is never deleted"
    );
    let status = a.status();
    assert!(!status.checkpoints.is_empty());
    assert_eq!(status.checkpoints[0].device_name, "Laptop");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn recovering_from_a_checkpoint_brings_a_deleted_item_back_and_tells_the_others() {
    let dir = temp_dir("engine-recover");
    let (a, b, remote, _key) = two_devices(&dir);
    a.pass(3600 + 10); // a checkpoint exists with both tasks
    let victim = a.read(|c| tasks::list(c, false).unwrap()[0].id);
    a.write(|c| {
        nodes::archive(c, NodeRef::new(NodeType::Task, victim)).unwrap();
        nodes::delete(c, NodeRef::new(NodeType::Task, victim)).unwrap();
    });
    a.pass(6);
    b.pass(16);
    assert_eq!(b.titles(), vec!["Second task"]);

    let checkpoints = a.engine.list_checkpoints().unwrap();
    assert!(!checkpoints.is_empty());
    let oldest = checkpoints.last().unwrap().name.clone();
    std::thread::sleep(Duration::from_millis(10));
    let result = a.engine.recover(&oldest).unwrap();
    assert_eq!(result.restored, 1);
    assert_eq!(a.titles(), vec!["First task", "Second task"]);
    let backups = minimap_store::backup::list(&a.host.dir.join("backups")).unwrap();
    assert!(backups.iter().any(|e| e.kind == BackupKind::PreRecover));
    a.pass(6);
    b.pass(16);
    assert_eq!(b.titles(), vec!["First task", "Second task"]);
    let _ = remote;
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn attachments_travel_encrypted_and_are_fetched_when_first_opened() {
    let dir = temp_dir("engine-media");
    let (a, b, remote, key) = two_devices(&dir);
    let task = a.read(|c| tasks::list(c, false).unwrap()[0].id);
    let src = dir.join("budget.xlsx");
    let content = b"confidential spreadsheet bytes ".repeat(5000);
    fs::write(&src, &content).unwrap();
    let sealed = a.engine.media_cache().add_file(&key, &src).unwrap();
    a.write(|c| {
        attachments::add(
            c,
            NodeRef::new(NodeType::Task, task),
            "budget.xlsx",
            sealed.plain_bytes,
            &sealed.sha256,
        )
        .unwrap();
    });
    a.pass(6);
    let on_drive = remote.list_media().unwrap();
    assert_eq!(on_drive.len(), 1);
    assert_eq!(on_drive[0].sha256, sealed.sha256);
    let blob = fs::read(
        remote
            .root()
            .join("media")
            .join(format!("{}.bin", sealed.sha256)),
    )
    .unwrap();
    assert!(
        !blob.windows(12).any(|w| w == b"confidential"),
        "encrypted on Drive"
    );
    assert_eq!(a.status().media_waiting, 0);

    // B learns of the attachment through the merge, but not the bytes.
    b.pass(16);
    let rows = b.read(|c| attachments::list_for_node(c, task, false).unwrap());
    assert_eq!(rows.len(), 1);
    assert!(!b.engine.media_cache().has(&sealed.sha256));
    let path = b.engine.fetch_media(&sealed.sha256).unwrap();
    assert!(path.is_file());
    let plain = b
        .engine
        .media_cache()
        .read_plain(&key, &sealed.sha256)
        .unwrap();
    assert_eq!(plain, content);

    // A damaged blob on Drive is refused and not kept.
    let (c_dir, d_dir) = (dir.join("c"), dir.join("d"));
    let _ = (c_dir, d_dir);
    let other = a
        .engine
        .media_cache()
        .add_bytes(&key, b"another file")
        .unwrap();
    a.write(|c| {
        attachments::add(
            c,
            NodeRef::new(NodeType::Task, task),
            "other.png",
            other.plain_bytes,
            &other.sha256,
        )
        .unwrap();
    });
    a.pass(6);
    let remote_blob = remote
        .root()
        .join("media")
        .join(format!("{}.bin", other.sha256));
    let mut bytes = fs::read(&remote_blob).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    fs::write(&remote_blob, bytes).unwrap();
    assert!(matches!(
        b.engine.fetch_media(&other.sha256),
        Err(SyncError::Corrupt(_))
    ));
    assert!(!b.engine.media_cache().has(&other.sha256));
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn files_nothing_refers_to_are_removed_from_drive_only_after_the_grace_period() {
    let dir = temp_dir("engine-orphans");
    let (a, _b, remote, key) = two_devices(&dir);
    let task = a.read(|c| tasks::list(c, false).unwrap()[0].id);
    let sealed = a
        .engine
        .media_cache()
        .add_bytes(&key, b"short lived")
        .unwrap();
    let att = a.write(|c| {
        attachments::add(
            c,
            NodeRef::new(NodeType::Task, task),
            "a.png",
            sealed.plain_bytes,
            &sealed.sha256,
        )
        .unwrap()
    });
    a.pass(6);
    assert_eq!(remote.list_media().unwrap().len(), 1);
    a.write(|c| attachments::remove(c, att.id).unwrap());
    a.pass(120);
    assert_eq!(
        remote.list_media().unwrap().len(),
        1,
        "a device that was away may still need it"
    );
    a.pass(8 * 24 * 3600);
    a.pass(120);
    assert!(remote.list_media().unwrap().is_empty());
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn syncing_now_does_everything_at_once_and_a_disconnected_device_is_local_only() {
    let dir = temp_dir("engine-now");
    let (a, b, remote, _key) = two_devices(&dir);
    a.task("Right now");
    a.engine.sync_now().unwrap();
    b.engine.sync_now().unwrap();
    assert!(b.titles().contains(&"Right now".to_owned()));
    // Disconnecting leaves the data and stops tracking.
    b.engine.set_remote(None);
    assert_eq!(b.status().state, SyncState::LocalOnly);
    b.task("Only here");
    assert!(!b.status().unsaved_changes);
    assert!(matches!(b.engine.sync_now(), Err(SyncError::NotConnected)));
    a.pass(16);
    assert!(!a.titles().contains(&"Only here".to_owned()));
    let _ = remote;
    fs::remove_dir_all(&dir).unwrap();
}
