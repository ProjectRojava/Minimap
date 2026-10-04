//! Encryption at rest (spec 21): the unlock screen's command, turning encryption on, changing how
//! the key is kept, turning it off, and removing backups that were made before encryption.
//!
//! Nothing here logs a key, a passphrase or a recovery key.

use std::{path::Path, time::Duration};

use minimap_store::security::{self, FileState, Key, Passphrase, RawKey};
use minimap_types::{
    AppError, BackupKind, EncryptionChoice, EncryptionResult, KeyMethod, Secret, SecurityStatus,
    SetEncryption, MIN_PASSPHRASE_CHARS,
};
use tauri::State;

use crate::{
    commands::backup::backup_folder,
    error::{app_error, store_error},
    keystore::{KeyStore, Slot},
    state::{locked, AppState, Vault},
};

/// Longest accepted passphrase, in characters.
const MAX_PASSPHRASE_CHARS: usize = 512;

/// How long a wrong key makes the caller wait, so guessing through the app is slow.
const WRONG_KEY_DELAY: Duration = Duration::from_millis(750);

/// Opens the database at start-up. A plain database just opens. An encrypted one is opened with
/// the key from the keychain (either slot); when that is not possible the app starts locked and
/// asks for the passphrase or recovery key.
pub fn boot(data_dir: &Path, keys: &dyn KeyStore) -> Result<Vault, AppError> {
    let path = data_dir.join("minimap.db");
    security::recover_interrupted(&path);
    let vault = match security::file_state(&path).map_err(store_error)? {
        FileState::Missing | FileState::Plain => {
            let conn = minimap_store::open(&path).map_err(store_error)?;
            Vault::open(conn, Key::None, None)
        }
        FileState::Encrypted => boot_encrypted(&path, keys)?,
    };
    if vault.conn.is_some() {
        security::remove_leftovers(&path);
    }
    Ok(vault)
}

fn boot_encrypted(path: &Path, keys: &dyn KeyStore) -> Result<Vault, AppError> {
    for slot in [Slot::Current, Slot::Pending] {
        // A keychain that can't be read is not fatal: ask the user instead.
        let Ok(Some(raw)) = keys.get(slot) else {
            continue;
        };
        let key = Key::Raw(raw);
        match minimap_store::open_with_key(path, &key) {
            Ok(conn) => {
                if slot == Slot::Pending {
                    // An interrupted key change: the pending key is the real one. Make it current.
                    if let Key::Raw(raw) = &key {
                        if keys.set(Slot::Current, raw).is_ok() {
                            let _ = keys.delete(Slot::Pending);
                        }
                    }
                }
                return Ok(Vault::open(conn, key, Some(KeyMethod::Keychain)));
            }
            Err(minimap_store::StoreError::WrongKey) => continue,
            Err(e) => return Err(store_error(e)),
        }
    }
    Ok(Vault::locked())
}

pub(crate) fn status_impl(vault: &Vault, keys: &dyn KeyStore, data_dir: &Path) -> SecurityStatus {
    let availability = keys.availability();
    let locked = vault.conn.is_none();
    let encrypted = locked || !vault.key.is_none();
    let unencrypted_backups = match (&vault.conn, encrypted) {
        (Some(conn), true) => backup_folder(conn, data_dir)
            .and_then(|(folder, _)| minimap_store::backup::list(&folder).map_err(store_error))
            .map(|list| list.iter().filter(|b| !b.encrypted).count() as u32)
            .unwrap_or(0),
        _ => 0,
    };
    SecurityStatus {
        locked,
        encrypted,
        method: if locked { None } else { vault.method },
        keychain_available: availability.is_ok(),
        keychain_problem: availability.err(),
        unencrypted_backups,
        min_passphrase_chars: MIN_PASSPHRASE_CHARS,
    }
}

/// Whether the database is locked, how it is protected, and whether a keychain can be used.
/// Works while the database is locked.
#[tauri::command]
pub async fn get_security_status(state: State<'_, AppState>) -> Result<SecurityStatus, AppError> {
    let data_dir = state.data_dir.clone();
    state
        .run_vault(move |vault, keys| Ok(status_impl(vault, keys, &data_dir)))
        .await
}

/// The keys a typed secret could be: a passphrase, and, if it looks like one, a recovery key.
pub(crate) fn candidates(secret: &str) -> Vec<Key> {
    let mut found = vec![Key::Passphrase(Passphrase::new(secret))];
    if let Some(raw) = RawKey::from_hex(secret) {
        found.push(Key::Raw(raw));
    }
    found
}

/// Opens a locked database with a passphrase or a recovery key.
#[tauri::command]
pub async fn unlock_database(
    state: State<'_, AppState>,
    secret: Secret,
) -> Result<SecurityStatus, AppError> {
    let data_dir = state.data_dir.clone();
    state
        .run_vault(move |vault, keys| {
            unlock_impl(vault, keys, &data_dir, &secret.0)?;
            Ok(status_impl(vault, keys, &data_dir))
        })
        .await
}

pub(crate) fn unlock_impl(
    vault: &mut Vault,
    keys: &dyn KeyStore,
    data_dir: &Path,
    secret: &str,
) -> Result<(), AppError> {
    if vault.conn.is_some() {
        return Ok(());
    }
    let path = data_dir.join("minimap.db");
    for key in candidates(secret) {
        match minimap_store::open_with_key(&path, &key) {
            Ok(conn) => {
                let method = match &key {
                    Key::Passphrase(_) => Some(KeyMethod::Passphrase),
                    // A recovery key: put it back in the keychain if one is usable, so the
                    // next start opens by itself again.
                    Key::Raw(raw) => repair_keychain(keys, raw),
                    Key::None => None,
                };
                vault.conn = Some(conn);
                vault.key = key;
                vault.method = method;
                security::remove_leftovers(&path);
                tracing::info!("database unlocked");
                return Ok(());
            }
            Err(minimap_store::StoreError::WrongKey) => continue,
            Err(e) => return Err(store_error(e)),
        }
    }
    std::thread::sleep(WRONG_KEY_DELAY);
    Err(app_error(
        "wrong_key",
        "That isn't the right passphrase or recovery key",
    ))
}

/// Stores `raw` as the current key and checks it reads back. `Some(Keychain)` when it is safe.
fn repair_keychain(keys: &dyn KeyStore, raw: &RawKey) -> Option<KeyMethod> {
    keys.availability().ok()?;
    keys.set(Slot::Current, raw).ok()?;
    match keys.get(Slot::Current) {
        Ok(Some(back)) if back == *raw => Some(KeyMethod::Keychain),
        _ => None,
    }
}

fn check_passphrase(text: Option<&Secret>) -> Result<Passphrase, AppError> {
    let text = text
        .map(|s| s.0.as_str())
        .ok_or_else(|| app_error("invalid", "Choose a passphrase"))?;
    let chars = text.chars().count();
    if chars < MIN_PASSPHRASE_CHARS as usize {
        return Err(app_error(
            "invalid",
            format!("The passphrase needs at least {MIN_PASSPHRASE_CHARS} characters"),
        ));
    }
    if chars > MAX_PASSPHRASE_CHARS {
        return Err(app_error(
            "invalid",
            format!("The passphrase can be at most {MAX_PASSPHRASE_CHARS} characters"),
        ));
    }
    Ok(Passphrase::new(text))
}

/// Turns encryption on, changes how the key is kept (keychain or passphrase, a new keychain key
/// is a new recovery key) or turns it off. The data is copied into a new file with the new key,
/// the copy is checked against the original, and only then does it replace it; a backup is taken
/// first. Returns the recovery key, once, when the keychain now holds the key.
#[tauri::command]
pub async fn set_encryption(
    state: State<'_, AppState>,
    request: SetEncryption,
) -> Result<EncryptionResult, AppError> {
    let data_dir = state.data_dir.clone();
    state
        .run_vault(move |vault, keys| set_encryption_impl(vault, keys, &data_dir, request))
        .await
}

pub(crate) fn set_encryption_impl(
    vault: &mut Vault,
    keys: &dyn KeyStore,
    data_dir: &Path,
    request: SetEncryption,
) -> Result<EncryptionResult, AppError> {
    if vault.conn.is_none() {
        return Err(locked());
    }
    let current = vault.key.clone();

    // Decide the new key before touching anything.
    let (new_key, in_keychain) = match request.choice {
        EncryptionChoice::Off => {
            if current.is_none() {
                return Err(app_error("invalid", "Encryption is already off"));
            }
            if !request.confirmed {
                return Err(app_error(
                    "invalid",
                    "Confirm that you want to turn encryption off",
                ));
            }
            if let Key::Passphrase(p) = &current {
                let typed = Passphrase::new(
                    request
                        .current_passphrase
                        .as_ref()
                        .map_or("", |s| s.0.as_str()),
                );
                if typed != *p {
                    std::thread::sleep(WRONG_KEY_DELAY);
                    return Err(app_error(
                        "wrong_key",
                        "Enter your current passphrase to turn encryption off",
                    ));
                }
            }
            (Key::None, false)
        }
        EncryptionChoice::Keychain => {
            keys.availability()
                .map_err(|why| app_error("keychain_unavailable", why))?;
            let raw = RawKey::generate().map_err(store_error)?;
            (Key::Raw(raw), true)
        }
        EncryptionChoice::Passphrase => (
            Key::Passphrase(check_passphrase(request.passphrase.as_ref())?),
            false,
        ),
    };

    let (conn, _) = vault.parts()?;
    let (folder, _) = backup_folder(conn, data_dir)?;
    let version = minimap_store::schema_version(conn).map_err(store_error)?;

    // The rule: a backup before changing the key. It is taken with the current key and removed
    // once the change has succeeded (it would otherwise be a copy under the old protection).
    let safety =
        minimap_store::backup::create(conn, &folder, BackupKind::PreEncryption, version, &current)
            .map_err(store_error)?;

    // A new keychain key goes to the pending slot first and must read back identically.
    if let Key::Raw(raw) = &new_key {
        let stored = keys
            .set(Slot::Pending, raw)
            .and_then(|()| match keys.get(Slot::Pending) {
                Ok(Some(back)) if back == *raw => Ok(()),
                Ok(_) => Err("The keychain did not keep the key".to_owned()),
                Err(e) => Err(e),
            });
        if let Err(why) = stored {
            let _ = keys.delete(Slot::Pending);
            security::secure_remove(Path::new(&safety.path));
            return Err(app_error(
                "keychain_unavailable",
                format!("Encryption was not changed. {why}"),
            ));
        }
    }

    let rekeyed = security::rekey(vault.conn.as_mut().ok_or_else(locked)?, &current, &new_key);
    if let Err(e) = rekeyed {
        let _ = keys.delete(Slot::Pending);
        let mut err = store_error(e);
        err.message = format!(
            "Encryption was not changed; your data is as it was. {} (a backup was kept as {})",
            err.message, safety.file_name
        );
        return Err(err);
    }

    // The database now has the new key.
    vault.method = match &new_key {
        Key::Raw(raw) => match keys.set(Slot::Current, raw) {
            Ok(()) => {
                let _ = keys.delete(Slot::Pending);
                Some(KeyMethod::Keychain)
            }
            // The pending slot still holds it and start-up knows to look there.
            Err(_) => Some(KeyMethod::Keychain),
        },
        Key::Passphrase(_) => {
            clear_keychain(keys);
            Some(KeyMethod::Passphrase)
        }
        Key::None => {
            clear_keychain(keys);
            None
        }
    };
    vault.key = new_key;
    security::secure_remove(Path::new(&safety.path));
    tracing::info!(method = ?vault.method, "encryption changed");

    let recovery_key = match (&vault.key, in_keychain) {
        (Key::Raw(raw), true) => Some(Secret(raw.to_recovery_text().as_str().to_owned())),
        _ => None,
    };
    let status = status_impl(vault, keys, data_dir);
    Ok(EncryptionResult {
        status,
        recovery_key,
    })
}

/// Removes any key left in the keychain after leaving keychain mode.
fn clear_keychain(keys: &dyn KeyStore) {
    for slot in [Slot::Current, Slot::Pending] {
        let _ = keys.delete(slot);
    }
}

/// Deletes the backups that are not encrypted (made before encryption was turned on). They
/// would otherwise keep an unprotected copy of the data. Only for an encrypted database.
#[tauri::command]
pub async fn delete_unencrypted_backups(state: State<'_, AppState>) -> Result<u32, AppError> {
    let data_dir = state.data_dir.clone();
    state
        .run_vault(move |vault, _| delete_unencrypted_impl(vault, &data_dir))
        .await
}

pub(crate) fn delete_unencrypted_impl(vault: &mut Vault, data_dir: &Path) -> Result<u32, AppError> {
    let encrypted = !vault.key.is_none();
    let (conn, _) = vault.parts()?;
    if !encrypted {
        return Err(app_error(
            "invalid",
            "The database isn't encrypted, so plain backups match it",
        ));
    }
    let (folder, _) = backup_folder(conn, data_dir)?;
    let mut removed = 0;
    for entry in minimap_store::backup::list(&folder).map_err(store_error)? {
        if !entry.encrypted {
            security::secure_remove(Path::new(&entry.path));
            removed += 1;
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{commands::backup::backup_now_impl, keystore::memory::MemoryKeyStore};
    use minimap_types::{AssigneeChoice, CreateTask};

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "minimap-sec-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn task(vault: &mut Vault, title: &str) {
        minimap_store::tasks::create(
            vault.conn.as_mut().unwrap(),
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
        .unwrap();
    }

    fn task_count(vault: &Vault) -> usize {
        minimap_store::tasks::list(vault.conn.as_ref().unwrap(), false)
            .unwrap()
            .len()
    }

    fn request(choice: EncryptionChoice) -> SetEncryption {
        SetEncryption {
            choice,
            ..Default::default()
        }
    }

    fn passphrase(text: &str) -> SetEncryption {
        SetEncryption {
            choice: EncryptionChoice::Passphrase,
            passphrase: Some(Secret(text.into())),
            ..Default::default()
        }
    }

    fn file_state(dir: &Path) -> FileState {
        security::file_state(&dir.join("minimap.db")).unwrap()
    }

    fn pre_encryption_files(dir: &Path) -> usize {
        minimap_store::backup::list(&dir.join("backups"))
            .unwrap()
            .iter()
            .filter(|b| b.kind == BackupKind::PreEncryption)
            .count()
    }

    /// A vault on a fresh data folder with three tasks.
    fn fresh(name: &str) -> (std::path::PathBuf, Vault) {
        let dir = temp_dir(name);
        let keys = MemoryKeyStore::new();
        let mut vault = boot(&dir, &keys).unwrap();
        for t in ["Ship the confidential launch", "Write docs", "Plan Q3"] {
            task(&mut vault, t);
        }
        (dir, vault)
    }

    #[test]
    fn a_new_installation_starts_plain_and_unlocked() {
        let dir = temp_dir("boot");
        let keys = MemoryKeyStore::new();
        let vault = boot(&dir, &keys).unwrap();
        let status = status_impl(&vault, &keys, &dir);
        assert!(!status.locked && !status.encrypted && status.method.is_none());
        assert!(status.keychain_available);
        assert_eq!(status.min_passphrase_chars, 12);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn encrypting_with_the_keychain_keeps_the_data_hides_the_file_and_shows_the_key_once() {
        let (dir, mut vault) = fresh("keychain");
        let keys = MemoryKeyStore::new();
        // A plain backup made before encryption.
        backup_now_impl(vault.conn.as_mut().unwrap(), &dir, None, &Key::None).unwrap();

        let result =
            set_encryption_impl(&mut vault, &keys, &dir, request(EncryptionChoice::Keychain))
                .unwrap();
        assert!(result.status.encrypted && !result.status.locked);
        assert_eq!(result.status.method, Some(KeyMethod::Keychain));
        let shown = result.recovery_key.expect("the key is shown once").0;
        assert_eq!(shown.split('-').count(), 16);
        // The key shown is the one in the keychain.
        let stored = keys.get(Slot::Current).unwrap().unwrap();
        assert_eq!(stored.to_recovery_text().as_str(), shown);
        assert!(!keys.has(Slot::Pending), "the pending slot is cleared");
        assert_eq!(task_count(&vault), 3, "no data lost");
        assert_eq!(file_state(&dir), FileState::Encrypted);
        assert_eq!(
            pre_encryption_files(&dir),
            0,
            "the safety backup is removed once it succeeded"
        );
        // The plain backup from before is flagged, and can be deleted.
        assert_eq!(result.status.unencrypted_backups, 1);
        assert_eq!(delete_unencrypted_impl(&mut vault, &dir).unwrap(), 1);
        assert_eq!(status_impl(&vault, &keys, &dir).unencrypted_backups, 0);
        // New backups are encrypted.
        let made =
            backup_now_impl(vault.conn.as_mut().unwrap(), &dir, None, &vault.key.clone()).unwrap();
        assert!(made.encrypted);
        drop(vault);

        // Restart: the keychain opens it by itself.
        let again = boot(&dir, &keys).unwrap();
        assert_eq!(again.method, Some(KeyMethod::Keychain));
        assert_eq!(task_count(&again), 3);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn without_the_keychain_entry_the_app_starts_locked_and_a_recovery_key_opens_it() {
        let (dir, mut vault) = fresh("recovery");
        let keys = MemoryKeyStore::new();
        let shown =
            set_encryption_impl(&mut vault, &keys, &dir, request(EncryptionChoice::Keychain))
                .unwrap()
                .recovery_key
                .unwrap()
                .0;
        drop(vault);

        // The keychain was wiped (new computer, reinstalled OS...).
        let empty = MemoryKeyStore::new();
        let mut locked_vault = boot(&dir, &empty).unwrap();
        assert!(locked_vault.conn.is_none());
        let status = status_impl(&locked_vault, &empty, &dir);
        assert!(status.locked && status.encrypted && status.method.is_none());
        assert_eq!(locked_vault.parts().unwrap_err().code, "locked");

        // Wrong secrets are refused (slowly) and change nothing.
        for wrong in ["not the key at all", &"0".repeat(64), ""] {
            assert_eq!(
                unlock_impl(&mut locked_vault, &empty, &dir, wrong)
                    .unwrap_err()
                    .code,
                "wrong_key"
            );
            assert!(locked_vault.conn.is_none());
        }
        // The recovery key as it was shown, even sloppily typed, opens it and re-stores itself.
        let typed = format!("  {}  ", shown.to_uppercase());
        unlock_impl(&mut locked_vault, &empty, &dir, &typed).unwrap();
        assert_eq!(task_count(&locked_vault), 3);
        assert_eq!(locked_vault.method, Some(KeyMethod::Keychain));
        assert!(
            empty.has(Slot::Current),
            "the key went back into the keychain"
        );
        // Unlocking an open database does nothing.
        unlock_impl(&mut locked_vault, &empty, &dir, "anything").unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn with_no_keychain_the_best_option_left_is_a_passphrase_and_never_a_stored_key() {
        let (dir, mut vault) = fresh("nokeychain");
        let none = MemoryKeyStore::unavailable();
        let status = status_impl(&vault, &none, &dir);
        assert!(!status.keychain_available && status.keychain_problem.is_some());

        // The keychain choice is refused with the reason and nothing is touched.
        let e = set_encryption_impl(&mut vault, &none, &dir, request(EncryptionChoice::Keychain))
            .unwrap_err();
        assert_eq!(e.code, "keychain_unavailable");
        assert_eq!(file_state(&dir), FileState::Plain);
        assert_eq!(pre_encryption_files(&dir), 0);

        // Too short a passphrase is refused before anything happens.
        let e = set_encryption_impl(&mut vault, &none, &dir, passphrase("too short")).unwrap_err();
        assert_eq!(e.code, "invalid");
        assert_eq!(file_state(&dir), FileState::Plain);
        assert_eq!(pre_encryption_files(&dir), 0);

        let result = set_encryption_impl(
            &mut vault,
            &none,
            &dir,
            passphrase("a long enough passphrase"),
        )
        .unwrap();
        assert_eq!(result.status.method, Some(KeyMethod::Passphrase));
        assert!(
            result.recovery_key.is_none(),
            "a passphrase has no recovery key"
        );
        assert_eq!(file_state(&dir), FileState::Encrypted);
        drop(vault);

        // Every start asks; the right passphrase opens it.
        let mut again = boot(&dir, &none).unwrap();
        assert!(again.conn.is_none());
        assert_eq!(
            unlock_impl(&mut again, &none, &dir, "a long enough passphrasE")
                .unwrap_err()
                .code,
            "wrong_key"
        );
        unlock_impl(&mut again, &none, &dir, "a long enough passphrase").unwrap();
        assert_eq!(again.method, Some(KeyMethod::Passphrase));
        assert_eq!(task_count(&again), 3);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn moving_between_keychain_passphrase_and_off_never_loses_data_and_cleans_up() {
        let (dir, mut vault) = fresh("moves");
        let keys = MemoryKeyStore::new();
        let first =
            set_encryption_impl(&mut vault, &keys, &dir, request(EncryptionChoice::Keychain))
                .unwrap();
        // Rotating the keychain key gives a new recovery key and the old one stops working.
        let second =
            set_encryption_impl(&mut vault, &keys, &dir, request(EncryptionChoice::Keychain))
                .unwrap();
        assert_ne!(first.recovery_key, second.recovery_key);
        assert!(security::open_with_key_for_tests(&dir, &first.recovery_key.unwrap().0).is_err());

        // To a passphrase: the keychain is emptied.
        set_encryption_impl(
            &mut vault,
            &keys,
            &dir,
            passphrase("another passphrase 123"),
        )
        .unwrap();
        assert!(!keys.has(Slot::Current) && !keys.has(Slot::Pending));
        assert_eq!(vault.method, Some(KeyMethod::Passphrase));

        // Turning off needs confirmation and the current passphrase.
        let off = |confirmed: bool, current: Option<&str>| SetEncryption {
            choice: EncryptionChoice::Off,
            confirmed,
            current_passphrase: current.map(|c| Secret(c.into())),
            ..Default::default()
        };
        assert_eq!(
            set_encryption_impl(
                &mut vault,
                &keys,
                &dir,
                off(false, Some("another passphrase 123"))
            )
            .unwrap_err()
            .code,
            "invalid"
        );
        assert_eq!(
            set_encryption_impl(
                &mut vault,
                &keys,
                &dir,
                off(true, Some("wrong wrong wrong"))
            )
            .unwrap_err()
            .code,
            "wrong_key"
        );
        assert_eq!(
            set_encryption_impl(&mut vault, &keys, &dir, off(true, None))
                .unwrap_err()
                .code,
            "wrong_key"
        );
        assert_eq!(
            file_state(&dir),
            FileState::Encrypted,
            "refusals change nothing"
        );
        let result = set_encryption_impl(
            &mut vault,
            &keys,
            &dir,
            off(true, Some("another passphrase 123")),
        )
        .unwrap();
        assert!(!result.status.encrypted && result.status.method.is_none());
        assert_eq!(file_state(&dir), FileState::Plain);
        assert_eq!(task_count(&vault), 3);
        // Already off.
        assert_eq!(
            set_encryption_impl(&mut vault, &keys, &dir, off(true, None))
                .unwrap_err()
                .code,
            "invalid"
        );
        // Turning off from keychain mode needs only the confirmation.
        set_encryption_impl(&mut vault, &keys, &dir, request(EncryptionChoice::Keychain)).unwrap();
        set_encryption_impl(&mut vault, &keys, &dir, off(true, None)).unwrap();
        assert!(!keys.has(Slot::Current));
        assert_eq!(pre_encryption_files(&dir), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_crash_after_re_keying_but_before_the_keychain_was_updated_is_recovered_at_start() {
        let (dir, mut vault) = fresh("pending");
        let keys = MemoryKeyStore::new();
        set_encryption_impl(&mut vault, &keys, &dir, request(EncryptionChoice::Keychain)).unwrap();
        drop(vault);
        // The state a crash would leave: the working key only in the pending slot.
        let key = keys.get(Slot::Current).unwrap().unwrap();
        keys.delete(Slot::Current).unwrap();
        keys.set(Slot::Pending, &key).unwrap();
        let vault = boot(&dir, &keys).unwrap();
        assert_eq!(task_count(&vault), 3);
        assert!(
            keys.has(Slot::Current) && !keys.has(Slot::Pending),
            "promoted"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn if_the_keychain_refuses_the_final_write_the_pending_key_still_protects_the_data() {
        let (dir, mut vault) = fresh("refuses");
        let mut keys = MemoryKeyStore::new();
        keys.refuse_current_writes = true;
        let result =
            set_encryption_impl(&mut vault, &keys, &dir, request(EncryptionChoice::Keychain))
                .unwrap();
        assert!(
            result.recovery_key.is_some(),
            "the recovery key is still shown"
        );
        assert!(
            keys.has(Slot::Pending),
            "the key survives in the pending slot"
        );
        drop(vault);
        let again = boot(&dir, &keys).unwrap();
        assert_eq!(task_count(&again), 3, "start-up finds it there");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_failed_change_keeps_the_data_the_safety_backup_and_a_clean_keychain() {
        let (dir, mut vault) = fresh("failed");
        let keys = MemoryKeyStore::new();
        // Something blocks the new file, so the re-keying cannot start.
        std::fs::create_dir(dir.join("minimap.db.reencrypting")).unwrap();
        let e = set_encryption_impl(&mut vault, &keys, &dir, request(EncryptionChoice::Keychain))
            .unwrap_err();
        assert!(
            e.message.contains("not changed") && e.message.contains("pre-encryption"),
            "{}",
            e.message
        );
        assert_eq!(file_state(&dir), FileState::Plain);
        assert_eq!(task_count(&vault), 3);
        assert!(
            !keys.has(Slot::Pending) && !keys.has(Slot::Current),
            "no stray key"
        );
        assert_eq!(
            pre_encryption_files(&dir),
            1,
            "the backup is kept after a failure"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn secrets_never_print() {
        let req = passphrase("my very secret passphrase");
        let shown = format!("{req:?} {:?}", Secret("abcd-efgh".into()));
        assert!(
            !shown.contains("secret passphrase") && !shown.contains("abcd"),
            "{shown}"
        );
        let result = EncryptionResult {
            status: SecurityStatus {
                locked: false,
                encrypted: true,
                method: Some(KeyMethod::Keychain),
                keychain_available: true,
                keychain_problem: None,
                unencrypted_backups: 0,
                min_passphrase_chars: 12,
            },
            recovery_key: Some(Secret("1111-2222".into())),
        };
        assert!(!format!("{result:?}").contains("1111"));
    }
}
