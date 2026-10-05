//! A [`Remote`] backed by a plain folder. It exists for tests (two devices syncing through one
//! folder exercise the whole engine without Google) and for developing without an account; it
//! can pretend the network is down or that an upload is cut off half way.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

use serde::{Deserialize, Serialize};

use crate::{
    error::{Result, SyncError},
    remote::{safe_name, CheckpointFile, DeviceFile, DeviceMeta, MediaFile, Remote},
};

pub struct FolderRemote {
    root: PathBuf,
    offline: AtomicBool,
    /// How many of the next uploads are cut off half way.
    interrupt_uploads: Mutex<u32>,
}

#[derive(Serialize, Deserialize)]
struct MetaFile {
    device_id: String,
    device_name: String,
    saved_at: String,
    seq: u64,
}

impl FolderRemote {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            offline: AtomicBool::new(false),
            interrupt_uploads: Mutex::new(0),
        }
    }

    /// Pretend the network is down (every call fails with `Offline`) or back up.
    pub fn set_offline(&self, offline: bool) {
        self.offline.store(offline, Ordering::SeqCst);
    }

    /// The next `n` uploads write half of their file and then fail, like a dropped connection.
    pub fn interrupt_next_uploads(&self, n: u32) {
        if let Ok(mut left) = self.interrupt_uploads.lock() {
            *left = n;
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn check(&self) -> Result<()> {
        if self.offline.load(Ordering::SeqCst) {
            Err(SyncError::Offline("no network (simulated)".into()))
        } else {
            Ok(())
        }
    }

    fn dir(&self, name: &str) -> Result<PathBuf> {
        let dir = self.root.join(name);
        fs::create_dir_all(&dir).map_err(|e| SyncError::io("Couldn't make a folder", e))?;
        Ok(dir)
    }

    fn named(&self, dir: &str, name: &str) -> Result<PathBuf> {
        if !safe_name(name) {
            return Err(SyncError::Invalid(format!("{name:?} is not a valid name")));
        }
        Ok(self.dir(dir)?.join(name))
    }

    /// Writes `from` to `to` via a `.part` file, so a reader never sees a half-written file.
    fn put(&self, from: &Path, to: &Path) -> Result<()> {
        self.check()?;
        let part = to.with_extension("part");
        let bytes = fs::read(from).map_err(|e| SyncError::io("Couldn't read the file", e))?;
        let cut_off = self.interrupt_uploads.lock().is_ok_and(|mut left| {
            let now = *left > 0;
            *left = left.saturating_sub(1);
            now
        });
        if cut_off {
            let mut f = fs::File::create(&part).map_err(|e| SyncError::io("write", e))?;
            let _ = f.write_all(&bytes[..bytes.len() / 2]);
            return Err(SyncError::Offline(
                "the connection dropped (simulated)".into(),
            ));
        }
        fs::write(&part, bytes).map_err(|e| SyncError::io("Couldn't write the file", e))?;
        fs::rename(&part, to).map_err(|e| SyncError::io("Couldn't finish the file", e))
    }

    fn get(&self, from: &Path, to: &Path) -> Result<()> {
        self.check()?;
        let bytes = fs::read(from).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                SyncError::Drive("that file isn't there".into())
            } else {
                SyncError::io("Couldn't read the file", e)
            }
        })?;
        fs::write(to, bytes).map_err(|e| SyncError::io("Couldn't write the file", e))
    }

    fn modified(path: &Path) -> String {
        fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .map(|t| {
                let t: time::OffsetDateTime = t.into();
                minimap_types::timefmt::fmt_ts(t)
            })
            .unwrap_or_default()
    }
}

impl Remote for FolderRemote {
    fn account(&self) -> Result<String> {
        self.check()?;
        Ok("test@example.com".into())
    }

    fn read_vault(&self) -> Result<Option<Vec<u8>>> {
        self.check()?;
        match fs::read(self.root.join("vault.json")) {
            Ok(b) => Ok(Some(b)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(SyncError::io("Couldn't read the vault file", e)),
        }
    }

    fn write_vault(&self, bytes: &[u8]) -> Result<()> {
        self.check()?;
        fs::create_dir_all(&self.root).map_err(|e| SyncError::io("Couldn't make a folder", e))?;
        let part = self.root.join("vault.json.part");
        fs::write(&part, bytes).map_err(|e| SyncError::io("Couldn't write the vault file", e))?;
        fs::rename(&part, self.root.join("vault.json"))
            .map_err(|e| SyncError::io("Couldn't finish the vault file", e))
    }

    fn list_devices(&self) -> Result<Vec<DeviceFile>> {
        self.check()?;
        let dir = self.dir("devices")?;
        let mut out = Vec::new();
        for entry in fs::read_dir(&dir)
            .map_err(|e| SyncError::io("Couldn't list devices", e))?
            .flatten()
        {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(id) = minimap_core::sync::parse_device_file_name(&name) else {
                continue;
            };
            let meta_path = dir.join(format!("{id}.json"));
            let Ok(text) = fs::read(&meta_path) else {
                continue;
            };
            let Ok(meta) = serde_json::from_slice::<MetaFile>(&text) else {
                continue;
            };
            let bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
            out.push(DeviceFile {
                device_id: meta.device_id,
                device_name: meta.device_name,
                saved_at: meta.saved_at,
                seq: meta.seq,
                bytes,
            });
        }
        Ok(out)
    }

    fn download_device(&self, device_id: &str, to: &Path) -> Result<()> {
        let from = self.named("devices", &minimap_core::sync::device_file_name(device_id))?;
        self.get(&from, to)
    }

    fn upload_device(&self, meta: &DeviceMeta, from: &Path) -> Result<()> {
        let name = minimap_core::sync::device_file_name(&meta.device_id);
        let to = self.named("devices", &name)?;
        self.put(from, &to)?;
        // The meta file is written after the snapshot, so a listing never shows a newer save
        // than the file it describes.
        let json = serde_json::to_vec(&MetaFile {
            device_id: meta.device_id.clone(),
            device_name: meta.device_name.clone(),
            saved_at: meta.saved_at.clone(),
            seq: meta.seq,
        })
        .map_err(|e| SyncError::Invalid(e.to_string()))?;
        fs::write(to.with_file_name(format!("{}.json", meta.device_id)), json)
            .map_err(|e| SyncError::io("Couldn't write the device record", e))
    }

    fn list_checkpoints(&self) -> Result<Vec<CheckpointFile>> {
        self.check()?;
        let mut out = Vec::new();
        for entry in fs::read_dir(self.dir("checkpoints")?)
            .map_err(|e| SyncError::io("Couldn't list checkpoints", e))?
            .flatten()
        {
            let name = entry.file_name().to_string_lossy().into_owned();
            if minimap_core::sync::parse_checkpoint_name(&name).is_some() {
                out.push(CheckpointFile {
                    name,
                    bytes: entry.metadata().map(|m| m.len()).unwrap_or(0),
                });
            }
        }
        Ok(out)
    }

    fn upload_checkpoint(&self, name: &str, from: &Path) -> Result<()> {
        let to = self.named("checkpoints", name)?;
        self.put(from, &to)
    }

    fn download_checkpoint(&self, name: &str, to: &Path) -> Result<()> {
        let from = self.named("checkpoints", name)?;
        self.get(&from, to)
    }

    fn delete_checkpoint(&self, name: &str) -> Result<()> {
        self.check()?;
        let path = self.named("checkpoints", name)?;
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(SyncError::io("Couldn't delete a checkpoint", e)),
        }
    }

    fn list_media(&self) -> Result<Vec<MediaFile>> {
        self.check()?;
        let mut out = Vec::new();
        for entry in fs::read_dir(self.dir("media")?)
            .map_err(|e| SyncError::io("Couldn't list media", e))?
            .flatten()
        {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(sha) = name.strip_suffix(".bin") else {
                continue;
            };
            if !minimap_core::attachments::is_sha256(sha) {
                continue;
            }
            out.push(MediaFile {
                sha256: sha.to_owned(),
                bytes: entry.metadata().map(|m| m.len()).unwrap_or(0),
                created: Self::modified(&entry.path()),
            });
        }
        Ok(out)
    }

    fn upload_media(&self, sha256: &str, from: &Path) -> Result<()> {
        let to = self.named("media", &format!("{sha256}.bin"))?;
        self.put(from, &to)
    }

    fn download_media(&self, sha256: &str, to: &Path) -> Result<()> {
        let from = self.named("media", &format!("{sha256}.bin"))?;
        self.get(&from, to)
    }

    fn delete_media(&self, sha256: &str) -> Result<()> {
        self.check()?;
        let path = self.named("media", &format!("{sha256}.bin"))?;
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(SyncError::io("Couldn't delete a file", e)),
        }
    }
}

#[cfg(test)]
pub(crate) fn temp_dir(name: &str) -> PathBuf {
    use std::sync::atomic::AtomicU32;
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "minimap-sync-{name}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_folder_remote_meets_the_contract() {
        let dir = temp_dir("folder-contract");
        let remote = FolderRemote::new(dir.join("drive"));
        crate::remote::contract(&remote, &dir.join("scratch"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_interrupted_upload_leaves_the_previous_file_intact() {
        let dir = temp_dir("folder-interrupt");
        let remote = FolderRemote::new(dir.join("drive"));
        let meta = |seq| DeviceMeta {
            device_id: "aaaaaaaa-0000-0000-0000-000000000001".into(),
            device_name: "Laptop".into(),
            saved_at: format!("2027-03-03T10:00:0{seq}.000Z"),
            seq,
        };
        let one = dir.join("one");
        fs::write(&one, b"first snapshot, complete").unwrap();
        remote.upload_device(&meta(1), &one).unwrap();

        let two = dir.join("two");
        fs::write(&two, b"second snapshot, which never arrives in full").unwrap();
        remote.interrupt_next_uploads(1);
        assert!(remote.upload_device(&meta(2), &two).is_err());
        let out = dir.join("out");
        remote.download_device(&meta(1).device_id, &out).unwrap();
        assert_eq!(fs::read(&out).unwrap(), b"first snapshot, complete");
        assert_eq!(remote.list_devices().unwrap()[0].seq, 1);
        // The next try goes through.
        remote.upload_device(&meta(2), &two).unwrap();
        assert_eq!(remote.list_devices().unwrap()[0].seq, 2);

        remote.set_offline(true);
        assert!(matches!(remote.list_devices(), Err(SyncError::Offline(_))));
        fs::remove_dir_all(&dir).unwrap();
    }
}
