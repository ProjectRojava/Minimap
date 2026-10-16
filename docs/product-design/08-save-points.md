# 08 — Save points: every restorable copy on one timeline

Status: Implemented as spec 51 — awaiting manual check (open questions answered as recommended) · Milestone: — · Priority: Could · Size: M
Depends on: specs 20, 21, 22, 23 (implemented) · Read first: [README](README.md)
Expected ADR: none (extends the backup file-name scheme; note it in spec 20's text and `core::backup` docs)

## Goal
Games make "is my progress safe?" a non-question: there are save slots with times on them, a quicksave key, and you can load any slot. Minimap already has the machinery (manual, daily and safety backups, spec 20; Drive checkpoints, spec 22; undo, spec 25), but it's spread over two Settings cards with different words, and nothing outside Settings says when the last restorable copy was made.

Show **every restorable copy as one timeline of save points**, let the user **make a named save point with `Ctrl/Cmd+S`** ("before the Q2 replan"), and show **"Last save point: 2 h ago"** in the status bar.

Undo stays separate: it's per session and per change, not a save point. The Help page explains the difference.

## User stories
- Before reorganising my projects I press `Ctrl/Cmd+S`, type "before Q2 replan", Enter. Toast: "Save point made: before Q2 replan." It's a manual backup, never deleted automatically.
- Settings → Data & backup → **Save points**: a list grouped by day: "Today · 14:02 *before Q2 replan* (you) · 09:00 daily · Drive: 13:00 hourly checkpoint (this laptop)". Each row: *Restore…*.
- The status bar shows "Last save point 2 h ago". If there hasn't been one for over 24 hours, it turns amber and says why ("Automatic backups are off"). Clicking it opens the timeline.

## Rules (`minimap-core`, pure)
Extend `crates/minimap-core/src/backup.rs` and add `crates/minimap-core/src/save_points.rs`.

- **Named manual backups**: file name `minimap-YYYYMMDD-HHMMSS--<label-slug>.db` (two hyphens, then the label slug). `label_slug(text) -> Result<String, String>`: trim, 1..=40 visible chars after slugify (`slug::slugify`, which lower-cases and joins ASCII words with `-`); non-ASCII-only labels fall back to the slug `save` and the original label is **not** recoverable from the name (see Open questions). `file_name(kind, at, schema)` gets an optional label for `Manual` only. `parse_name` accepts both forms and returns `label: Option<String>` (the slug, with `-` shown as spaces by the UI). Unnamed manual backups keep today's name; **all existing names keep parsing** (test with every kind).
- **Timeline**: `save_points::merge(local: &[BackupEntry], drive: &[CheckpointInfo], this_device: &str, now) -> SaveTimeline { days: Vec<SaveDay { date, points: Vec<SavePoint> }>, last: Option<SavePoint>, stale: Option<StaleReason> }`.
  - `SavePoint { source: Local{path, kind, label, encrypted} | Drive{name, device_name, this_device: bool, cadence: Hourly|Daily}, at: OffsetDateTime, bytes, keeps_until: Option<Date> }`.
  - Order: newest first; group by the **local** calendar day (take the user's UTC offset as a parameter, as meetings do).
  - `keeps_until`: for `Auto` the date it will be pruned given `keep_auto` and the current list (the 14th newest auto backup's successor date estimate: simply "kept while among the newest 14": compute which auto entries are beyond index 13 → `keeps_until = None` and flag `will_be_pruned`); for Drive checkpoints use `sync::checkpoint_prune_plan` to mark those about to go. Manual/named and safety backups never expire (`None`).
  - `last` = newest point of any source that can restore the whole database (all of them). `stale` when `now - last.at > 24 h`: reason `AutoOff` (auto backup off and Drive not connected), `AutoFailing` (auto on but none in 24 h), `DriveOffline` (Drive connected but no checkpoint in 24 h and no local one either).
- **Tests**: parse/format round trip with and without labels; old names parse unchanged (every `BackupKind`); slug rules; merge ordering and day grouping across an offset; prune flags agree with `prune_plan` and `checkpoint_prune_plan`; stale reasons. **proptest**: `parse_name(file_name(kind, at, label))` returns the same kind, time (to the second) and slug.

## Data
No migration. No new setting.
Types: `SaveTimeline`, `SaveDay`, `SavePoint`, `SaveSource`, `StaleReason` in `crates/minimap-types/src/backup.rs`; `BackupEntry.label: Option<String>`.

## Commands
- `backup_now(path?, label?)`: optional label; invalid label → `invalid` with the message. (Argument struct camelCase.)
- `list_save_points(utc_offset_minutes)` in `commands/backup.rs` (`save_points_at(.., now)` for tests): local list (`store::backup::list`) + Drive checkpoints when connected (`Engine::list_checkpoints`, with a **short timeout (5 s)**; on `offline`/`drive` errors return the local points plus `drive_note: Some("Couldn't reach Google Drive; showing this computer's save points")`, never fail the whole call). Register in the 4 places.
- `get_last_save_point()` cheap variant for the status bar: local list + the engine's **cached** newest checkpoint time if it has one (no network). Add a cached field to the engine status if it doesn't exist (`SyncStatus.last_checkpoint_at: Option<String>`), set when a checkpoint is written or listed.
- Restoring uses the **existing** flows: local → `preview_restore` / `restore_backup`; Drive → `recover_checkpoint` (with its confirm dialog). No new restore code.

## UI
- **`Ctrl/Cmd+S`** (`keyboard.rs`: handle before the "leave Ctrl/Cmd combos alone" early return, and also while typing in a field, since browsers' save-page default must be prevented anyway): opens the **SaveBox**, a small centred dialog: "Save point name (optional)", Enter = make it (`backup_now(None, label)`), Esc = cancel. While it runs: "Saving…"; then toast "Save point made: <label or time>". An encrypted DB makes an encrypted backup as today.
- **Settings → Data & backup → Save points** (`components/backup_settings.rs`, renamed card *Backups and save points* or a new `components/save_points.rs` card placed above Google Drive): replaces the backup **list** of the Backup card (folder, auto-backup and *Back up now* controls stay where they are; *Back up now* opens the SaveBox). Day headings (`GroupLabel`), rows: time, a kind chip (*Named* accent, *Manual*, *Daily*, *Before restore/upgrade/sync/recover/encryption* neutral, *Drive hourly/daily* with the device name), the label in `fg` when named, size, "will be removed soon" in `muted` for prunable ones, *Restore…*. The Drive card's checkpoint list stays (it's where recovering is explained) but gets "See all save points" linking here.
- **Status bar** (`components/sync_status.rs::StatusBar`): a second item right of the Drive state: a small save glyph + "Last save point 2 h ago" (pure `last_text(age)`: "just now", "12 min ago", "2 h ago", "yesterday", "3 days ago"); `warning` text with the reason (pure `stale_text`) when stale; click → `/settings?tab=data#save-points`. Refreshes on the same 4 s poll as sync status? **No**: refresh on `DataVersion` bumps and every 5 minutes; it's cheap.
- **Palette**: "Make a save point…" (opens the SaveBox).

## Help wiki
- `backup-restore.md`: rename the user-facing idea to *save points* where it reads better; a *Save points* section: what counts (named, manual, daily, safety copies, Drive checkpoints), `Ctrl/Cmd+S`, which ones are kept forever and which are pruned, the status-bar line and when it turns amber, how this differs from undo.
- `keyboard.md`: `Ctrl/Cmd+S`.
- `google-drive.md`: checkpoints appear in the save points list.
- `undo.md`: one sentence pointing to save points for "go back to how it was this morning".

## Demo data
No schema change; the demo seeds no backups. Nothing to do; say so in the PR.

## Export, undo, sync
Backups are files, not rows: no export/undo/merge change. Label text is in the file name only; it never syncs (local backups never do).

## Out of scope
Labels on Drive checkpoints; renaming or deleting save points from the timeline (deleting plain backups of an encrypted DB already exists in Security); diffing two save points.

## Acceptance criteria
- [x] Core naming, parsing (old names unchanged), merge, prune flags, stale reasons with tests and proptest.
- [x] `backup_now` with a label; `list_save_points` with and without Drive (offline returns local points and a note; command test with the mock remote).
- [x] `Ctrl/Cmd+S` SaveBox works from any screen and inside text boxes; the browser default is prevented.
- [x] Save points card, status-bar line, palette action.
- [x] Help pages; CHANGELOG; knowledge graph (§5 `mod:backup`, `mod:save_points`, §7, §8); `docs/progress.md`; gate passes.

## Implementation notes (spec 51)
- **Names**: `core::backup` gained `label_slug`, `label_text`, `file_name_with_label` and `parse_name_full` (returning a `ParsedName {kind, at, label}`); `file_name` and `parse_name` are unchanged wrappers, so no caller moved. A name is at most 80 characters as typed (the slug is cut at a word to 40) and a blank one is refused by `label_slug` (the command treats a blank label as none). The label is only a slug in the file name, never recoverable as typed (open question 1).
- **Timeline**: `core::save_points::merge(&TimelineInput{...})` takes the listings, this computer's Drive device id (to mark its own checkpoints), the user's UTC offset in minutes, `keep_auto`, whether automatic backups are on and Drive connected. `SavePoint.will_be_pruned` replaces the spec's `keeps_until` (the spec itself ends with `None` for everything). Drive cadence is *hourly* for a device's newest 24 checkpoints and *daily* beyond. Stale reasons: `AutoFailing` when automatic backups are on, else `DriveOffline` when Drive is connected, else `AutoOff`.
- **Commands**: `list_save_points(utc_offset_minutes)` asks Drive on a worker thread with a 5 s wait; on timeout or error it answers this computer's save points and `drive_note`. `get_last_save_point` uses the sync engine's cached checkpoints (`SyncStatus.checkpoints`, filled whenever one is written or listed) and the backup folder, with no network, so it can say a Drive-only user is stale until the first checkpoint of a session; no new `SyncStatus` field was needed. `timeline_at` / `summary_at` are pure and tested with an offline and an online `DriveListing`; the engine itself is not started in these tests.
- **UI**: `SaveBox` (state `SaveBoxState` from `App`, host in `Shell`), `Ctrl/Cmd+S` handled in `keyboard.rs` before everything else (so also inside text boxes), the *Save points* card above Google Drive (`#save-points`), `SaveStatus` in the status bar (outside the Drive status closure, which redraws every poll). *Back up now* now opens the same box. The backup card's list was removed (the timeline replaces it); local rows restore through the existing `check` / `ConfirmRestore` flow and Drive rows through `ConfirmRecover`.
- **Demo data**: none (the demo seeds no backups).

## Not yet verified by hand
- `Ctrl/Cmd+S`, name it, Enter: it appears at the top of the timeline with its name; the file in the backup folder has the slug in its name.
- Restore it: the existing restore confirmation shows its counts; afterwards a *Before restore* point is in the list.
- With Drive connected: hourly checkpoints are interleaved by time; turn the network off: the list still shows local points with the note.
- Turn automatic backups off and wait (or set the clock) past 24 h: the status-bar line turns amber with the reason.

## Open questions
1. Keep the label's exact text (e.g. non-ASCII) in a sidecar file `<name>.label`? **No**; slug only, keeps backups one file each.
2. Should `Ctrl/Cmd+S` skip the dialog and save at once (true quicksave)? **No**: the name is the point; Enter on the empty box is one extra key.
