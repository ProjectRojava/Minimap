//! The sync engine (spec 22, ADR-0011): decides when to pull other devices' snapshots and merge
//! them, when to save this device's snapshot, when to keep a checkpoint, and which attachment
//! files to upload. It never touches SQL: the database side is `minimap-store` (snapshots,
//! merging), reached through a [`Host`] that holds the connection's lock for each call.
//!
//! Network work (listing, downloading, uploading) always happens *outside* the database lock;
//! the lock is held only to make a snapshot copy or to apply a merge.
//!
//! One thread calls [`Engine::tick`] every few hundred milliseconds ([`Engine::run_forever`]);
//! commands call [`Engine::sync_now`], [`Engine::note_change`] and friends from other threads.
//! A step lock makes sure only one sync operation runs at a time.

use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use minimap_core::sync::{
    backoff_secs, checkpoint_due, checkpoint_name, default_checkpoint_prune_plan,
    parse_checkpoint_name, save_wait_ms, OFFLINE_WARNING_DAYS, ORPHAN_MEDIA_GRACE, POLL_SECS,
};
use minimap_store::{
    attachments, backup, merge, meta,
    security::{secure_remove, Key, RawKey},
    snapshot, Connection, StoreError,
};
use minimap_types::{
    BackupKind, CheckpointInfo, ConnectOutcome, DeviceInfo, MergeSummary, RecoverResult, SyncState,
    SyncStatus, DEFAULT_MEDIA_CACHE_BYTES,
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::{
    error::{Result, SyncError},
    media::MediaCache,
    remote::{CheckpointFile, DeviceFile, DeviceMeta, MediaFile, Remote},
    vault,
};

/// A merge that would remove more than this many items takes a safety backup first.
const BIG_MERGE_DELETES: u32 = 20;

/// What the engine needs from the app around it.
pub trait Host: Send + Sync {
    /// Runs `f` with the live connection and the key it is encrypted with (`Key::None` when
    /// plain) while holding the connection's lock. Fails with [`SyncError::Locked`] while an
    /// encrypted database is waiting for its key.
    fn with_db(&self, f: &mut dyn FnMut(&mut Connection, &Key)) -> Result<()>;

    /// The app's data folder (temporary files, the attachment cache and the default backup
    /// folder live under it).
    fn data_dir(&self) -> PathBuf;

    /// A merge changed this device's data; screens should reload.
    fn data_changed(&self) {}
}

/// What is on a remote Drive folder before this device connects to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Probe {
    /// Nothing of Minimap's.
    Empty,
    /// Data saved by other devices (or an earlier install of this one).
    Existing { devices: u32 },
}

/// The meta flag that says changes are waiting to be saved.
const DIRTY_KEY: &str = "local.sync_dirty";

fn kind_of(e: &SyncError) -> &'static str {
    match e {
        SyncError::Offline(_) => "offline",
        SyncError::Auth(_) => "auth",
        SyncError::Drive(_) => "drive",
        SyncError::WrongKey => "wrong_key",
        SyncError::Corrupt(_) => "corrupt",
        SyncError::NewerData { .. } => "newer_data",
        SyncError::Locked => "locked",
        SyncError::NotConnected => "not_connected",
        SyncError::Invalid(_) => "invalid",
        SyncError::Io(_) => "io",
        SyncError::Cancelled => "cancelled",
        SyncError::Store(_) => "store",
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
struct Persisted {
    /// Device id -> the stamp of the snapshot of theirs that was last merged.
    merged: BTreeMap<String, String>,
    /// How many times this device has saved.
    seq: u64,
    last_saved_at: Option<String>,
    /// The last time Drive answered at all.
    last_ok_at: Option<String>,
    checkpoint_at: Option<String>,
    first_merge_done: bool,
    last_merge: Option<MergeSummary>,
    account: Option<String>,
}

struct Inner {
    persisted: Persisted,
    dirty: bool,
    change_counter: u64,
    first_unsaved_ms: Option<u64>,
    last_change_ms: u64,
    failures: u32,
    next_attempt_ms: u64,
    last_pull_ms: Option<u64>,
    last_media_ms: Option<u64>,
    saving: bool,
    syncing: bool,
    offline: bool,
    needs_attention: bool,
    last_error: Option<String>,
    warnings: Vec<String>,
    /// Snapshots from newer versions already reported, so they aren't downloaded again.
    skipped: HashSet<String>,
    data_revision: u64,
    devices: Vec<DeviceFile>,
    checkpoints: Vec<CheckpointFile>,
    media: Vec<MediaFile>,
    media_waiting: u32,
    clock_skew_secs: Option<i64>,
    device_id: String,
    device_name: String,
}

pub struct Engine {
    host: Arc<dyn Host>,
    remote: Mutex<Option<Arc<dyn Remote>>>,
    inner: Mutex<Inner>,
    /// Only one sync operation at a time.
    step: Mutex<()>,
    started: Instant,
    /// Added to both clocks by [`advance`](Self::advance); tests use it to move time on.
    offset_ms: std::sync::atomic::AtomicU64,
    cache_limit: u64,
}

fn iso(at: OffsetDateTime) -> String {
    minimap_types::timefmt::fmt_ts(at)
}

fn parse_iso(text: &str) -> Option<OffsetDateTime> {
    minimap_types::timefmt::parse_ts(text).ok()
}

fn locked_inner(m: &Mutex<Inner>) -> std::sync::MutexGuard<'_, Inner> {
    // A panic in another thread must not stop syncing for good.
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Engine {
    pub fn new(host: Arc<dyn Host>) -> Arc<Self> {
        Arc::new(Self {
            host,
            remote: Mutex::new(None),
            inner: Mutex::new(Inner {
                persisted: Persisted::default(),
                dirty: false,
                change_counter: 0,
                first_unsaved_ms: None,
                last_change_ms: 0,
                failures: 0,
                next_attempt_ms: 0,
                last_pull_ms: None,
                last_media_ms: None,
                saving: false,
                syncing: false,
                offline: false,
                needs_attention: false,
                last_error: None,
                warnings: Vec::new(),
                skipped: HashSet::new(),
                data_revision: 0,
                devices: Vec::new(),
                checkpoints: Vec::new(),
                media: Vec::new(),
                media_waiting: 0,
                clock_skew_secs: None,
                device_id: String::new(),
                device_name: String::new(),
            }),
            step: Mutex::new(()),
            started: Instant::now(),
            offset_ms: std::sync::atomic::AtomicU64::new(0),
            cache_limit: DEFAULT_MEDIA_CACHE_BYTES,
        })
    }

    fn now_ms(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis())
            .unwrap_or(u64::MAX)
            .saturating_add(self.offset_ms.load(std::sync::atomic::Ordering::SeqCst))
    }

    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::now_utc()
            + time::Duration::milliseconds(
                i64::try_from(self.offset_ms.load(std::sync::atomic::Ordering::SeqCst))
                    .unwrap_or(i64::MAX),
            )
    }

    /// Moves both clocks forward without waiting (tests).
    pub fn advance(&self, by: Duration) {
        let ms = u64::try_from(by.as_millis()).unwrap_or(u64::MAX);
        self.offset_ms
            .fetch_add(ms, std::sync::atomic::Ordering::SeqCst);
    }

    fn inner(&self) -> std::sync::MutexGuard<'_, Inner> {
        locked_inner(&self.inner)
    }

    fn step_lock(&self) -> std::sync::MutexGuard<'_, ()> {
        self.step.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn db<T>(&self, f: impl FnOnce(&mut Connection) -> Result<T>) -> Result<T> {
        self.db_keyed(|conn, _| f(conn))
    }

    fn db_keyed<T>(&self, f: impl FnOnce(&mut Connection, &Key) -> Result<T>) -> Result<T> {
        let mut f = Some(f);
        let mut out: Option<Result<T>> = None;
        self.host.with_db(&mut |conn, key| {
            if let Some(f) = f.take() {
                out = Some(f(conn, key));
            }
        })?;
        out.unwrap_or(Err(SyncError::Locked))
    }

    /// The folder safety backups go to: the one chosen in Settings, else `backups` in the data
    /// folder.
    fn backup_dir(&self, conn: &Connection) -> PathBuf {
        minimap_store::settings::get(conn)
            .ok()
            .and_then(|s| s.backup_folder)
            .map(PathBuf::from)
            .unwrap_or_else(|| self.host.data_dir().join("backups"))
    }

    fn safety_backup(&self, conn: &Connection, key: &Key, kind: BackupKind) -> Result<()> {
        let version = minimap_store::schema_version(conn)?;
        backup::create(conn, &self.backup_dir(conn), kind, version, key)?;
        Ok(())
    }

    fn tmp_dir(&self) -> Result<PathBuf> {
        let dir = self.host.data_dir().join("sync-tmp");
        fs::create_dir_all(&dir)
            .map_err(|e| SyncError::io("Couldn't make a temporary folder", e))?;
        Ok(dir)
    }

    pub fn media_cache(&self) -> MediaCache {
        MediaCache::new(self.host.data_dir().join("media"))
    }

    fn remote(&self) -> Result<Arc<dyn Remote>> {
        self.remote
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .ok_or(SyncError::NotConnected)
    }

    /// Signs out: Google is told to forget the sign-in and the engine stops syncing. The data on
    /// this device stays complete.
    pub fn disconnect(&self) {
        let _step = self.step_lock();
        let remote = self.remote.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(remote) = remote {
            remote.revoke();
        }
        self.set_remote(None);
    }

    pub fn is_connected(&self) -> bool {
        self.remote
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    }

    fn vault_key(&self) -> Result<RawKey> {
        self.db(|conn| Ok(meta::vault_key(conn)?))?
            .ok_or_else(|| SyncError::Invalid("There is no vault key on this device".into()))
    }

    // ------------------------------------------------------------ state

    /// Reads what was saved last time (what was merged from whom, whether changes were left
    /// unsaved) and this device's identity. Call once at start, before [`tick`](Self::tick).
    pub fn load(&self) -> Result<()> {
        let (persisted, dirty, id, name) = self.db(|conn| {
            let persisted = meta::get(conn, meta::SYNC_STATE)?
                .and_then(|t| serde_json::from_str::<Persisted>(&t).ok())
                .unwrap_or_default();
            let dirty = meta::get(conn, DIRTY_KEY)?.is_some();
            Ok((
                persisted,
                dirty,
                meta::device_id(conn)?,
                meta::device_name(conn)?,
            ))
        })?;
        let mut inner = self.inner();
        inner.persisted = persisted;
        inner.dirty = dirty;
        inner.first_unsaved_ms = dirty.then(|| self.now_ms());
        inner.last_change_ms = 0;
        inner.device_id = id;
        inner.device_name = name;
        Ok(())
    }

    /// The name this device shows in the device list changed.
    pub fn set_device_name(&self, name: &str) {
        self.inner().device_name = name.to_owned();
    }

    fn persist(&self) {
        let (text, dirty) = {
            let inner = self.inner();
            (serde_json::to_string(&inner.persisted), inner.dirty)
        };
        let Ok(text) = text else { return };
        let _ = self.db(|conn| {
            meta::set(conn, meta::SYNC_STATE, &text)?;
            if dirty {
                meta::set(conn, DIRTY_KEY, "1")?;
            } else {
                meta::remove(conn, DIRTY_KEY)?;
            }
            Ok(())
        });
    }

    /// Connects (`Some`) or disconnects (`None`) the remote. Disconnecting forgets what was
    /// merged; the local data stays complete.
    pub fn set_remote(&self, remote: Option<Arc<dyn Remote>>) {
        let connected = remote.is_some();
        *self.remote.lock().unwrap_or_else(|e| e.into_inner()) = remote;
        {
            let mut inner = self.inner();
            inner.failures = 0;
            inner.next_attempt_ms = 0;
            inner.offline = false;
            inner.needs_attention = false;
            inner.last_error = None;
            inner.last_pull_ms = None;
            inner.last_media_ms = None;
            if !connected {
                inner.persisted = Persisted::default();
                inner.dirty = false;
                inner.first_unsaved_ms = None;
                inner.devices.clear();
                inner.checkpoints.clear();
                inner.media.clear();
                inner.media_waiting = 0;
                inner.skipped.clear();
            }
        }
        if !connected {
            let _ = self.db(|conn| {
                meta::remove(conn, meta::SYNC_STATE)?;
                meta::remove(conn, DIRTY_KEY)?;
                Ok(())
            });
        } else {
            self.persist();
        }
    }

    /// A command changed the data on this device. Call with the connection the command used,
    /// while its lock is still held (nothing here takes it again).
    pub fn note_change(&self, conn: &Connection) {
        if !self.is_connected() {
            return;
        }
        let now = self.now_ms();
        let first_time = {
            let mut inner = self.inner();
            inner.change_counter += 1;
            inner.last_change_ms = now;
            if inner.first_unsaved_ms.is_none() {
                inner.first_unsaved_ms = Some(now);
            }
            let first_time = !inner.dirty;
            inner.dirty = true;
            first_time
        };
        if first_time {
            let _ = meta::set(conn, DIRTY_KEY, "1");
        }
    }

    /// Something other than a command changed the data (a restore): save it.
    pub fn mark_dirty(&self) {
        {
            let mut inner = self.inner();
            inner.dirty = true;
            inner.change_counter += 1;
            let now = self.now_ms();
            inner.last_change_ms = now;
            inner.first_unsaved_ms.get_or_insert(now);
            inner.data_revision += 1;
        }
        self.persist();
    }

    // ------------------------------------------------------------ the loop

    /// One step of work: pull when it is time, save when it is time, look after attachment
    /// files. Returns how long to wait before the next call.
    pub fn tick(&self) -> Duration {
        self.tick_at(self.now_ms())
    }

    /// [`tick`](Self::tick) with the monotonic clock given (tests).
    pub fn tick_at(&self, now_ms: u64) -> Duration {
        if !self.is_connected() {
            return Duration::from_millis(1000);
        }
        let _step = self.step_lock();
        let (blocked, retry_at) = {
            let inner = self.inner();
            (inner.needs_attention, inner.next_attempt_ms)
        };
        if blocked {
            return Duration::from_secs(5);
        }
        if now_ms < retry_at {
            return Duration::from_millis((retry_at - now_ms).min(5_000));
        }
        let result = self.cycle(now_ms, false);
        match result {
            Ok(()) => self.succeeded(),
            Err(e) => self.failed(&e, now_ms),
        }
        self.next_wake(now_ms)
    }

    /// Runs forever, ticking; for the background thread.
    pub fn run_forever(self: Arc<Self>) {
        loop {
            let wait = self.tick();
            std::thread::sleep(wait.min(Duration::from_millis(500)));
        }
    }

    fn next_wake(&self, now_ms: u64) -> Duration {
        let inner = self.inner();
        let mut wait = POLL_SECS * 1000;
        if let Some(last) = inner.last_pull_ms {
            wait = (last + POLL_SECS * 1000).saturating_sub(now_ms);
        }
        if let Some(w) = save_wait_ms(
            inner.dirty.then_some(inner.first_unsaved_ms).flatten(),
            inner.last_change_ms,
            now_ms,
        ) {
            wait = wait.min(w.max(100));
        }
        if now_ms < inner.next_attempt_ms {
            wait = wait.min(inner.next_attempt_ms - now_ms);
        }
        Duration::from_millis(wait.clamp(100, 5_000))
    }

    /// Pull, save and media work as they come due (or all of it at once when `force`).
    fn cycle(&self, now_ms: u64, force: bool) -> Result<()> {
        let pull_due = force
            || self
                .inner()
                .last_pull_ms
                .is_none_or(|last| now_ms.saturating_sub(last) >= POLL_SECS * 1000);
        if pull_due {
            self.inner().syncing = true;
            let pulled = self.pull();
            let mut inner = self.inner();
            inner.syncing = false;
            inner.last_pull_ms = Some(now_ms);
            drop(inner);
            pulled?;
        }
        let save_due = {
            let inner = self.inner();
            inner.dirty
                && (force
                    || save_wait_ms(inner.first_unsaved_ms, inner.last_change_ms, now_ms)
                        == Some(0))
        };
        if save_due {
            self.push()?;
        }
        let media_due = force
            || save_due
            || self
                .inner()
                .last_media_ms
                .is_none_or(|last| now_ms.saturating_sub(last) >= 60_000);
        if media_due {
            self.inner().last_media_ms = Some(now_ms);
            self.sweep_media()?;
        }
        Ok(())
    }

    fn succeeded(&self) {
        let at = iso(self.now());
        {
            let mut inner = self.inner();
            inner.failures = 0;
            inner.offline = false;
            inner.last_error = None;
            inner.persisted.last_ok_at = Some(at);
        }
        self.persist();
    }

    fn failed(&self, e: &SyncError, now_ms: u64) {
        tracing::warn!(kind = %kind_of(e), "sync step failed");
        let mut inner = self.inner();
        inner.saving = false;
        inner.syncing = false;
        inner.last_error = Some(e.to_string());
        inner.offline = matches!(e, SyncError::Offline(_));
        if e.needs_attention() {
            inner.needs_attention = true;
        } else {
            let wait = backoff_secs(inner.failures) * 1000;
            inner.failures += 1;
            inner.next_attempt_ms = now_ms + wait;
        }
    }

    /// Pull and save right now, whatever the timers say, and report the first problem. A
    /// problem that needs the user (an expired sign-in) stays until they act.
    pub fn sync_now(&self) -> Result<()> {
        let _step = self.step_lock();
        self.remote()?;
        {
            let mut inner = self.inner();
            inner.needs_attention = false;
            inner.failures = 0;
            inner.next_attempt_ms = 0;
            if inner.dirty && inner.first_unsaved_ms.is_none() {
                inner.first_unsaved_ms = Some(0);
            }
        }
        let now_ms = self.now_ms();
        let result = self.cycle(now_ms, true);
        match &result {
            Ok(()) => self.succeeded(),
            Err(e) => self.failed(e, now_ms),
        }
        result
    }

    /// On quit: save what is unsaved, if it can be done quickly. Anything left stays marked
    /// unsaved and goes up at the next start.
    pub fn flush(&self) {
        if !self.is_connected() {
            return;
        }
        let _step = self.step_lock();
        if self.inner().dirty && !self.inner().needs_attention {
            let _ = self.push();
        }
    }

    // ------------------------------------------------------------ pulling

    fn pull(&self) -> Result<()> {
        let remote = self.remote()?;
        let listed = remote.list_devices()?;
        let me = self.inner().device_id.clone();
        {
            let mut inner = self.inner();
            if let Some(own) = listed.iter().find(|d| d.device_id == me) {
                inner.persisted.seq = inner.persisted.seq.max(own.seq);
            }
            inner.devices = listed.clone();
            if let Some(skew) = remote.clock_skew_seconds() {
                inner.clock_skew_secs = Some(skew);
            }
        }
        let vault = self.vault_key()?;
        let mut first_problem: Option<SyncError> = None;
        for device in listed.iter().filter(|d| d.device_id != me) {
            let stamp = device.stamp();
            {
                let inner = self.inner();
                if inner.persisted.merged.get(&device.device_id) == Some(&stamp)
                    || inner.skipped.contains(&stamp)
                {
                    continue;
                }
            }
            match self.merge_device(&*remote, device, &vault) {
                Ok(summary) => {
                    let changed = summary.changed_anything();
                    {
                        let mut inner = self.inner();
                        inner
                            .persisted
                            .merged
                            .insert(device.device_id.clone(), stamp);
                        inner.persisted.first_merge_done = true;
                        inner.warnings.retain(|w| !w.contains(&device.device_name));
                        if changed {
                            inner.persisted.last_merge = Some(summary);
                            inner.data_revision += 1;
                            // The others need to receive the combined result.
                            inner.dirty = true;
                            inner.change_counter += 1;
                            let now = self.now_ms();
                            inner.last_change_ms = now;
                            inner.first_unsaved_ms.get_or_insert(now);
                        }
                    }
                    self.persist();
                    if changed {
                        self.host.data_changed();
                    }
                }
                Err(SyncError::NewerData {
                    device: _,
                    found,
                    supported,
                }) => {
                    let mut inner = self.inner();
                    inner.skipped.insert(stamp);
                    let note = SyncError::NewerData {
                        device: device.device_name.clone(),
                        found,
                        supported,
                    }
                    .to_string();
                    if !inner.warnings.contains(&note) {
                        inner.warnings.push(note);
                    }
                }
                Err(e) if e.needs_attention() || matches!(e, SyncError::Corrupt(_)) => {
                    return Err(e)
                }
                Err(e) => {
                    first_problem.get_or_insert(e);
                }
            }
        }
        match first_problem {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Downloads one device's snapshot and merges it.
    fn merge_device(
        &self,
        remote: &dyn Remote,
        device: &DeviceFile,
        vault: &RawKey,
    ) -> Result<MergeSummary> {
        let tmp = self
            .tmp_dir()?
            .join(format!("pull-{}.db", device.device_id));
        let _ = fs::remove_file(&tmp);
        let result = (|| {
            remote.download_device(&device.device_id, &tmp)?;
            let first = !self.inner().persisted.first_merge_done;
            let name = device.device_name.clone();
            self.db_keyed(|conn, key| {
                let snapshot = snapshot::open_remote(&tmp, vault).map_err(|e| match e {
                    StoreError::WrongKey => SyncError::WrongKey,
                    StoreError::NewerData { found, supported } => SyncError::NewerData {
                        device: name.clone(),
                        found,
                        supported,
                    },
                    other => other.into(),
                })?;
                let preview = merge::merge(conn, &snapshot, &name, merge::MergeMode::Preview)?;
                if !preview.changed_anything() {
                    return Ok(preview);
                }
                if first || preview.deleted > BIG_MERGE_DELETES {
                    self.safety_backup(conn, key, BackupKind::PreSync)?;
                }
                Ok(merge::merge(
                    conn,
                    &snapshot,
                    &name,
                    merge::MergeMode::Apply,
                )?)
            })
        })();
        secure_remove(&tmp);
        result
    }

    // ------------------------------------------------------------ saving

    fn push(&self) -> Result<()> {
        let remote = self.remote()?;
        let vault = self.vault_key()?;
        let started_at = self.inner().change_counter;
        self.inner().saving = true;
        let tmp = self.tmp_dir()?.join("snapshot.db.enc");
        let outcome = (|| {
            self.db(|conn| Ok(snapshot::create(conn, &tmp, &vault)?))?;
            let at = self.now();
            let (device_id, device_name, seq) = {
                let inner = self.inner();
                (
                    inner.device_id.clone(),
                    inner.device_name.clone(),
                    inner.persisted.seq + 1,
                )
            };
            remote.upload_device(
                &DeviceMeta {
                    device_id: device_id.clone(),
                    device_name,
                    saved_at: iso(at),
                    seq,
                },
                &tmp,
            )?;
            {
                let mut inner = self.inner();
                inner.persisted.seq = seq;
                inner.persisted.last_saved_at = Some(iso(at));
                if inner.change_counter == started_at {
                    inner.dirty = false;
                    inner.first_unsaved_ms = None;
                }
                inner.saving = false;
            }
            self.persist();
            // A checkpoint is a bonus: if it fails the save itself still counts.
            let due = checkpoint_due(
                self.inner()
                    .persisted
                    .checkpoint_at
                    .as_deref()
                    .and_then(parse_iso),
                at,
            );
            if due {
                match self.keep_checkpoint(&*remote, &device_id, &tmp, at) {
                    Ok(()) => {
                        self.inner().persisted.checkpoint_at = Some(iso(at));
                        self.persist();
                    }
                    Err(e) => tracing::warn!(kind = %kind_of(&e), "checkpoint failed"),
                }
            }
            Ok(())
        })();
        secure_remove(&tmp);
        self.inner().saving = false;
        outcome
    }

    fn keep_checkpoint(
        &self,
        remote: &dyn Remote,
        device_id: &str,
        from: &std::path::Path,
        at: OffsetDateTime,
    ) -> Result<()> {
        remote.upload_checkpoint(&checkpoint_name(device_id, at), from)?;
        let all = remote.list_checkpoints()?;
        let mine: Vec<(String, OffsetDateTime)> = all
            .iter()
            .filter_map(|c| {
                let (id, when) = parse_checkpoint_name(&c.name)?;
                (id == device_id).then_some((c.name.clone(), when))
            })
            .collect();
        for name in default_checkpoint_prune_plan(&mine) {
            // Only ever this device's own, app-named checkpoints.
            remote.delete_checkpoint(&name)?;
        }
        self.inner().checkpoints = remote.list_checkpoints()?;
        Ok(())
    }

    // ------------------------------------------------------------ attachments

    /// Uploads attachment files that aren't on the remote yet, removes remote files nothing
    /// refers to any more (after a grace period), and trims the local cache.
    fn sweep_media(&self) -> Result<()> {
        let remote = self.remote()?;
        let cutoff = iso(self.now() - ORPHAN_MEDIA_GRACE);
        let (live, referenced) = self.db(|conn| {
            Ok((
                attachments::live_blobs(conn)?,
                attachments::referenced_shas(conn, &cutoff)?,
            ))
        })?;
        let on_remote_list = remote.list_media()?;
        let on_remote: HashSet<String> = on_remote_list.iter().map(|m| m.sha256.clone()).collect();
        let cache = self.media_cache();
        let mut waiting: HashSet<String> = HashSet::new();
        let mut first_error: Option<SyncError> = None;
        for (sha, _) in &live {
            if on_remote.contains(sha) {
                continue;
            }
            if !cache.has(sha) {
                continue; // this device never had it; the device that did will upload it
            }
            match cache.path(sha).and_then(|p| remote.upload_media(sha, &p)) {
                Ok(()) => {}
                Err(e) => {
                    waiting.insert(sha.clone());
                    first_error.get_or_insert(e);
                }
            }
        }
        // Files nothing refers to, uploaded long enough ago that every device has caught up.
        let referenced: HashSet<String> = referenced.into_iter().collect();
        let grace_cutoff = self.now() - ORPHAN_MEDIA_GRACE;
        for m in &on_remote_list {
            let old = parse_iso(&m.created).is_some_and(|t| t < grace_cutoff);
            if old && !referenced.contains(&m.sha256) {
                let _ = remote.delete_media(&m.sha256);
            }
        }
        cache.evict(self.cache_limit, &waiting);
        {
            let mut inner = self.inner();
            inner.media_waiting = u32::try_from(waiting.len()).unwrap_or(u32::MAX);
            inner.media = on_remote_list;
        }
        match first_error {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Makes sure the encrypted file for `sha256` is in the local cache, downloading it from the
    /// remote when it isn't, and returns its path.
    pub fn fetch_media(&self, sha256: &str) -> Result<PathBuf> {
        let cache = self.media_cache();
        let path = cache.path(sha256)?;
        if path.is_file() {
            cache.touch(sha256);
            return Ok(path);
        }
        let remote = self.remote()?;
        let vault = self.vault_key()?;
        let tmp = self.tmp_dir()?.join(format!("media-{sha256}.part"));
        let _ = fs::remove_file(&tmp);
        remote.download_media(sha256, &tmp).inspect_err(|_| {
            let _ = fs::remove_file(&tmp);
        })?;
        cache.accept_download(&vault, sha256, &tmp)?;
        cache.evict(self.cache_limit, &HashSet::new());
        Ok(path)
    }

    /// Where the bytes of an attachment are, for showing it in the list.
    pub fn is_on_remote(&self, sha256: &str) -> bool {
        self.inner().media.iter().any(|m| m.sha256 == sha256)
    }

    // ------------------------------------------------------------ connecting

    /// What is on the remote: nothing, or data from other devices.
    pub fn probe(remote: &dyn Remote) -> Result<Probe> {
        let vault = remote.read_vault()?;
        let devices = remote.list_devices()?;
        Ok(if vault.is_none() && devices.is_empty() {
            Probe::Empty
        } else {
            Probe::Existing {
                devices: u32::try_from(devices.len()).unwrap_or(u32::MAX),
            }
        })
    }

    /// First connect to an empty Drive: this device's vault key becomes the key of the Drive
    /// data, `vault.json` is written, and everything is uploaded. Returns the key, to be shown
    /// once as the recovery key.
    pub fn start_new(&self, remote: Arc<dyn Remote>) -> Result<RawKey> {
        let _step = self.step_lock();
        let key = self.db(|conn| Ok(meta::ensure_vault_key(conn)?))?;
        remote.write_vault(&vault::create(&key)?)?;
        let account = remote.account().ok();
        self.set_remote(Some(remote));
        self.inner().persisted.account = account;
        self.mark_dirty();
        let now_ms = self.now_ms();
        let result = self.cycle(now_ms, true);
        match &result {
            Ok(()) => self.succeeded(),
            Err(e) => self.failed(e, now_ms),
        }
        result?;
        Ok(key)
    }

    /// First connect to a Drive that already holds Minimap data. `recovery` must be the vault
    /// key of that data. A device with nothing of its own adopts the newest snapshot; one with
    /// data of its own merges (both sets are kept).
    pub fn join_existing(
        &self,
        remote: Arc<dyn Remote>,
        recovery: RawKey,
    ) -> Result<ConnectOutcome> {
        let _step = self.step_lock();
        let bytes = remote.read_vault()?.ok_or_else(|| {
            SyncError::Invalid("There is no Minimap data on this Google Drive".into())
        })?;
        vault::verify(&bytes, &recovery)?;
        let devices = remote.list_devices()?;
        let account = remote.account().ok();
        let pristine = self.db(|conn| Ok(snapshot::is_pristine(conn)?))?;
        let adopt_from = devices
            .iter()
            .max_by(|a, b| a.saved_at.cmp(&b.saved_at).then(a.seq.cmp(&b.seq)))
            .filter(|_| pristine)
            .cloned();
        // The key first: it is what opens everything that follows.
        let key_for_db = recovery.clone();
        self.db(|conn| Ok(meta::set_vault_key(conn, &key_for_db)?))?;
        let adopted = adopt_from.is_some();
        if let Some(newest) = adopt_from {
            self.adopt(&*remote, &newest, &recovery)?;
            let mut inner = self.inner();
            inner
                .persisted
                .merged
                .insert(newest.device_id.clone(), newest.stamp());
            inner.persisted.first_merge_done = true;
        }
        self.set_remote(Some(remote));
        {
            let mut inner = self.inner();
            inner.persisted.account = account;
            inner.data_revision += 1;
        }
        self.host.data_changed();
        self.mark_dirty();
        let now_ms = self.now_ms();
        let result = self.cycle(now_ms, true);
        match &result {
            Ok(()) => self.succeeded(),
            Err(e) => self.failed(e, now_ms),
        }
        result?;
        Ok(ConnectOutcome::Joined {
            adopted,
            devices: u32::try_from(devices.len()).unwrap_or(u32::MAX),
        })
    }

    /// Replaces this device's (empty) data with a snapshot from the remote.
    fn adopt(&self, remote: &dyn Remote, device: &DeviceFile, vault: &RawKey) -> Result<()> {
        let tmp = self
            .tmp_dir()?
            .join(format!("adopt-{}.db", device.device_id));
        let _ = fs::remove_file(&tmp);
        let result = (|| {
            remote.download_device(&device.device_id, &tmp)?;
            self.db_keyed(|conn, live| {
                let dir = self.backup_dir(conn);
                backup::restore(conn, &tmp, &dir, live, &[Key::Raw(vault.clone())]).map_err(
                    |e| match e {
                        StoreError::BackupKeyNeeded | StoreError::WrongKey => SyncError::WrongKey,
                        other => other.into(),
                    },
                )?;
                Ok(())
            })
        })();
        secure_remove(&tmp);
        result
    }

    // ------------------------------------------------------------ recovering

    /// The checkpoints on the remote (from this device and the others), newest first.
    pub fn list_checkpoints(&self) -> Result<Vec<CheckpointFile>> {
        let remote = self.remote()?;
        let mut all = remote.list_checkpoints()?;
        newest_first(&mut all);
        self.inner().checkpoints = all.clone();
        Ok(all)
    }

    /// Brings back, from the checkpoint `name`, every item that differs from it (see
    /// [`merge::MergeMode::Recover`]); a safety backup is taken first. The recovered items are
    /// new edits, so they reach the other devices.
    pub fn recover(&self, name: &str) -> Result<RecoverResult> {
        let _step = self.step_lock();
        let remote = self.remote()?;
        let vault = self.vault_key()?;
        let (device, _) = parse_checkpoint_name(name)
            .ok_or_else(|| SyncError::Invalid("That isn't one of Minimap's checkpoints".into()))?;
        let device_name = self
            .inner()
            .devices
            .iter()
            .find(|d| d.device_id == device)
            .map(|d| d.device_name.clone())
            .unwrap_or_else(|| "a checkpoint".to_owned());
        let tmp = self.tmp_dir()?.join("recover.db");
        let _ = fs::remove_file(&tmp);
        let result = (|| {
            remote.download_checkpoint(name, &tmp)?;
            self.db_keyed(|conn, key| {
                let snapshot = snapshot::open_remote(&tmp, &vault).map_err(|e| match e {
                    StoreError::WrongKey => SyncError::WrongKey,
                    other => other.into(),
                })?;
                self.safety_backup(conn, key, BackupKind::PreRecover)?;
                let summary =
                    merge::merge(conn, &snapshot, &device_name, merge::MergeMode::Recover)?;
                Ok(summary)
            })
        })();
        secure_remove(&tmp);
        let summary = result?;
        self.mark_dirty();
        self.host.data_changed();
        Ok(RecoverResult {
            restored: summary.added + summary.updated,
            saved_current_as: "a pre-recover backup in the backup folder".to_owned(),
        })
    }

    // ------------------------------------------------------------ status

    /// Everything the status bar and Settings show.
    pub fn status(&self, banner_hidden_until: Option<String>) -> SyncStatus {
        let connected = self.is_connected();
        let cache = self.media_cache();
        let inner = self.inner();
        let now = self.now();
        let seconds_ago = |text: &Option<String>| {
            text.as_deref()
                .and_then(parse_iso)
                .map(|t| u64::try_from((now - t).whole_seconds().max(0)).unwrap_or(0))
        };
        let last_saved_seconds_ago = seconds_ago(&inner.persisted.last_saved_at);
        let state = if !connected {
            SyncState::LocalOnly
        } else if inner.needs_attention {
            SyncState::NeedsAttention
        } else if inner.offline {
            SyncState::Offline
        } else if inner.last_error.is_some() && inner.failures > 0 {
            SyncState::Error
        } else if inner.saving {
            SyncState::Saving
        } else if inner.syncing {
            SyncState::Syncing
        } else {
            SyncState::Saved
        };
        let summary = match state {
            SyncState::LocalOnly => "Local only: not backed up".to_owned(),
            SyncState::NeedsAttention => "Google Drive needs you".to_owned(),
            SyncState::Offline if inner.dirty => "Offline · changes waiting".to_owned(),
            SyncState::Offline => "Offline".to_owned(),
            SyncState::Error => "Drive error".to_owned(),
            SyncState::Saving => "Saving…".to_owned(),
            SyncState::Syncing => "Syncing…".to_owned(),
            SyncState::Saved if inner.dirty => "Changes not saved yet".to_owned(),
            SyncState::Saved => match last_saved_seconds_ago {
                Some(s) => format!("Saved to Drive · {}", ago(s)),
                None => "Connected to Drive".to_owned(),
            },
        };
        let mut warnings = inner.warnings.clone();
        if connected {
            let silent_days = inner
                .persisted
                .last_ok_at
                .as_deref()
                .and_then(parse_iso)
                .map(|t| (now - t).whole_days());
            if silent_days.is_some_and(|d| d >= OFFLINE_WARNING_DAYS) {
                warnings.push(format!(
                    "Google Drive hasn't been reachable for {} days; recent changes exist on this device only",
                    silent_days.unwrap_or(0)
                ));
            }
            if inner.clock_skew_secs.is_some_and(|s| s.abs() > 120) {
                warnings.push(
                    "This computer's clock is more than 2 minutes off. Sync decides which edit is newest by the clock, so fix the time settings"
                        .to_owned(),
                );
            }
        }
        let devices: Vec<DeviceInfo> = {
            let mut list: Vec<DeviceInfo> = inner
                .devices
                .iter()
                .map(|d| DeviceInfo {
                    device_id: d.device_id.clone(),
                    name: if d.device_id == inner.device_id {
                        inner.device_name.clone()
                    } else {
                        d.device_name.clone()
                    },
                    last_saved: parse_iso(&d.saved_at)
                        .map(minimap_core::backup::display_time)
                        .unwrap_or_default(),
                    is_this_device: d.device_id == inner.device_id,
                })
                .collect();
            if connected && !list.iter().any(|d| d.is_this_device) {
                list.push(DeviceInfo {
                    device_id: inner.device_id.clone(),
                    name: inner.device_name.clone(),
                    last_saved: String::new(),
                    is_this_device: true,
                });
            }
            list.sort_by(|a, b| {
                b.is_this_device
                    .cmp(&a.is_this_device)
                    .then(a.name.cmp(&b.name))
            });
            list
        };
        let names: BTreeMap<&str, &str> = inner
            .devices
            .iter()
            .map(|d| (d.device_id.as_str(), d.device_name.as_str()))
            .collect();
        let mut checkpoints: Vec<CheckpointInfo> = inner
            .checkpoints
            .iter()
            .filter_map(|c| {
                let (id, at) = parse_checkpoint_name(&c.name)?;
                Some(CheckpointInfo {
                    name: c.name.clone(),
                    device_name: names
                        .get(id.as_str())
                        .copied()
                        .unwrap_or("another device")
                        .to_owned(),
                    created: minimap_core::backup::display_time(at),
                    bytes: c.bytes,
                })
            })
            .collect();
        checkpoints.sort_by_key(|c| std::cmp::Reverse(checkpoint_time(&c.name)));
        let drive_bytes = connected.then(|| {
            inner.devices.iter().map(|d| d.bytes).sum::<u64>()
                + inner.checkpoints.iter().map(|c| c.bytes).sum::<u64>()
                + inner.media.iter().map(|m| m.bytes).sum::<u64>()
        });
        SyncStatus {
            state,
            summary,
            connected,
            account: inner.persisted.account.clone(),
            client_configured: false,
            client_built_in: false,
            device_id: inner.device_id.clone(),
            device_name: inner.device_name.clone(),
            unsaved_changes: connected && inner.dirty,
            last_saved: inner
                .persisted
                .last_saved_at
                .as_deref()
                .and_then(parse_iso)
                .map(minimap_core::backup::display_time),
            last_saved_seconds_ago,
            last_merge: inner.persisted.last_merge.clone(),
            last_error: inner.last_error.clone(),
            warning: (!warnings.is_empty()).then(|| warnings.join(". ")),
            devices,
            checkpoints,
            drive_bytes,
            media_waiting: inner.media_waiting,
            data_revision: inner.data_revision,
            banner_hidden_until,
            media_cache_bytes: cache.total_bytes(),
            media_cache_limit_bytes: self.cache_limit,
        }
    }
}

fn checkpoint_time(name: &str) -> Option<OffsetDateTime> {
    parse_checkpoint_name(name).map(|(_, at)| at)
}

fn newest_first(list: &mut [CheckpointFile]) {
    list.sort_by_key(|c| std::cmp::Reverse((checkpoint_time(&c.name), c.name.clone())));
}

/// "12 s ago", "5 min ago", "3 h ago", "2 days ago".
fn ago(seconds: u64) -> String {
    match seconds {
        0..=4 => "just now".to_owned(),
        5..=59 => format!("{seconds} s ago"),
        60..=3599 => format!("{} min ago", seconds / 60),
        3600..=86_399 => format!("{} h ago", seconds / 3600),
        _ => format!("{} days ago", seconds / 86_400),
    }
}

#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;
