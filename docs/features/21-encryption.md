# 21 — Encryption at rest

Status: Implemented — awaiting manual check · Milestone: M4 · Priority: Should
Depends on: 20

## Goal
Optional encryption of the database with SQLCipher, without the user managing keys unless they want to.

## Scope
**In**
- Off by default. Enabling: generate a random 256-bit key, store it in the OS keychain (`keyring`), `PRAGMA rekey`.
- Optional user-supplied passphrase instead of (or in addition to) the keychain key.
- On startup: read key from keychain → `PRAGMA key`; if missing or wrong, prompt for passphrase.
- Disable encryption (rekey to empty) with confirmation.
- Command: `set_db_passphrase`.
- Recovery: show/export the key once so the user can store it safely.

## Rules
- Take a backup before rekeying.
- Never log the key.

## Acceptance criteria
- [x] Encrypt an existing DB with demo data: no data loss, file unreadable without key. *(store `encrypting_an_existing_database_loses_nothing_and_hides_the_file`: every table (objectives … activity, settings) compared before and after, search and writes work afterwards, none of the plain text ("confidential", a person's name, the SQLite header) is in the file, it refuses no key / a wrong raw key / a wrong passphrase, and no plain or `.old` copy is left in the folder; demo data itself is spec 24, the fixture is hand-built)*
- [x] App opens normally on restart with the keychain key. *(command test `encrypting_with_the_keychain_keeps_the_data_…` boots again with the same keychain and finds the data; start-up also recovers an interrupted key change; the real OS keychain could not be exercised in the development environment, which has no Secret Service, so it needs a manual check)*
- [x] Encrypted backups restore with the same key. *(store `backups_of_an_encrypted_database_are_encrypted_and_never_plain`, `a_backup_of_another_key_needs_that_key_…`, `plain_and_encrypted_backups_restore_into_the_other_kind_of_database`)*

## Decisions
- **No keychain? Passphrase only, never a stored key** (your answer: "utilise the best option available, security is uncompromisable"). With a usable system keychain (macOS Keychain, Windows Credential Manager, Linux Secret Service such as GNOME Keyring or KWallet) the recommended option is a random key kept there. Where none is available Settings says why and offers the passphrase; there is **no fallback that writes a key to a file or the database**. The passphrase is stored nowhere and cannot be recovered; the screen says so before it is set. A passphrase needs at least 12 characters (SQLCipher then derives the key with PBKDF2).
- **"Instead of", not "in addition to"**: one mode at a time, keychain *or* passphrase (SQLCipher has one key; combining would need a key-wrapping scheme of our own, which is not worth inventing for a security feature). The keychain mode also gives a **recovery key**, so the keychain is not a single point of failure; passphrase mode has no recovery by design.
- **Re-keying is export-and-swap, not `PRAGMA rekey`** (ADR-0010). SQLCipher cannot encrypt or decrypt a plain database in place and its backup API refuses plain↔encrypted copies, so every change (on, off, keychain↔passphrase, new key) writes the data into a new file with the new key (`sqlcipher_export`), **checks it against the original** (integrity check, schema version, every table's row count, schema objects), swaps it in and reopens, keeping the original as `minimap.db.old` until the new file has opened and putting it back if anything fails. The old file is then overwritten with zeros and deleted (best effort; a journaling or flash file system may keep traces, which is one more reason to encrypt early). An interrupted swap is undone at the next start.
- **Backup before changing the key** (the rule): a `pre-encryption` backup is taken with the current key before anything changes. If the change succeeds it is removed (it is a copy under the old protection); if it fails it is kept and named in the error.
- **Crash-safe keychain key**: a new key is written to a *pending* keychain slot and read back before the database is touched, and becomes the *current* one only afterwards. Start-up tries both slots, so a crash at any point leaves a key that opens the database.
- **Recovery key**: shown once, in groups (`1a2b-3c4d-…`), after turning on keychain mode or rotating ("New recovery key"); it stays on screen until you tick that you saved it, with a Copy button. It is not shown again. The unlock screen accepts a passphrase or a recovery key (dashes, spaces and capitals are ignored); a recovery key that works is put back into the keychain if one is usable. (Export to a file was not added: the key is displayed and copyable, and writing a key to a file is the thing to avoid.)
- **Locked start**: with an encrypted database and no usable key the app starts *locked*: an unlock screen replaces the app and every command answers `locked`. A wrong secret waits about 0.75 s before answering. The automatic daily backup does nothing while locked.
- **Turning off** needs a confirmation, and the current passphrase in passphrase mode.
- **Backups follow the database**: a backup of an encrypted database is encrypted with the same key (SQLCipher refuses to write it into a plain file, so a plaintext copy can't happen by accident); a backup is restorable into either kind of database (plain into encrypted and the reverse go through a verified re-keyed copy); a backup encrypted with a *different* key asks for that key. **Backups made before encryption stay plain**: Settings lists how many and offers "Delete them…" (overwrites and removes them; encrypted ones are untouched). They are not deleted automatically because they are your history.
- **Never log the key**: key types print as `<redacted>`, are wiped from memory on drop, and secrets sent from the screen (`Secret`) print the same way; nothing logs them. SQLCipher's own "error decrypting page" stderr line (printed whenever a key is tried and fails) is switched off. Files holding private data (the encrypted database, backups) are created owner-only on Unix.
- **`set_db_passphrase`** from the spec is `set_encryption` with the `passphrase` choice (and `keychain` / `off`); there are also `get_security_status`, `unlock_database` and `delete_unencrypted_backups`.

## Implementation notes
- Types (`minimap-types::security`): `Secret`, `KeyMethod`, `SecurityStatus`, `EncryptionChoice`, `SetEncryption`, `EncryptionResult`, `MIN_PASSPHRASE_CHARS`; `BackupKind::PreEncryption`, `BackupEntry.encrypted`, `BackupStatus.database_encrypted`, `RestorePreview.encrypted`.
- Store `security`: `RawKey`, `Passphrase`, `Key{None,Raw,Passphrase}`, `file_state`, `rekey`/`replace_live`, `recover_interrupted`, `remove_leftovers`, `secure_remove`; `open_with_key`; `StoreError::{WrongKey, BackupKeyNeeded}`; `backup::{create, inspect, restore}` take the key(s).
- Commands (`commands/security.rs`): `boot` (start-up, keychain, locked state), `get_security_status`, `unlock_database`, `set_encryption`, `delete_unencrypted_backups`; `keystore.rs` (`KeyStore` trait with the `keyring` crate behind it, a memory fake for tests); `AppState` now holds a `Vault{conn: Option<Connection>, key, method}`.
- UI: `components/unlock.rs` (the locked screen, shown by `LockGate` in `app.rs`), `components/security_settings.rs` (Settings → Encryption), the Backup card flags unencrypted backups and asks for a key when a backup needs one; permissions `allow-get-security-status`, `allow-unlock-database`, `allow-set-encryption`, `allow-delete-unencrypted-backups`.

## Not yet verified by hand
- the real OS keychain: Encrypt with the system keychain → the recovery key appears once; restart → opens without asking; (Linux) with no Secret Service the keychain button is disabled with the reason and the passphrase option works
- passphrase mode: restart shows the unlock screen; a wrong passphrase is refused; the right one opens it; the recovery key opens a keychain-mode database after clearing the keychain entry
- turn encryption off (with the passphrase in passphrase mode) and confirm the data is intact
- Settings → Backup marks plain backups "unencrypted" after encrypting; "Delete them…" removes them; restoring an old plain backup into the encrypted database works and it stays encrypted
- `~/.local/share/app.minimap.desktop/minimap.db` is not readable with `strings` once encrypted
