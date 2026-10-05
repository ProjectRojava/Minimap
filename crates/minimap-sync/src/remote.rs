//! Where the data goes: the operations the engine needs from a store of files. Google Drive
//! implements it ([`crate::drive`]); a plain folder implements it too ([`crate::folder`]) for
//! tests and for developing without a Google account.
//!
//! Layout (see spec 22): `vault.json`, `devices/<id>.db.enc` (one per device; a device only
//! ever writes its own), `checkpoints/<name>`, `media/<sha256>.bin`.

use std::path::Path;

use crate::error::Result;

/// A device's latest snapshot as it sits on the remote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceFile {
    pub device_id: String,
    pub device_name: String,
    /// RFC 3339, set by the device that saved it.
    pub saved_at: String,
    /// Counts that device's saves.
    pub seq: u64,
    pub bytes: u64,
}

impl DeviceFile {
    /// Changes whenever the device saves again; two stamps are equal only for the same save.
    pub fn stamp(&self) -> String {
        format!("{}@{}", self.seq, self.saved_at)
    }
}

/// What a device says about the snapshot it is uploading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceMeta {
    pub device_id: String,
    pub device_name: String,
    pub saved_at: String,
    pub seq: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointFile {
    pub name: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaFile {
    pub sha256: String,
    pub bytes: u64,
    /// RFC 3339: when it was uploaded (blobs nothing refers to are removed only after a grace
    /// period).
    pub created: String,
}

pub trait Remote: Send + Sync {
    /// Who is signed in, for the status line.
    fn account(&self) -> Result<String>;

    /// How far this computer's clock is ahead of the remote's, in seconds, when the remote
    /// said what time it is (Drive answers carry a `Date` header).
    fn clock_skew_seconds(&self) -> Option<i64> {
        None
    }

    /// Tells the service to forget this sign-in (best effort; a no-op where there is none).
    fn revoke(&self) {}

    fn read_vault(&self) -> Result<Option<Vec<u8>>>;
    fn write_vault(&self, bytes: &[u8]) -> Result<()>;

    fn list_devices(&self) -> Result<Vec<DeviceFile>>;
    /// Downloads a device's snapshot to `to` (a path the caller owns).
    fn download_device(&self, device_id: &str, to: &Path) -> Result<()>;
    /// Replaces the device's snapshot with `from`. The old snapshot stays until the new one has
    /// arrived in full.
    fn upload_device(&self, meta: &DeviceMeta, from: &Path) -> Result<()>;

    fn list_checkpoints(&self) -> Result<Vec<CheckpointFile>>;
    fn upload_checkpoint(&self, name: &str, from: &Path) -> Result<()>;
    fn download_checkpoint(&self, name: &str, to: &Path) -> Result<()>;
    fn delete_checkpoint(&self, name: &str) -> Result<()>;

    fn list_media(&self) -> Result<Vec<MediaFile>>;
    fn upload_media(&self, sha256: &str, from: &Path) -> Result<()>;
    fn download_media(&self, sha256: &str, to: &Path) -> Result<()>;
    fn delete_media(&self, sha256: &str) -> Result<()>;
}

/// Only names the engine itself makes are ever sent to a remote, but a remote must not trust
/// that: a name with a path separator or a dot-dot would escape the folder.
pub fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 200
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// The checks every `Remote` implementation must pass; each implementation's tests call this.
#[cfg(test)]
pub(crate) fn contract(remote: &dyn Remote, scratch: &Path) {
    use std::fs;

    use crate::error::SyncError;

    std::fs::create_dir_all(scratch).unwrap();
    let file = |name: &str, content: &[u8]| {
        let p = scratch.join(name);
        fs::write(&p, content).unwrap();
        p
    };

    // The vault file: absent, then written, then replaced.
    assert_eq!(remote.read_vault().unwrap(), None);
    remote.write_vault(b"one").unwrap();
    remote.write_vault(b"two").unwrap();
    assert_eq!(remote.read_vault().unwrap().as_deref(), Some(&b"two"[..]));

    // Device snapshots: one per device, replaced by that device only.
    assert!(remote.list_devices().unwrap().is_empty());
    let a = DeviceMeta {
        device_id: "aaaaaaaa-0000-0000-0000-000000000001".into(),
        device_name: "Laptop".into(),
        saved_at: "2027-03-03T10:00:00.000Z".into(),
        seq: 1,
    };
    let b = DeviceMeta {
        device_id: "bbbbbbbb-0000-0000-0000-000000000002".into(),
        device_name: "Desktop".into(),
        saved_at: "2027-03-03T10:00:05.000Z".into(),
        seq: 7,
    };
    remote
        .upload_device(&a, &file("a1", b"snapshot a1"))
        .unwrap();
    remote
        .upload_device(&b, &file("b1", b"snapshot b"))
        .unwrap();
    let mut a2 = a.clone();
    a2.seq = 2;
    a2.saved_at = "2027-03-03T10:01:00.000Z".into();
    remote
        .upload_device(&a2, &file("a2", b"second snapshot of a"))
        .unwrap();
    let mut listed = remote.list_devices().unwrap();
    listed.sort_by(|x, y| x.device_id.cmp(&y.device_id));
    assert_eq!(listed.len(), 2, "{listed:?}");
    assert_eq!(listed[0].device_name, "Laptop");
    assert_eq!(listed[0].seq, 2);
    assert_eq!(listed[0].saved_at, "2027-03-03T10:01:00.000Z");
    assert_eq!(listed[0].bytes, b"second snapshot of a".len() as u64);
    assert_eq!(listed[1].seq, 7);
    assert_ne!(listed[0].stamp(), a.seq.to_string());
    let out = scratch.join("down-a");
    remote.download_device(&a.device_id, &out).unwrap();
    assert_eq!(fs::read(&out).unwrap(), b"second snapshot of a");
    assert!(remote
        .download_device("cccccccc-0000-0000-0000-000000000003", &out)
        .is_err());

    // Checkpoints: listed by name, deleted by name.
    assert!(remote.list_checkpoints().unwrap().is_empty());
    let cp1 = format!("{}-20270303-100000.db.enc", a.device_id);
    let cp2 = format!("{}-20270303-110000.db.enc", a.device_id);
    remote
        .upload_checkpoint(&cp1, &file("c1", b"cp one"))
        .unwrap();
    remote
        .upload_checkpoint(&cp2, &file("c2", b"cp two"))
        .unwrap();
    let mut names: Vec<String> = remote
        .list_checkpoints()
        .unwrap()
        .into_iter()
        .map(|c| c.name)
        .collect();
    names.sort();
    assert_eq!(names, vec![cp1.clone(), cp2.clone()]);
    let out = scratch.join("down-cp");
    remote.download_checkpoint(&cp2, &out).unwrap();
    assert_eq!(fs::read(&out).unwrap(), b"cp two");
    remote.delete_checkpoint(&cp1).unwrap();
    assert_eq!(remote.list_checkpoints().unwrap().len(), 1);
    remote.delete_checkpoint(&cp1).unwrap(); // already gone is fine

    // Media: by checksum, shared by every device.
    assert!(remote.list_media().unwrap().is_empty());
    let sha = "ab".repeat(32);
    remote
        .upload_media(&sha, &file("m1", b"blob bytes"))
        .unwrap();
    remote
        .upload_media(&sha, &file("m1b", b"blob bytes"))
        .unwrap(); // again: still one
    let media = remote.list_media().unwrap();
    assert_eq!(media.len(), 1, "{media:?}");
    assert_eq!(media[0].sha256, sha);
    assert_eq!(media[0].bytes, 10);
    assert!(!media[0].created.is_empty());
    let out = scratch.join("down-m");
    remote.download_media(&sha, &out).unwrap();
    assert_eq!(fs::read(&out).unwrap(), b"blob bytes");
    remote.delete_media(&sha).unwrap();
    assert!(remote.list_media().unwrap().is_empty());
    remote.delete_media(&sha).unwrap();

    // Names that could escape the folder are refused.
    for bad in ["../x", "a/b", "", ".hidden"] {
        assert!(
            matches!(
                remote.upload_checkpoint(bad, &file("bad", b"x")),
                Err(SyncError::Invalid(_))
            ),
            "{bad:?}"
        );
    }
    assert!(!remote.account().unwrap().is_empty());
}
