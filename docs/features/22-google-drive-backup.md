# 22 — Google Drive storage, autosave and multi-device sync

Status: Implemented — awaiting manual check (needs your Google OAuth client) · Milestone: M4 · Priority: Must
Depends on: 20, 21

## Goal
Google Drive is where Minimap's data and media live. Once connected, every change is saved to Drive automatically, and every device the user connects stays in step with it: edit on the laptop, see it on the desktop seconds later, with no "take over", no manual merge, no conflict dialogs. Files attached to items (images, SVG, Markdown, Excel, PowerPoint, Word, PDF …) are stored there too. Drive stays **optional**: without it the app is fully usable, but the user is told clearly, and repeatedly, that their data lives on this device only. Design recorded in ADR-0011.

## Model
- **Every device keeps a full local working copy** (`<app_data_dir>/minimap.db`, fast, offline, transactional; SQLite cannot run on a network drive, and the live DB never sits in a synced folder). **Drive holds each device's latest snapshot**, and devices merge each other's snapshots into their own copy automatically.
- **One snapshot file per device** (`devices/<device_id>.db.enc`). A device only ever writes its own file, so two devices can never overwrite each other and no lease or lock is needed. Each device's snapshot already contains everything it has merged, so any one device's file is a complete picture as of its last save.
- **Merging is automatic, row by row, newest write wins** (see *Merge rules*). It is deterministic and symmetric: merging A into B and B into A gives the same result, so devices converge no matter who syncs first, and merging the same snapshot twice changes nothing.
- **Media** is stored on Drive as encrypted, content-addressed blobs (shared by all devices); each device keeps a local cache.
- **Without Drive connected**: data in the local DB, media in the local cache, local backups (20). Connecting uploads everything; disconnecting leaves the local copy complete.

### Drive layout (all inside one app folder, `drive.file` scope)
```
Minimap/
  vault.json                          key check + format version (no secrets)
  devices/<device_id>.db.enc          each device's latest snapshot (that device writes only its own)
  checkpoints/<device_id>-YYYYMMDD-HHMMSS.db.enc   hourly/daily history
  media/<sha256>.bin                  one encrypted blob per distinct file
```
Each device file carries `appProperties`: `device_id`, `device_name`, `saved_at`, `seq` (counts that device's saves), `format`.

## Merge rules
Merge runs on the live connection in one transaction (all or nothing) against a decrypted-in-memory/keyed temp copy of the other device's snapshot.
- **Rows** (objectives, projects, tasks, people, teams, notes, decisions, waiting-on, attachments, edges): matched by `id` (edges by `(edge_type, from_id, to_id)`). Missing here → added. Present on both → the row with the later `updated_at` wins whole; equal timestamps break the tie on a stable content hash so both devices pick the same row. Archiving is just a write, so a later unarchive beats an earlier archive and the reverse.
- **Hard deletes** leave a tombstone (`tombstones(kind, id, deleted_at)`); a tombstone beats any edit, so a deleted item never comes back.
- **Activity** is unioned by `id` (the later `at` wins for a folded row).
- **Settings** that describe the work (hours per day, stale-waiting days, health thresholds, capacity limit, report template) merge by key, newest write wins. Per-device settings (theme, backup folder, auto-backup, Drive options) are never synced.
- **After merging, the rules the database cannot express are re-checked** and repaired, each repair listed in the sync report so nothing changes silently:
  - Two active projects with the same handle: the later-created one gets `<handle>-<last six hex digits of its id>` (the same on every device whatever the merge order; counting -2, -3 would not be).
  - Two self people: the incoming one stays but is no longer "me".
  - Loops in `blocks`, `depends_on`, `supersedes`, `reports_to`, or team nesting created by two devices together: the most recently written link in the loop is archived (a team's parent is cleared).
  - Links whose item no longer exists are removed; tasks, projects, teams or waiting-on rows pointing at an item that no longer exists lose that pointer (waiting-on falls back to me).
- **Known limit**: a row is replaced as a whole, so if two devices edit *different fields of the same item* within the same sync interval, the later save wins for all its fields. Both versions stay in the activity history.

## Scope
**In**
- **Connect** Google account from Settings: desktop OAuth with loopback redirect + PKCE, scope `drive.file` only; the system browser does the sign-in. Refresh token in the OS keychain (or, where there is none, in the local database, which is encrypted when encryption (21) is on). Disconnect revokes it and deletes it locally.
- **Local-only warning**: while Drive is not connected, the status bar always shows "Local only" (click → Settings), a banner on This week explains that data exists on this device alone and offers **Connect Google Drive** (dismissed for 7 days, never permanently), and Settings says the same. A first-run notice mentions it too.
- **First connect, Drive empty**: create the folder layout, create the vault key (if not made already for local attachments), show the **recovery key** once, upload the snapshot and all media.
- **First connect, Drive already has Minimap data** (second device, reinstall): ask for the recovery key (checked against `vault.json` before anything changes). If this device has no data of its own it **adopts** the newest snapshot; otherwise it **merges** with Drive (both sets of data are kept). Either way a `pre-sync` backup is taken first.
- **Autosave**: after any committed write, upload a fresh snapshot once the app has been idle for 5 s, and at most 30 s after the first unsaved change. Also on app close (flush, wait up to 10 s, else queue for next start), on reconnect, and on wake.
- **Pull**: every 15 s while online and at start, list `devices/`; any other device whose `seq` is newer than the one last merged is downloaded and merged; the UI refreshes (event `data-changed`). A merge that changes this device's data triggers a save so the others see the merged result. Pulling never blocks the UI; at start the app opens immediately on its local copy.
- **Snapshot**: SQLite online backup into a temp file keyed with the vault key before any page is written (no plaintext copy touches disk), device-local rows removed (settings marked local, the vault key, tokens, sync bookkeeping), then uploaded (resumable above 5 MB) over this device's file.
- **Checkpoints**: the first snapshot of each hour is kept as a checkpoint (this device's). Retention: last 24 hourly + last 30 daily. Only app-named files are ever deleted.
- **Attachments** on any item (objective, project, task, person, team, note, decision, waiting-on): allowed kinds are images (png, jpg, gif, webp, bmp, svg, heic, avif, tiff), text and Markdown (md, markdown, txt, csv, tsv, json), PDF, Word (doc, docx, dot, dotx, odt, rtf), Excel (xls, xlsx, xlsm, csv, ods), and PowerPoint (ppt, pptx, odp, key). Executables and scripts are refused.
  - Add from the detail pane (file picker or drag and drop); in the note editor, paste or drop an image → inserted as `![name](attachment:<id>)`.
  - Stored content-addressed by SHA-256 (the same file attached twice is stored once), encrypted, uploaded in the background ("Waiting to upload…" until done).
  - Listed in the detail pane (name, kind, size, date); images and SVGs show a thumbnail; open with the OS default app; remove.
  - Images referenced from notes render in the preview, served by the backend through a custom Tauri URI scheme (`minimap-media`), never a network URL. SVG is only ever shown as an image (scripts don't run). External images are still not fetched (spec 09 rule).
  - Local cache in `<app_data_dir>/media/` (encrypted blobs), default limit 2 GB, least-recently-used first; a blob is never evicted until it is on Drive. Missing blobs download on demand.
  - Per-file limit 250 MB.
- **Status bar**, always visible: "Saved to Drive · 12 s ago" / "Saving…" / "Syncing…" / "Offline · changes waiting" / "Drive error" / "Local only". Settings shows the account, last save, last merge (who from, what changed, repairs made), errors, Drive usage by Minimap, other devices (name, last seen), checkpoints, and **Sync now**.
- **Recover from a checkpoint** (Settings): download → validate → a `pre-recover` backup is taken → every item that differs from the checkpoint comes back as it was then (changed ones are put back, deleted ones return, tombstones for them are lifted), stamped as new edits so they also win on the other devices; items created after the checkpoint are kept. It is recovery of old data, not a rollback of the whole database (a rollback would be undone by the next sync from the other devices).
- **Local backups (20) stay**: the daily local backup keeps running.
- All HTTP in Rust (`reqwest`, rustls) on background threads; the webview CSP stays closed to the network.

**Out**
- Merging at field level (see *Known limit*), or three-way text merge of a note edited on two devices at once.
- Sharing data or attachments with other Google users.
- Storing media inside the SQLite file.
- Other cloud providers (the remote is behind a trait so they can be added).

## Data
- Migration 0008: `edges.updated_at` (kept current by triggers); `settings.updated_at` (kept current by triggers); `tombstones(kind, id, deleted_at, PRIMARY KEY(kind, id))`, written by every hard delete; `attachments(id, node_type, node_id, sha256, file_name, mime_type, size_bytes, created_at, updated_at, archived_at)` with indexes on `node_id` and `sha256`.
- Device-local data lives in `app_meta` (never merged, never uploaded, kept across a restore): `device_id`, `device_name`, `vault_key`, `drive_refresh_token` (only when there is no keychain), `drive_client`, `sync_state` (what was last merged per device, last save, last error).
- Writes to attachments go through the store with an activity row on the owning item.
- An archived item keeps its attachments (they are hidden with it and come back with it); hard delete removes the attachment rows and writes tombstones, and the cached file and Drive blob go once no attachment uses that `sha256` (blobs nothing references are deleted from Drive after 7 days).

## Encryption
- Everything uploaded is encrypted, whether or not the local DB is encrypted (21).
- **Vault key**: a random 256-bit key created the first time it is needed (first attachment or first connect), kept in the local database (`app_meta`), shown once as a **recovery key** when Drive is connected (same display and "I saved it" confirmation as 21). A second device needs the recovery key to read Drive; without it Drive data cannot be recovered, and the screen says so.
- Snapshots and checkpoints are SQLCipher databases keyed with the vault key. Media blobs (Drive and local cache) use XChaCha20-Poly1305 in a chunked streaming construction, keyed from the vault key.
- `vault.json` holds the format version and a key check (an encrypted known value) so a wrong recovery key is refused before anything is downloaded or merged. No key material.
- Never log keys, tokens, file names or note content.

## Rules
- **Never silently lose a write.** An upload replaces this device's snapshot only after it completes; a failed upload leaves the previous one in place and retries with backoff (5 s, 30 s, 2 min, then every 10 min). A snapshot is made only after local changes are committed; changes made while it uploads set the dirty flag again.
- **Merging never discards the local copy's newer rows**, and a failed merge changes nothing (one transaction). A `pre-sync` backup is taken before the first merge on a device and before any merge that would remove more than 20 rows.
- **A snapshot from a newer schema** is skipped with "Update Minimap on this device to sync with <device>"; an older one is upgraded in a temp copy first.
- **Offline**: changes queue silently; after 3 days without a successful save, show a persistent warning.
- Retention deletes only app-created checkpoints beyond the limits, and media no attachment references (after 7 days, so a slow device can still catch up).
- Autosave and merge never block the UI: the DB lock is held only for the snapshot copy and for the merge transaction.
- Clock differences between devices decide "newest" (`updated_at` is the writing device's clock); Settings warns when a device clock is more than 2 minutes off Google's.
- Activity log and undo (25) are unaffected: syncing copies activity, it does not create rows of its own.

## Deviations (resolved in ADR-0011)
- CLAUDE.md §1 "All data in one SQLite file": structured data stays in one SQLite file; attachments are blobs stored alongside (Drive plus local cache).
- CLAUDE.md §8 / ADR-0002 described Drive as backup only; Drive is now the shared store with autosave, multi-device merge, checkpoints and media, and a vault key separate from the DB key.
- Spec 09 rule "images show their alt text only (nothing is fetched)": attachment images now render from the backend; external images still do not.
- New crate `minimap-sync` (network, crypto, sync engine); new dependencies `reqwest`, `chacha20poly1305`, `sha2`, `base64`.
- New permissions: a custom URI scheme for media, a file-picker read through the existing dialog permission, and opening a file with the OS default app (done by the backend; no new webview permission).

## Prerequisites (you)
- Google Cloud project, OAuth client ID **and secret** (Desktop app type; Google requires both for desktop clients and treats the secret as non-confidential), Drive API enabled. Paste them in Settings → Google Drive → Advanced, or build with `MINIMAP_GOOGLE_CLIENT_ID` / `MINIMAP_GOOGLE_CLIENT_SECRET` set.
- Consent-screen verification before public distribution.

## Acceptance criteria
- [ ] Connect, autosave, pull, merge, checkpoint recovery and disconnect work end to end against Google Drive. *(needs your OAuth client. The engine is tested end to end with two simulated devices over a folder remote (`engine_tests.rs`), and the real Drive client, resumable upload and sign-in against a mock Google server (`drive_tests.rs`); the first connection to real Google Drive is a manual check)*
- [x] A change is on Drive within 30 s of being made while online; quitting right after a change still saves it (or queues it for next start when offline). *(engine `saving_waits_for_a_quiet_moment_but_never_longer_than_thirty_seconds`, `unsaved_changes_survive_a_restart_and_go_up_at_the_next_start`; the app flushes on `ExitRequested`)*
- [x] Second device: connect, enter the recovery key: the same data and attachments appear. Edits on either device show up on the other within about 30 s, without any prompt. *(engine `a_second_device_adopts_the_data_with_the_recovery_key_and_a_wrong_key_changes_nothing`, `a_change_on_one_device_reaches_the_other_and_then_everything_goes_quiet`, `attachments_travel_encrypted_and_are_fetched_when_first_opened`; the poll is every 15 s)*
- [x] Two devices editing offline, then reconnecting: all non-conflicting edits from both are present on both; for the same item the later save wins; both devices end up with identical data. *(engine `edits_made_offline_on_both_devices_merge_when_they_reconnect`; store `the_later_write_wins_in_both_directions_and_the_devices_converge`, `work_on_different_items_is_all_kept`)*
- [x] Merging is repeatable: merging the same snapshot twice, or in either order, gives identical data. Hard deletes stay deleted. Repairs (handles, loops, dangling links) are listed in the sync report. *(proptests `two_devices_always_converge` (600 cases run once) and `the_order_of_syncing_does_not_matter`; `a_deleted_item_stays_deleted_everywhere`; the repair tests for handles, "me", loops, teams, dangling pointers)*
- [x] Killing the app or the network mid-upload leaves the previous Drive snapshot intact and the next save succeeds. *(engine `an_interrupted_upload_keeps_the_old_snapshot_and_the_next_try_succeeds`; drive `big_files_go_up_in_chunks_and_replacing_keeps_the_old_file_until_complete`, `a_dropped_connection_resumes_from_where_the_server_got_to`)*
- [x] Every uploaded file (snapshot, checkpoint, media) is unreadable without the vault key; a wrong recovery key is refused without changing anything. *(store snapshot test checks the file for plain text and the key; sync crypto tests for tampering, truncation, reordering; engine tests scan every file on the remote for the key; `a_second_device_adopts_*`)*
- [x] Attach an image to a note: it renders in the preview, survives cache eviction (re-downloaded), and the same file attached twice is stored once on Drive. *(core render tests, command `a_picture_is_stored_encrypted_served_to_the_webview_and_removed`, `the_same_file_is_one_blob_*`, media `eviction_*`, engine `attachments_travel_*`; the preview itself needs a manual look)*
- [x] Without Drive, the status bar says "Local only" and the This week banner appears; dismissing it brings it back after 7 days. *(command `a_new_install_is_local_only_*`, `settings_keep_the_client_the_device_name_and_the_banner_choice`, UI `sync_status` tests; the screens need a manual look)*
- [x] Retention deletes only app-created checkpoints beyond the limits and unreferenced media. *(core `checkpoint_prune_plan` tests, engine `checkpoints_are_kept_hourly_pruned_and_only_this_devices_are_touched`, `files_nothing_refers_to_are_removed_from_drive_only_after_the_grace_period`)*

## Decisions (your answers)
- **Drive stays optional**, but the user must be clearly warned while data is local only (status bar chip, This week banner, Settings, first-run notice).
- **Seamless multi-device sync**, not read-only/take-over: automatic row-level merge, newest write wins, repairs reported.
- **Defaults**: 2 GB media cache, 250 MB per file, 24 hourly + 30 daily checkpoints.
- **Attachment kinds**: images, SVG, Markdown, Excel, PowerPoint, Word and similar documents (list above); no executables.

## Implementation notes
- **Crates**: `minimap-sync` (new; no SQL) holds `crypto`, `vault`, `remote` (+ `folder` test remote), `drive`, `oauth`, `media`, `engine`; `minimap-store` gained `merge`, `snapshot`, `attachments`, `meta` and migration 0008; `minimap-core` gained `sync` and `attachments` (pure); `src-tauri` gained `sync.rs`, `commands/sync.rs`, `commands/attachments.rs` and the `minimap-media` protocol; the UI gained `sync_status`, `drive_settings` and `attachments` components. Details in `docs/knowledge-graph.md` (§3.4, §6, §6b, §7).
- **Merge** is row by row, newest `updated_at` wins, ties broken by a content hash (so every device chooses the same copy); tombstones carry `deleted_at` and `lifted_at` so a recovery from a checkpoint beats an older delete and a later delete beats the recovery; repairs never stamp a new `updated_at`, so every device makes the same repair. Property tests found and fixed: devices recording different delete times, equal settings written at different times, and handle renames that depended on merge order (now derived from the project's id).
- **Change tracking**: `AppState::run_vault` compares the connection's `total_changes()` around every command, so every write path (including future ones) marks changes to save, with no per-command code. The engine's own writes go through `Host::with_db` and are not marked.
- **Snapshots** use `sqlcipher_export` into a file keyed with the vault key (the backup API refuses plain -> encrypted), then drop device-local rows and `VACUUM`.
- **Polling, not events**: the UI reads `get_sync_status` every 4 s and reloads every screen when `data_revision` moves; no event permission was added to the capability.
- **Chunk uploads are sent once.** A lost answer is resolved by asking the server how much it has (Drive's protocol), not by re-sending.
- **The attachment cache limit** (2 GB) and the per-file limit are constants for now; there is no Settings control for the cache size yet.
- **Device name** defaults to the host name (`HOSTNAME`/`COMPUTERNAME`, else the `hostname` program) and can be changed in Settings.
- **Not verified here**: a real sign-in and real Drive traffic (no OAuth client in the development environment), the real OS keychain (none in the development environment; the refresh token falls back to the local database), and how the new screens look and feel. The app was started against a throwaway data directory (migration to schema 8, database ready, sync thread running, no panics).
