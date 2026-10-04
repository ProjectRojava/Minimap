# 20 — Local backup and restore

Status: Implemented — awaiting manual check · Milestone: M4 · Priority: Must
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
- [x] Backup → modify data → restore → data matches the backup exactly. *(store test `backup_modify_restore_gives_back_exactly_the_backed_up_data` compares every table (objectives … activity, settings) as text before the backup and after the restore, and checks search and new writes still work on the restored data; command test `restoring_through_the_commands_round_trips_and_refuses_bad_files`)*
- [x] Only 14 auto-backups kept. *(core `prune_plan` + proptest; store `only_the_newest_fourteen_automatic_backups_survive` (20 made, 14 stay, manual / pre-migration / pre-restore / foreign files untouched); command `the_daily_backup_runs_when_due_and_keeps_fourteen`)*
- [x] A corrupt or newer-schema file is refused with a clear message. *(store `files_that_cannot_be_restored_are_refused_with_a_reason_and_change_nothing`: not a database, a backup with its middle overwritten, a truncated one, a newer schema, a non-Minimap SQLite file, a folder, a missing path; each says why, and nothing is changed or saved)*

## Decisions
- **Default backup folder** (your answer): there is a default, `backups` inside the app's data folder, so backups work from the first launch with no setup; **Settings → Backup → Change…** (OS folder picker) moves them anywhere, **Use default** goes back. The default is shown with a "default" chip. Backups already in an old folder stay there; the list shows the current folder only.
- **File names say what a backup is**, so Minimap can tell its own files from anything else in the folder and never deletes a file it did not name: `minimap-YYYYMMDD-HHMMSS.db` (manual, as the spec says), `minimap-auto-…` (daily), `minimap-pre-migration-vN-…` (N = the version upgraded from), `minimap-pre-restore-…` (the live data just before a restore). Times are UTC. Retention (14) applies **only to the daily ones**; manual, pre-migration and pre-restore copies are never deleted automatically.
- **Daily backup**: on start, and then hourly checks, a backup is made when there is none or the newest manual-or-daily one is over 24 hours old; a manual backup therefore postpones it. Turn it off with the checkbox. It runs on a background thread and takes the database lock only while copying.
- **`backup_now(path)`**: `path` is optional and is a *folder*; omitted = the configured folder. The UI uses the configured one.
- **Pre-migration backup** happens in `store::open`: when an existing database is below the latest schema it is copied to `<app data>/backups/` first; if the copy cannot be made, nothing is migrated. (It is always the default folder because the chosen folder is stored inside the very database being upgraded.)
- **Restore** (Settings → Backup → a row's **Restore…**, or **Restore from file…**): the file is checked (it opens, SQLite `integrity_check` passes, it is a Minimap database, its schema is not newer than this version), then the dialog shows what it holds (counts of items) and asks. On confirm: the live data is saved as a `pre-restore` backup in the backup folder, the backup is copied **into the live connection with SQLite's backup API** (one transaction: all or nothing; no file swapping, the open connection and WAL stay valid), an older backup is upgraded to the current schema, and the page reloads. **This installation's backup folder and auto-backup choice are kept** (otherwise a backup would silently repoint where new backups go).
- **The live database never moves.** It stays in the app data folder (not synced); the cloud-synced-folder tip is about the *backup* folder. The "warn when moving the live DB into a synced folder" rule belongs to spec 23 (changing the DB location), which is not built.
- **Backups and encryption**: the online backup copies pages as they are, so once the database is encrypted (spec 21) the copies are encrypted with the same key. Restoring then needs that key; spec 21 must carry it through `inspect`.
- **Folder access**: `dialog:allow-open` was added for the folder and file pickers (ADR-0009 addendum); the paths still go through backend checks (a backup folder must be an absolute path; a restore file must pass the checks above).

## Implementation notes
- Types (`minimap-types::backup`): `BackupKind`, `BackupEntry`, `BackupStatus`, `BackupCount`, `RestorePreview`, `RestoreResult`, `AUTO_BACKUPS_KEPT`; `Settings.backup_folder` (`None` = default) and `Settings.auto_backup` (default on), with the matching `UpdateSettings` fields (empty folder = default).
- Core `backup`: `file_name`, `parse_name`, `is_due`, `prune_plan`, `display_time` (pure).
- Store `backup`: `create` (online backup to a `.part` file, `journal_mode = DELETE`, `quick_check`, rename), `list`, `prune_auto`, `inspect`, `restore`, `before_migration`; `lib.rs`: `LATEST_SCHEMA`, `upgrade`, `now`.
- Commands (`commands/backup.rs`): `get_backup_status`, `backup_now`, `preview_restore`, `restore_backup`; `AppState` now carries `data_dir`; the timer thread is `spawn_auto_backup` in `main.rs`.
- UI: `components/backup_settings.rs` (Settings → Backup), `api::pick_folder`, `api::pick_backup_file`.

## Not yet verified by hand
- Settings → Backup shows the default folder with a "default" chip; Back up now adds a "manual" row and a toast; the file exists in that folder
- the daily backup: with the app closed for over a day (or no backup yet) a "daily" row appears after launch
- Change… picks a folder and later backups land there; Use default returns
- Restore…: pick a backup, the dialog lists what it holds; Cancel changes nothing; Restore reloads the app with the old data and a "before restore" backup appears; restoring that one brings the later work back
- Restore from file… with a text file renamed to `.db`, or a truncated backup, shows a clear refusal
- the upgrade path: an old database file in app data is copied to `backups/` ("before upgrade") before the app starts using it
