//! Encryption at rest (spec 21) with SQLCipher.
//!
//! Keys are either 32 random bytes (kept in the OS keychain by the app) or a passphrase (kept
//! nowhere; SQLCipher derives the key with PBKDF2). Key material lives in types that wipe
//! themselves on drop and print as `<redacted>`, and nothing here logs it.
//!
//! SQLCipher cannot encrypt or decrypt a database in place (`PRAGMA rekey` only changes the key
//! of an already encrypted one), and its backup API refuses to copy between plain and encrypted
//! files. Every change of key therefore **exports** the data into a new file with the new key,
//! checks that copy against the original, and only then swaps it in, keeping the original
//! until the swap is confirmed (and putting it back if anything fails).

use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use rusqlite::{Connection, ErrorCode};
use zeroize::{Zeroize, Zeroizing};

use crate::error::{Result, StoreError};

/// What every plain SQLite file starts with. An encrypted file starts with random bytes.
const PLAIN_HEADER: &[u8; 16] = b"SQLite format 3\0";

fn invalid(message: impl Into<String>) -> StoreError {
    StoreError::Invalid(message.into())
}

// ------------------------------------------------------------------ keys

/// 32 random bytes. Wiped on drop; never printed.
pub struct RawKey([u8; 32]);

impl RawKey {
    pub fn generate() -> Result<Self> {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes)
            .map_err(|e| invalid(format!("no secure random source: {e}")))?;
        Ok(Self(bytes))
    }

    /// 64 hex digits; dashes, spaces and case are ignored (a recovery key as it was shown).
    pub fn from_hex(text: &str) -> Option<Self> {
        let digits: Vec<u8> = text
            .bytes()
            .filter(|b| !matches!(b, b'-' | b' ' | b'\t' | b'\n' | b'\r'))
            .collect();
        if digits.len() != 64 {
            return None;
        }
        let mut bytes = [0u8; 32];
        for (i, pair) in digits.chunks(2).enumerate() {
            let hi = (pair[0] as char).to_digit(16)?;
            let lo = (pair[1] as char).to_digit(16)?;
            bytes[i] = (hi * 16 + lo) as u8;
        }
        Some(Self(bytes))
    }

    /// The key as 64 lowercase hex digits.
    pub fn to_hex(&self) -> Zeroizing<String> {
        let mut out = Zeroizing::new(String::with_capacity(64));
        for b in self.0 {
            out.push_str(&format!("{b:02x}"));
        }
        out
    }

    /// Hex in groups of four, for a person to copy down: `1a2b-3c4d-…`.
    pub fn to_recovery_text(&self) -> Zeroizing<String> {
        let hex = self.to_hex();
        let groups: Vec<&str> = (0..16).map(|i| &hex[i * 4..i * 4 + 4]).collect();
        Zeroizing::new(groups.join("-"))
    }
}

impl Clone for RawKey {
    fn clone(&self) -> Self {
        Self(self.0)
    }
}

impl PartialEq for RawKey {
    fn eq(&self, other: &Self) -> bool {
        // Constant time: no early exit on the first differing byte.
        self.0
            .iter()
            .zip(other.0.iter())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
    }
}

impl Drop for RawKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl std::fmt::Debug for RawKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RawKey(<redacted>)")
    }
}

/// A passphrase. Wiped on drop; never printed.
pub struct Passphrase(String);

impl Passphrase {
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Clone for Passphrase {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl PartialEq for Passphrase {
    fn eq(&self, other: &Self) -> bool {
        let (a, b) = (self.0.as_bytes(), other.0.as_bytes());
        a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
    }
}

impl Drop for Passphrase {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl std::fmt::Debug for Passphrase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Passphrase(<redacted>)")
    }
}

/// How a database file is (or is to be) keyed.
#[derive(Clone, PartialEq, Debug)]
pub enum Key {
    /// Not encrypted.
    None,
    Raw(RawKey),
    Passphrase(Passphrase),
}

impl Key {
    pub fn is_none(&self) -> bool {
        matches!(self, Key::None)
    }

    /// The value for `PRAGMA key` / `ATTACH ... KEY`: `x'hex'` for a raw key (SQLCipher's syntax
    /// for "use these bytes as they are"), the text itself for a passphrase, empty for none.
    fn pragma_value(&self) -> Zeroizing<String> {
        match self {
            Key::None => Zeroizing::new(String::new()),
            Key::Raw(k) => Zeroizing::new(format!("x'{}'", k.to_hex().as_str())),
            Key::Passphrase(p) => Zeroizing::new(p.as_str().to_owned()),
        }
    }
}

// ------------------------------------------------------------------ files

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileState {
    /// No file, or an empty one: a new database will be created.
    Missing,
    Plain,
    /// Doesn't start with the SQLite header: encrypted (or damaged).
    Encrypted,
}

fn read_header(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(16);
    fs::File::open(path)?.take(16).read_to_end(&mut buf)?;
    Ok(buf)
}

pub fn file_state(path: &Path) -> Result<FileState> {
    match read_header(path) {
        Ok(h) if h.is_empty() => Ok(FileState::Missing),
        Ok(h) if h == PLAIN_HEADER => Ok(FileState::Plain),
        Ok(_) => Ok(FileState::Encrypted),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(FileState::Missing),
        Err(e) => Err(invalid(format!("Couldn't read {}: {e}", path.display()))),
    }
}

/// Whether a file (a backup) is encrypted. Unreadable files count as encrypted, the cautious
/// answer.
pub fn file_is_encrypted(path: &Path) -> bool {
    !matches!(file_state(path), Ok(FileState::Plain))
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".");
    name.push(suffix);
    PathBuf::from(name)
}

fn side_files(path: &Path) -> [PathBuf; 2] {
    let with = |s: &str| {
        let mut name = path.as_os_str().to_owned();
        name.push(s);
        PathBuf::from(name)
    };
    [with("-wal"), with("-shm")]
}

/// Overwrites a file with zeros and deletes it. Best effort: it cannot reach copies a journaling
/// or flash file system keeps, which is one more reason to encrypt before data accumulates.
pub fn secure_remove(path: &Path) {
    if let Ok(meta) = fs::metadata(path) {
        if meta.is_file() {
            if let Ok(mut f) = fs::OpenOptions::new().write(true).open(path) {
                let zeros = [0u8; 8192];
                let mut left = meta.len();
                while left > 0 {
                    let n = left.min(zeros.len() as u64) as usize;
                    if f.write_all(&zeros[..n]).is_err() {
                        break;
                    }
                    left -= n as u64;
                }
                let _ = f.sync_all();
            }
        }
    }
    let _ = fs::remove_file(path);
}

/// Owner-only permissions for files holding private data (no effect off Unix).
pub fn restrict_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    let _ = path;
}

// ------------------------------------------------------------------ connections

/// SQLCipher prints "error decrypting page" to stderr whenever a key is tried and fails, which
/// is a normal event here (checking a recovery key, a wrong passphrase). Silence it.
fn quiet(conn: &Connection) {
    let _ = conn.pragma_update(None, "cipher_log_level", "NONE");
}

/// Applies `key` to a fresh connection. Nothing is read yet, so a wrong key is only noticed by
/// [`verify`].
pub(crate) fn apply(conn: &Connection, key: &Key) -> Result<()> {
    if key.is_none() {
        return Ok(());
    }
    // An empty passphrase would mean "no encryption" to SQLCipher; it is never a key.
    if matches!(key, Key::Passphrase(p) if p.as_str().is_empty()) {
        return Err(StoreError::WrongKey);
    }
    let value = key.pragma_value();
    conn.pragma_update(None, "key", value.as_str())?;
    Ok(())
}

/// Reads the schema: the first thing that fails with the wrong key (or on a non-database).
pub(crate) fn verify(conn: &Connection) -> Result<()> {
    match conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
        r.get::<_, i64>(0)
    }) {
        Ok(_) => Ok(()),
        Err(rusqlite::Error::SqliteFailure(f, _)) if f.code == ErrorCode::NotADatabase => {
            Err(StoreError::WrongKey)
        }
        Err(e) => Err(e.into()),
    }
}

/// Opens `path` read-write with `key` and checks that the key works.
pub(crate) fn connect(path: &Path, key: &Key) -> Result<Connection> {
    let conn = Connection::open(path)?;
    quiet(&conn);
    apply(&conn, key)?;
    verify(&conn)?;
    Ok(conn)
}

// ------------------------------------------------------------------ changing the key

fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Writes all of `source` into a new file `dest` keyed with `key` (plain for `Key::None`).
fn build_copy(source: &Connection, dest: &Path, key: &Key) -> Result<()> {
    if matches!(key, Key::Passphrase(p) if p.as_str().is_empty()) {
        return Err(StoreError::WrongKey);
    }
    let value = key.pragma_value();
    let user_version: i64 = source.query_row("PRAGMA main.user_version", [], |r| r.get(0))?;
    source.execute(
        "ATTACH DATABASE ?1 AS reencrypted KEY ?2",
        rusqlite::params![dest.to_string_lossy(), value.as_str()],
    )?;
    let exported = (|| -> Result<()> {
        source.query_row("SELECT sqlcipher_export('reencrypted')", [], |_| Ok(()))?;
        // The export does not carry the schema version.
        source.execute_batch(&format!("PRAGMA reencrypted.user_version = {user_version}"))?;
        Ok(())
    })();
    let detached = source.execute("DETACH DATABASE reencrypted", []);
    exported?;
    detached?;
    Ok(())
}

/// Checks that the copy at `copy` (opened with `key`) holds exactly what `source` holds.
fn verify_copy(source: &Connection, copy: &Path, key: &Key) -> Result<()> {
    let wrong = |what: String| {
        invalid(format!(
            "The re-encrypted copy failed its check ({what}); nothing was changed"
        ))
    };
    let opened = connect(copy, key).map_err(|_| wrong("it does not open".into()))?;
    let integrity: String = opened.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    if integrity != "ok" {
        return Err(wrong(format!("integrity: {integrity}")));
    }
    let a: i64 = source.query_row("PRAGMA main.user_version", [], |r| r.get(0))?;
    let b: i64 = opened.query_row("PRAGMA main.user_version", [], |r| r.get(0))?;
    if a != b {
        return Err(wrong("schema version".into()));
    }
    let tables: Vec<String> = source
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    for table in &tables {
        let sql = format!("SELECT count(*) FROM {}", quote_ident(table));
        let n: i64 = source.query_row(&sql, [], |r| r.get(0))?;
        let m: i64 = opened
            .query_row(&sql, [], |r| r.get(0))
            .map_err(|_| wrong(format!("table {table} is missing")))?;
        if n != m {
            return Err(wrong(format!("table {table} has {m} rows, expected {n}")));
        }
    }
    let objects = |c: &Connection| -> Result<Vec<String>> {
        Ok(
            c.prepare("SELECT type || ':' || name FROM sqlite_master ORDER BY 1")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?,
        )
    };
    if objects(source)? != objects(&opened)? {
        return Err(wrong("schema objects differ".into()));
    }
    Ok(())
}

/// Replaces the live database file with `fresh` (already built and checked, keyed with `new`),
/// reopening `conn` on it. The old file is kept as `<db>.old` until the new one has opened, and
/// put back if anything fails. On success the old file (and its WAL files) are wiped.
fn swap_in(
    conn: &mut Connection,
    path: &Path,
    fresh: &Path,
    current: &Key,
    new: &Key,
) -> Result<()> {
    let old = sibling(path, "old");
    // Fold the WAL into the main file so nothing is left behind, then let go of the file.
    let _ = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()));
    let placeholder = Connection::open_in_memory()?;
    let live = std::mem::replace(conn, placeholder);
    if let Err((live, e)) = live.close() {
        *conn = live;
        secure_remove(fresh);
        return Err(e.into());
    }
    let restore_old = |conn: &mut Connection| -> Result<()> {
        for f in side_files(path) {
            let _ = fs::remove_file(f);
        }
        let _ = fs::remove_file(path);
        fs::rename(&old, path)
            .map_err(|e| invalid(format!("Couldn't put the original database back: {e}")))?;
        *conn = crate::open_with_key_unchecked(path, current)?;
        Ok(())
    };
    for f in side_files(path) {
        let _ = fs::remove_file(f);
    }
    if let Err(e) = fs::rename(path, &old) {
        secure_remove(fresh);
        *conn = crate::open_with_key_unchecked(path, current)?;
        return Err(invalid(format!("Couldn't move the database aside: {e}")));
    }
    if let Err(e) = fs::rename(fresh, path) {
        secure_remove(fresh);
        restore_old(conn)?;
        return Err(invalid(format!(
            "Couldn't put the new database in place: {e}"
        )));
    }
    restore_permissions(path);
    match crate::open_with_key_unchecked(path, new) {
        Ok(opened) => {
            *conn = opened;
            secure_remove(&old);
            for f in side_files(&old) {
                secure_remove(&f);
            }
            Ok(())
        }
        Err(e) => {
            secure_remove(path);
            restore_old(conn)?;
            Err(e)
        }
    }
}

fn restore_permissions(path: &Path) {
    restrict_permissions(path);
}

/// Replaces the live database with a copy of `source` (the live data itself when `None`) keyed
/// with `new`. Verified before anything is touched; `current` is the key the live file has now.
pub fn replace_live(
    conn: &mut Connection,
    source: Option<&Connection>,
    current: &Key,
    new: &Key,
) -> Result<()> {
    let path = conn
        .path()
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| invalid("An in-memory database can't be re-encrypted"))?;
    let fresh = sibling(&path, "reencrypting");
    secure_remove(&fresh);
    {
        let from: &Connection = source.unwrap_or(&*conn);
        let built = build_copy(from, &fresh, new).and_then(|()| verify_copy(from, &fresh, new));
        if let Err(e) = built {
            secure_remove(&fresh);
            return Err(e);
        }
    }
    swap_in(conn, &path, &fresh, current, new)
}

/// Changes how the live database is keyed: plain to encrypted, encrypted to plain, or to another
/// key. Nothing changes unless the new copy checks out.
pub fn rekey(conn: &mut Connection, current: &Key, new: &Key) -> Result<()> {
    replace_live(conn, None, current, new)
}

/// Opens `<dir>/minimap.db` with a recovery key (tests of the layers above use this to check a
/// key does or does not open the file).
#[doc(hidden)]
pub fn open_with_key_for_tests(dir: &Path, recovery_key: &str) -> Result<Connection> {
    let raw = RawKey::from_hex(recovery_key).ok_or_else(|| invalid("not a key"))?;
    crate::open_with_key(&dir.join("minimap.db"), &Key::Raw(raw))
}

/// If a previous run was interrupted between moving the old database aside and putting the new
/// one in place, puts the old one back. Run before opening the database.
pub fn recover_interrupted(path: &Path) {
    let old = sibling(path, "old");
    let fresh = sibling(path, "reencrypting");
    if !path.exists() && old.exists() {
        let _ = fs::rename(&old, path);
    }
    // A half-built copy is never usable.
    secure_remove(&fresh);
}

/// After the database opened normally: a leftover `.old` from an interrupted swap that did
/// finish is wiped (it would otherwise be an unprotected copy of the data).
pub fn remove_leftovers(path: &Path) {
    let old = sibling(path, "old");
    if old.exists() && path.exists() {
        secure_remove(&old);
        for f in side_files(&old) {
            secure_remove(&f);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        backup,
        test_support::{add_person, add_task, fingerprint, temp_dir},
        {open, open_with_key, tasks, LATEST_SCHEMA},
    };
    use minimap_types::BackupKind;

    fn raw() -> Key {
        Key::Raw(RawKey::generate().unwrap())
    }

    fn pass(text: &str) -> Key {
        Key::Passphrase(Passphrase::new(text))
    }

    /// A database with the sort of data the app holds, in WAL mode.
    fn populated(dir: &Path) -> (PathBuf, Connection) {
        let db = dir.join("minimap.db");
        let mut conn = open(&db).unwrap();
        add_person(&mut conn, "Priya");
        for t in ["Ship the confidential launch", "Write docs", "Plan Q3"] {
            add_task(&mut conn, t);
        }
        (db, conn)
    }

    fn bytes_contain(path: &Path, needle: &str) -> bool {
        fs::read(path)
            .unwrap()
            .windows(needle.len())
            .any(|w| w == needle.as_bytes())
    }

    fn leftovers(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".old") || n.contains("reencrypting"))
            .collect();
        names.sort();
        names
    }

    #[test]
    fn keys_round_trip_through_hex_and_never_print() {
        let k = RawKey::generate().unwrap();
        let hex = k.to_hex();
        assert_eq!(hex.len(), 64);
        let again = RawKey::from_hex(hex.as_str()).unwrap();
        assert!(k == again);
        // A recovery key as a person copies it: groups, dashes, capitals, stray spaces.
        let shown = k.to_recovery_text();
        assert_eq!(shown.split('-').count(), 16);
        assert!(shown.split('-').all(|g| g.len() == 4));
        let typed = format!("  {} \n", shown.to_uppercase());
        assert!(RawKey::from_hex(&typed).unwrap() == k);
        assert!(RawKey::from_hex("abc").is_none());
        assert!(RawKey::from_hex(&"g".repeat(64)).is_none());
        assert!(RawKey::generate().unwrap() != RawKey::generate().unwrap());
        // Debug output hides everything.
        let shown_debug = format!(
            "{k:?} {:?} {:?}",
            pass("hunter2 hunter2"),
            Key::Raw(k.clone())
        );
        assert!(!shown_debug.contains(hex.as_str()) && !shown_debug.contains("hunter2"));
        assert!(shown_debug.contains("redacted"));
        assert!(pass("same") == pass("same") && pass("same") != pass("other"));
    }

    #[test]
    fn a_plain_file_is_plain_an_encrypted_one_is_not_and_a_missing_one_is_new() {
        let dir = temp_dir("filestate");
        let db = dir.join("minimap.db");
        assert_eq!(file_state(&db).unwrap(), FileState::Missing);
        fs::write(&db, b"").unwrap();
        assert_eq!(file_state(&db).unwrap(), FileState::Missing);
        let (db, mut conn) = populated(&dir);
        conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
            .unwrap();
        assert_eq!(file_state(&db).unwrap(), FileState::Plain);
        assert!(
            bytes_contain(&db, "confidential"),
            "plain files show their contents"
        );
        rekey(&mut conn, &Key::None, &raw()).unwrap();
        assert_eq!(file_state(&db).unwrap(), FileState::Encrypted);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn encrypting_an_existing_database_loses_nothing_and_hides_the_file() {
        let dir = temp_dir("encrypt");
        let (db, mut conn) = populated(&dir);
        let before = fingerprint(&conn);
        let key = raw();
        rekey(&mut conn, &Key::None, &key).unwrap();

        // The connection works as before, on the encrypted file.
        assert_eq!(fingerprint(&conn), before, "no data was lost or changed");
        add_task(&mut conn, "Written after encrypting");
        assert_eq!(tasks::list(&conn, false).unwrap().len(), 4);
        assert_eq!(crate::schema_version(&conn).unwrap(), LATEST_SCHEMA);
        let hits = crate::search::run(&conn, "\"docs\"*", &[], false, 10).unwrap();
        assert_eq!(hits.len(), 1, "full-text search came across");
        drop(conn);

        // The file reveals nothing and does not open without the key.
        for needle in ["confidential", "Priya", "SQLite format"] {
            assert!(
                !bytes_contain(&db, needle),
                "{needle} is readable in the file"
            );
        }
        assert!(matches!(open(&db), Err(StoreError::WrongKey)), "no key");
        assert!(
            matches!(open_with_key(&db, &raw()), Err(StoreError::WrongKey)),
            "wrong key"
        );
        assert!(matches!(
            open_with_key(&db, &pass("guess guess guess")),
            Err(StoreError::WrongKey)
        ));
        // The right key opens it with everything in place.
        let reopened = open_with_key(&db, &key).unwrap();
        assert_eq!(tasks::list(&reopened, false).unwrap().len(), 4);
        // Nothing unprotected is left behind.
        assert!(leftovers(&dir).is_empty(), "{:?}", leftovers(&dir));
        let plain_files: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.is_file() && matches!(file_state(p), Ok(FileState::Plain)))
            .collect();
        assert!(plain_files.is_empty(), "{plain_files:?}");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn every_way_of_keying_round_trips_and_decrypting_restores_a_plain_file() {
        let dir = temp_dir("transitions");
        let (db, mut conn) = populated(&dir);
        let before = fingerprint(&conn);
        let odd = pass("it's a \"pass\" phrase: ünïcode ✓ 12345");
        let (k1, k2) = (raw(), raw());
        let mut current = Key::None;
        for next in [
            odd.clone(),
            k1.clone(),
            odd.clone(),
            k2.clone(),
            Key::None,
            k1.clone(),
        ] {
            rekey(&mut conn, &current, &next).unwrap();
            assert_eq!(fingerprint(&conn), before);
            assert_eq!(file_state(&db).unwrap() == FileState::Plain, next.is_none());
            current = next;
        }
        drop(conn);
        // Only the key it ended with opens it.
        assert!(matches!(open_with_key(&db, &k2), Err(StoreError::WrongKey)));
        assert!(matches!(
            open_with_key(&db, &odd),
            Err(StoreError::WrongKey)
        ));
        let conn = open_with_key(&db, &k1).unwrap();
        assert_eq!(fingerprint(&conn), before);
        assert!(leftovers(&dir).is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_failed_change_leaves_the_database_exactly_as_it_was() {
        let dir = temp_dir("failure");
        let (db, mut conn) = populated(&dir);
        let before = fingerprint(&conn);
        // Something is in the way of the new file.
        fs::create_dir(sibling(&db, "reencrypting")).unwrap();
        let err = rekey(&mut conn, &Key::None, &raw());
        assert!(err.is_err());
        assert_eq!(
            fingerprint(&conn),
            before,
            "the live connection still works"
        );
        add_task(&mut conn, "Still writable");
        assert_eq!(
            file_state(&db).unwrap(),
            FileState::Plain,
            "and the file is still plain"
        );
        // A database that lives only in memory can't be re-keyed.
        let mut memory = crate::open_in_memory().unwrap();
        assert!(rekey(&mut memory, &Key::None, &raw()).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_interrupted_swap_is_undone_on_the_next_start() {
        let dir = temp_dir("interrupted");
        let (db, conn) = populated(&dir);
        drop(conn);
        // The crash happened after the old file was moved aside and before the new one landed.
        let old = sibling(&db, "old");
        fs::rename(&db, &old).unwrap();
        fs::write(sibling(&db, "reencrypting"), b"half a file").unwrap();
        assert!(!db.exists());
        recover_interrupted(&db);
        assert!(db.exists() && !old.exists());
        assert!(!sibling(&db, "reencrypting").exists());
        assert_eq!(tasks::list(&open(&db).unwrap(), false).unwrap().len(), 3);
        // A leftover `.old` beside a working database is wiped.
        fs::write(&old, b"plaintext copy").unwrap();
        remove_leftovers(&db);
        assert!(!old.exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn secure_remove_deletes_and_ignores_what_is_not_there() {
        let dir = temp_dir("wipe");
        let f = dir.join("secret.txt");
        fs::write(&f, vec![7u8; 20_000]).unwrap();
        secure_remove(&f);
        assert!(!f.exists());
        secure_remove(&f);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn backups_of_an_encrypted_database_are_encrypted_and_never_plain() {
        let dir = temp_dir("encbackup");
        let (db, mut conn) = populated(&dir);
        let key = raw();
        rekey(&mut conn, &Key::None, &key).unwrap();
        let backups = dir.join("backups");
        let made = backup::create(&conn, &backups, BackupKind::Manual, 0, &key).unwrap();
        assert!(made.encrypted);
        assert!(!bytes_contain(Path::new(&made.path), "confidential"));
        // Asking for a plain copy of encrypted data is refused rather than leaking it.
        assert!(backup::create(&conn, &backups, BackupKind::Manual, 0, &Key::None).is_err());
        assert_eq!(
            backup::list(&backups).unwrap().len(),
            1,
            "and leaves no file behind"
        );
        // The backup restores with the same key, and keeps the database encrypted.
        add_task(&mut conn, "Added after the backup");
        backup::restore(&mut conn, Path::new(&made.path), &backups, &key, &[]).unwrap();
        assert_eq!(tasks::list(&conn, false).unwrap().len(), 3);
        drop(conn);
        assert_eq!(file_state(&db).unwrap(), FileState::Encrypted);
        assert_eq!(
            tasks::list(&open_with_key(&db, &key).unwrap(), false)
                .unwrap()
                .len(),
            3
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_backup_of_another_key_needs_that_key_and_nothing_changes_without_it() {
        let dir = temp_dir("otherkey");
        let (_db, mut conn) = populated(&dir);
        let old_key = pass("the passphrase I used before");
        rekey(&mut conn, &Key::None, &old_key).unwrap();
        let backups = dir.join("backups");
        let made = backup::create(&conn, &backups, BackupKind::Manual, 0, &old_key).unwrap();
        let new_key = raw();
        rekey(&mut conn, &old_key, &new_key).unwrap();
        add_task(&mut conn, "Newer work");
        let before = fingerprint(&conn);
        let path = Path::new(&made.path);

        let e = backup::restore(&mut conn, path, &backups, &new_key, &[]).unwrap_err();
        assert!(matches!(e, StoreError::BackupKeyNeeded), "{e:?}");
        let e = backup::restore(
            &mut conn,
            path,
            &backups,
            &new_key,
            &[pass("not it, sorry")],
        )
        .unwrap_err();
        assert!(matches!(e, StoreError::WrongKey), "{e:?}");
        assert_eq!(fingerprint(&conn), before, "refusals change nothing");
        assert_eq!(
            backup::list(&backups).unwrap().len(),
            1,
            "and save no pre-restore copy"
        );

        let preview = backup::inspect(
            path,
            LATEST_SCHEMA,
            &new_key,
            std::slice::from_ref(&old_key),
        )
        .unwrap();
        assert!(preview.encrypted);
        backup::restore(
            &mut conn,
            path,
            &backups,
            &new_key,
            std::slice::from_ref(&old_key),
        )
        .unwrap();
        assert_eq!(
            tasks::list(&conn, false).unwrap().len(),
            3,
            "back to the older data"
        );
        // The live database kept its own (new) key, not the backup's.
        drop(conn);
        assert!(matches!(
            open_with_key(&dir.join("minimap.db"), &old_key),
            Err(StoreError::WrongKey)
        ));
        assert!(open_with_key(&dir.join("minimap.db"), &new_key).is_ok());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn plain_and_encrypted_backups_restore_into_the_other_kind_of_database() {
        let dir = temp_dir("mixed");
        let (db, mut conn) = populated(&dir);
        let backups = dir.join("backups");
        let plain_backup =
            backup::create(&conn, &backups, BackupKind::Manual, 0, &Key::None).unwrap();
        assert!(!plain_backup.encrypted);
        let plain_state = fingerprint(&conn);
        let key = raw();
        rekey(&mut conn, &Key::None, &key).unwrap();
        add_task(&mut conn, "Only in the encrypted database");

        // An old plain backup comes back into the encrypted database: data restored, still encrypted.
        backup::restore(
            &mut conn,
            Path::new(&plain_backup.path),
            &backups,
            &key,
            &[],
        )
        .unwrap();
        assert_eq!(tasks::list(&conn, false).unwrap().len(), 3);
        // (Settings differ by the backup bookkeeping the restore itself writes.)
        let restored = fingerprint(&conn);
        assert_eq!(
            restored.split("settings:").next(),
            plain_state.split("settings:").next(),
            "everything but settings matches the old backup"
        );
        drop(conn);
        assert_eq!(file_state(&db).unwrap(), FileState::Encrypted);
        let mut conn = open_with_key(&db, &key).unwrap();
        assert!(!bytes_contain(&db, "confidential"));

        // And an encrypted backup comes back into a database that has since been decrypted.
        let enc_backup = backup::create(&conn, &backups, BackupKind::Manual, 0, &key).unwrap();
        rekey(&mut conn, &key, &Key::None).unwrap();
        add_task(&mut conn, "Only in the plain database");
        backup::restore(
            &mut conn,
            Path::new(&enc_backup.path),
            &backups,
            &Key::None,
            std::slice::from_ref(&key),
        )
        .unwrap();
        assert_eq!(tasks::list(&conn, false).unwrap().len(), 3);
        drop(conn);
        assert_eq!(file_state(&db).unwrap(), FileState::Plain);
        assert!(leftovers(&dir).is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn upgrading_an_encrypted_database_saves_an_encrypted_copy_first() {
        let dir = temp_dir("encmigrate");
        let db = dir.join("minimap.db");
        let key = raw();
        {
            let mut conn = Connection::open(&db).unwrap();
            apply(&conn, &key).unwrap();
            crate::register_functions(&conn).unwrap();
            crate::migrations().to_version(&mut conn, 5).unwrap();
        }
        let conn = open_with_key(&db, &key).unwrap();
        assert_eq!(crate::schema_version(&conn).unwrap(), LATEST_SCHEMA);
        let saved = backup::list(&dir.join("backups")).unwrap();
        assert_eq!(saved.len(), 1);
        assert!(saved[0].encrypted, "the pre-migration copy is not plain");
        fs::remove_dir_all(&dir).unwrap();
    }
}
