# 20 — Local backup and restore

Status: Draft · Milestone: M4 · Priority: Must
Depends on: 01

## Goal
Users never lose data, and can prove it to themselves.

## Scope
**In**
- `backup_now(path)`: SQLite online backup API → `minimap-YYYYMMDD-HHMMSS.db` in the chosen folder.
- Automatic daily backup (on app start if the last one is > 24 h old, and on a timer); keep the last 14.
- Pre-migration backup before any schema upgrade.
- Restore: pick a backup file → validate (opens, schema version ≤ current, integrity_check) → confirm → replace live DB (current DB saved as a backup first) → reload app.
- Settings: backup folder, auto-backup on/off, last backup time, "Back up now", "Restore…".
- Tip in Settings: a folder synced by Google Drive/Dropbox/OneDrive desktop apps works as off-site backup.

## Rules
- The live DB must never sit in a cloud-synced folder; warn if the user tries to move it there.
- Backups are encrypted if the DB is encrypted (21).

## Acceptance criteria
- [ ] Backup → modify data → restore → data matches the backup exactly.
- [ ] Only 14 auto-backups kept.
- [ ] A corrupt or newer-schema file is refused with a clear message.

## Open questions
- Default backup folder: inside app-data, or force the user to choose one?
