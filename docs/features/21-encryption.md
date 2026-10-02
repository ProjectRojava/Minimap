# 21 — Encryption at rest

Status: Draft · Milestone: M4 · Priority: Should
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
- [ ] Encrypt an existing DB with demo data: no data loss, file unreadable without key.
- [ ] App opens normally on restart with the keychain key.
- [ ] Encrypted backups restore with the same key.

## Open questions
- If the keychain is unavailable (some Linux setups), fall back to passphrase only?
