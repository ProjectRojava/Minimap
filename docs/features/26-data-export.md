# 26 — Full data export

Status: Implemented — awaiting manual check · Milestone: M4 · Priority: Should
Depends on: 01

## Goal
Show users they're not locked in: export everything in open formats.

## Scope
**In** (built)
- **`export_all(path, format)`** writes a **new folder** `minimap-export-YYYYMMDD-HHMMSS` inside the folder the user picked (Settings → Data & backup → *Export your data*, with the folder picker). Nothing existing is overwritten: the export is built as `<name>.part` and renamed when complete (a name already taken gets `-2`, `-3`, ...), and a failure removes the half-written folder.
- **JSON** (always): one file per node type (`objectives`, `projects`, `tasks`, `people`, `teams`, `notes`, `decisions`, `waiting_on`), `edges.json`, `attachments.json` (the rows), `activity.json` (the whole history, oldest first), and `manifest.json` (`format: "minimap-export"`, `format_version`, app version, schema version, `exported_at`, per-file record counts, attached-file counts). Each file is a pretty-printed list of exactly the structs the app uses. **Archived items and removed links are included** and marked by `archived_at`, so the export is everything, not what is currently visible.
- **Attached files**: the ones that are on this computer are written decrypted to `attachments/<id>/<name>`. Files that exist only on Google Drive are not downloaded (an export is not a sync) and are counted in the manifest and the result, and still listed in `attachments.json`.
- **Markdown** (`JsonAndMarkdown`, the checkbox): `projects/<name>.md` (details, owner, dates, objectives, depends on / needed by, tasks as a checklist with assignee, due date, estimate, priority and what blocks them, decisions, waiting-ons), `inbox.md` (tasks without a project), `notes/<date>-<title>.md` (mentions become `@Name`, picture and file links point at the exported files). File names are made safe and unique.
- `README.md` in every export says what the files are and how they fit together.
- Types: `ExportFormat`, `DataExport`, `AttachmentRecord`, `ExportManifest`, `ExportAllResult`.

**Out**
- Import (later; possibly paired with Jira/Linear importers).
- Settings, Google Drive state, device-local data (keys, tokens) and the media cache's encrypted blobs: not user content.

## Acceptance criteria
- [x] Export of demo data round-trips through `serde` into the same structs (`the_demo_data_round_trips_through_the_files_into_the_same_structs`: the eleven files are read back into one `DataExport` and compared with what the database held, after archiving a task so archived rows and removed links are in it too).
- [ ] Click-through (see below).

## Decisions
- **Need JSON import in the MVP to make export useful? No.** The point is that the data is readable, complete and documented (`README.md`, `manifest.json`, plain JSON and Markdown), not that it round-trips into this app; import is its own feature and is listed as out of scope. The files are serialized from the app's own structs, so an importer later is a `serde` read.
- **A new folder inside the chosen one**, not the chosen folder itself: nothing the user keeps there can be overwritten or mixed up with export files.
- **Everything, archived included.** An export that silently dropped archived items would be a quiet form of lock-in; `archived_at` lets a reader filter.
- **Plain text, said plainly.** The card, the README and the result say the export is not encrypted even when the database is. There is no option to encrypt it: a folder of ordinary files is the point.
- **Attachments from the local cache only.** Pulling every file from Drive could be gigabytes and needs the network; the missing ones are counted and listed instead of being faked.

## Implementation notes
- Store: `export::collect(conn) -> DataExport` (every list with archived included, `edges::list_all`, `attachments::list_records`, `activity::list_all`).
- Core `export_md` (pure): `markdown(&DataExport, attachment paths) -> Vec<MdFile>`, `readme(markdown)`, `rewrite_attachments`, `encode_link`; user text goes through `report::escape`.
- Command `export_all` (`commands/export.rs`): checks the folder (full path to an existing folder, else `invalid`), reads the data under the database lock, then writes the files outside it (`write_export`).
- UI: `components/export_settings.rs` (checkbox, *Export…*, result with the folder, counts and attachment line); `api::pick_folder(title)` now takes the dialog's title.

## Not yet verified by hand
- Settings → Data & backup → Export your data → Export…: pick a folder; a toast and a result line name the new folder, the file count and size, and what it holds
- the folder has the JSON files, `manifest.json` and `README.md`; with the checkbox on, also `projects/`, `notes/` and `inbox.md`; open a project file and a note in a Markdown viewer
- an attached picture shows in the exported note (the link points into `attachments/`)
- a second export makes a second folder; the chosen folder's other files are untouched
- the same with an encrypted database
