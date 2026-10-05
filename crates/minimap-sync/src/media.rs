//! The local cache of attachment files (spec 22). Every blob is stored encrypted, exactly as it
//! sits on Drive (`<sha256>.bin`), so the cache is protected even when the database is not, and
//! a blob can be uploaded as it is. Least recently used blobs are evicted when the cache is
//! over its limit, but never one that is not on Drive yet.

use std::{
    collections::HashSet,
    fs,
    io::{BufReader, BufWriter},
    path::{Path, PathBuf},
    time::SystemTime,
};

use minimap_core::attachments::is_sha256;
use minimap_store::security::{restrict_permissions, RawKey};

use crate::{
    crypto::{decrypt_stream, encrypt_stream, Sealed},
    error::{Result, SyncError},
};

pub struct MediaCache {
    dir: PathBuf,
}

impl MediaCache {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn ensure_dir(&self) -> Result<()> {
        fs::create_dir_all(&self.dir)
            .map_err(|e| SyncError::io("Couldn't make the attachment folder", e))
    }

    /// Where the blob for `sha256` is (or would be).
    pub fn path(&self, sha256: &str) -> Result<PathBuf> {
        if !is_sha256(sha256) {
            return Err(SyncError::Invalid("not a valid file checksum".into()));
        }
        Ok(self.dir.join(format!("{sha256}.bin")))
    }

    pub fn has(&self, sha256: &str) -> bool {
        self.path(sha256).is_ok_and(|p| p.is_file())
    }

    /// Encrypts the file at `src` into the cache and returns its checksum and size. The same
    /// content is stored once.
    pub fn add_file(&self, vault: &RawKey, src: &Path) -> Result<Sealed> {
        let input = fs::File::open(src).map_err(|e| SyncError::io("Couldn't open the file", e))?;
        self.add_stream(vault, &mut BufReader::new(input))
    }

    pub fn add_bytes(&self, vault: &RawKey, bytes: &[u8]) -> Result<Sealed> {
        self.add_stream(vault, &mut &bytes[..])
    }

    fn add_stream(&self, vault: &RawKey, input: &mut impl std::io::Read) -> Result<Sealed> {
        self.ensure_dir()?;
        let tmp = self
            .dir
            .join(format!("incoming-{}.part", uuid::Uuid::now_v7()));
        let sealed = (|| {
            let out = fs::File::create(&tmp).map_err(|e| SyncError::io("Couldn't write", e))?;
            let mut out = BufWriter::new(out);
            let sealed = encrypt_stream(vault, input, &mut out)?;
            std::io::Write::flush(&mut out).map_err(|e| SyncError::io("Couldn't write", e))?;
            Ok::<_, SyncError>(sealed)
        })();
        let sealed = match sealed {
            Ok(s) => s,
            Err(e) => {
                let _ = fs::remove_file(&tmp);
                return Err(e);
            }
        };
        let target = self.path(&sealed.sha256)?;
        if target.is_file() {
            let _ = fs::remove_file(&tmp);
        } else {
            restrict_permissions(&tmp);
            fs::rename(&tmp, &target).map_err(|e| SyncError::io("Couldn't store the file", e))?;
        }
        Ok(sealed)
    }

    /// Stores a blob downloaded from Drive after checking it decrypts with `vault` and hashes
    /// to `sha256`. A blob that fails is deleted and reported as `Corrupt`.
    pub fn accept_download(&self, vault: &RawKey, sha256: &str, downloaded: &Path) -> Result<()> {
        let target = self.path(sha256)?;
        let check = (|| {
            let input = fs::File::open(downloaded)
                .map_err(|e| SyncError::io("Couldn't read the download", e))?;
            decrypt_stream(
                vault,
                &mut BufReader::new(input),
                &mut std::io::sink(),
                Some(sha256),
            )
        })();
        if let Err(e) = check {
            let _ = fs::remove_file(downloaded);
            return Err(e);
        }
        self.ensure_dir()?;
        restrict_permissions(downloaded);
        fs::rename(downloaded, &target)
            .or_else(|_| {
                fs::copy(downloaded, &target)
                    .map(|_| ())
                    .and_then(|()| fs::remove_file(downloaded))
            })
            .map_err(|e| SyncError::io("Couldn't store the download", e))
    }

    /// Marks the blob as just used (for least-recently-used eviction).
    pub fn touch(&self, sha256: &str) {
        if let Ok(path) = self.path(sha256) {
            if let Ok(f) = fs::OpenOptions::new().write(true).open(path) {
                let _ = f.set_modified(SystemTime::now());
            }
        }
    }

    /// Decrypts a blob into memory, checking its checksum.
    pub fn read_plain(&self, vault: &RawKey, sha256: &str) -> Result<Vec<u8>> {
        let path = self.path(sha256)?;
        let input =
            fs::File::open(&path).map_err(|e| SyncError::io("The file isn't on this device", e))?;
        let mut out = Vec::new();
        decrypt_stream(vault, &mut BufReader::new(input), &mut out, Some(sha256))?;
        self.touch(sha256);
        Ok(out)
    }

    /// Decrypts a blob into the file `dest` (for opening it with another program).
    pub fn export_plain(&self, vault: &RawKey, sha256: &str, dest: &Path) -> Result<()> {
        let path = self.path(sha256)?;
        let input =
            fs::File::open(&path).map_err(|e| SyncError::io("The file isn't on this device", e))?;
        let out =
            fs::File::create(dest).map_err(|e| SyncError::io("Couldn't write the file", e))?;
        restrict_permissions(dest);
        let mut out = BufWriter::new(out);
        let result = decrypt_stream(vault, &mut BufReader::new(input), &mut out, Some(sha256));
        match result {
            Ok(_) => {
                std::io::Write::flush(&mut out)
                    .map_err(|e| SyncError::io("Couldn't write the file", e))?;
                self.touch(sha256);
                Ok(())
            }
            Err(e) => {
                drop(out);
                let _ = fs::remove_file(dest);
                Err(e)
            }
        }
    }

    /// `(sha256, bytes, last used)` of every blob.
    fn entries(&self) -> Vec<(String, u64, SystemTime)> {
        let Ok(read) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        read.flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let sha = name.strip_suffix(".bin")?;
                if !is_sha256(sha) {
                    return None;
                }
                let meta = e.metadata().ok()?;
                Some((
                    sha.to_owned(),
                    meta.len(),
                    meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                ))
            })
            .collect()
    }

    pub fn total_bytes(&self) -> u64 {
        self.entries().iter().map(|(_, n, _)| n).sum()
    }

    /// The blobs on this device.
    pub fn shas(&self) -> HashSet<String> {
        self.entries().into_iter().map(|(s, _, _)| s).collect()
    }

    /// Removes least recently used blobs until the cache fits in `limit` bytes, skipping
    /// `keep` (blobs that are not on Drive yet). Returns how many were removed.
    pub fn evict(&self, limit: u64, keep: &HashSet<String>) -> u32 {
        let mut entries = self.entries();
        let mut total: u64 = entries.iter().map(|(_, n, _)| n).sum();
        entries.sort_by_key(|(sha, _, used)| (*used, sha.clone()));
        let mut removed = 0;
        for (sha, bytes, _) in entries {
            if total <= limit {
                break;
            }
            if keep.contains(&sha) {
                continue;
            }
            if self.remove(&sha) {
                total = total.saturating_sub(bytes);
                removed += 1;
            }
        }
        removed
    }

    pub fn remove(&self, sha256: &str) -> bool {
        self.path(sha256).is_ok_and(|p| fs::remove_file(p).is_ok())
    }

    /// Deletes half-written files left by a crash.
    pub fn clean_partials(&self) {
        if let Ok(read) = fs::read_dir(&self.dir) {
            for e in read.flatten() {
                if e.file_name().to_string_lossy().ends_with(".part") {
                    let _ = fs::remove_file(e.path());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::folder::temp_dir;

    fn key() -> RawKey {
        RawKey::generate().unwrap()
    }

    #[test]
    fn a_file_goes_in_encrypted_comes_out_the_same_and_is_stored_once() {
        let dir = temp_dir("media-basic");
        let cache = MediaCache::new(dir.join("media"));
        let k = key();
        let src = dir.join("plan.md");
        fs::write(&src, "# Q3 plan\nsecret roadmap").unwrap();
        let sealed = cache.add_file(&k, &src).unwrap();
        assert_eq!(sealed.plain_bytes, 24);
        assert!(cache.has(&sealed.sha256));
        let on_disk = fs::read(cache.path(&sealed.sha256).unwrap()).unwrap();
        assert!(!on_disk.windows(7).any(|w| w == b"roadmap"));
        assert_eq!(
            cache.read_plain(&k, &sealed.sha256).unwrap(),
            b"# Q3 plan\nsecret roadmap"
        );
        // Same content again (even from another name): still one blob.
        let again = cache.add_bytes(&k, b"# Q3 plan\nsecret roadmap").unwrap();
        assert_eq!(again.sha256, sealed.sha256);
        assert_eq!(cache.shas().len(), 1);
        // Exported for another program to open.
        let out = dir.join("opened.md");
        cache.export_plain(&k, &sealed.sha256, &out).unwrap();
        assert_eq!(fs::read(&out).unwrap(), b"# Q3 plan\nsecret roadmap");
        // The wrong key can't read it, and leaves no export behind.
        let out2 = dir.join("nope.md");
        assert!(cache.export_plain(&key(), &sealed.sha256, &out2).is_err());
        assert!(!out2.exists());
        assert!(cache.read_plain(&k, &"0".repeat(64)).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn nothing_but_a_checksum_names_a_path() {
        let cache = MediaCache::new("/tmp/does-not-matter");
        for bad in ["../x", "a/b", "short", &"G".repeat(64)] {
            assert!(cache.path(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn downloads_are_checked_before_they_are_kept() {
        let dir = temp_dir("media-download");
        let cache = MediaCache::new(dir.join("media"));
        let (k, other) = (key(), key());
        // A good blob made elsewhere (another device's cache).
        let elsewhere = MediaCache::new(dir.join("other"));
        let sealed = elsewhere.add_bytes(&k, b"drawing").unwrap();
        let blob = elsewhere.path(&sealed.sha256).unwrap();

        let good = dir.join("good.part");
        fs::copy(&blob, &good).unwrap();
        cache.accept_download(&k, &sealed.sha256, &good).unwrap();
        assert_eq!(cache.read_plain(&k, &sealed.sha256).unwrap(), b"drawing");

        // Encrypted with another key, tampered with, or not what the name says: all refused
        // and not kept.
        let fresh = MediaCache::new(dir.join("fresh"));
        let wrong_key = dir.join("wrong.part");
        fs::copy(&blob, &wrong_key).unwrap();
        assert!(fresh
            .accept_download(&other, &sealed.sha256, &wrong_key)
            .is_err());
        let mut bytes = fs::read(&blob).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        let tampered = dir.join("tampered.part");
        fs::write(&tampered, bytes).unwrap();
        assert!(fresh
            .accept_download(&k, &sealed.sha256, &tampered)
            .is_err());
        let wrong_name = dir.join("renamed.part");
        fs::copy(&blob, &wrong_name).unwrap();
        assert!(fresh
            .accept_download(&k, &"1".repeat(64), &wrong_name)
            .is_err());
        assert!(fresh.shas().is_empty());
        assert!(!wrong_key.exists() && !tampered.exists() && !wrong_name.exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn eviction_drops_the_least_recently_used_and_never_what_is_not_on_drive() {
        let dir = temp_dir("media-evict");
        let cache = MediaCache::new(dir.join("media"));
        let k = key();
        let mut shas = Vec::new();
        for i in 0..4u8 {
            let sealed = cache.add_bytes(&k, &vec![i; 1000]).unwrap();
            // Space the "last used" times out.
            let f = fs::OpenOptions::new()
                .write(true)
                .open(cache.path(&sealed.sha256).unwrap())
                .unwrap();
            f.set_modified(SystemTime::now() - Duration::from_secs(1000 - u64::from(i) * 100))
                .unwrap();
            shas.push(sealed.sha256);
        }
        let one = cache.total_bytes() / 4;
        // Using the oldest blob makes it the newest.
        cache.read_plain(&k, &shas[0]).unwrap();
        // Room for two; the oldest unused (shas[1]) and the next (shas[2]) go... but shas[1] is
        // not on Drive yet, so it stays.
        let keep: HashSet<String> = [shas[1].clone()].into();
        let removed = cache.evict(2 * one, &keep);
        assert_eq!(removed, 2);
        let left = cache.shas();
        assert!(left.contains(&shas[1]), "not on Drive: kept");
        assert!(left.contains(&shas[0]), "just used: kept");
        assert!(!left.contains(&shas[2]) && !left.contains(&shas[3]));
        // Nothing to do when it fits.
        assert_eq!(cache.evict(u64::MAX, &HashSet::new()), 0);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn partial_files_from_a_crash_are_cleaned_up() {
        let dir = temp_dir("media-partials");
        let cache = MediaCache::new(dir.join("media"));
        cache.add_bytes(&key(), b"x").unwrap();
        fs::write(dir.join("media").join("incoming-123.part"), b"half").unwrap();
        cache.clean_partials();
        assert_eq!(fs::read_dir(dir.join("media")).unwrap().count(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }
}
