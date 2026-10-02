# 26 — Full data export

Status: Draft · Milestone: M4 · Priority: Should
Depends on: 01

## Goal
Show users they're not locked in: export everything in open formats.

## Scope
**In**
- Export all nodes, edges and activity to a folder: one JSON file per node type + `edges.json` + `activity.json`, with a `manifest.json` (app version, schema version, exported_at).
- Optional Markdown export: one file per project with its tasks, and notes as `.md` files.
- Command: `export_all(path, format)`.

**Out**
- Import (later; possibly paired with Jira/Linear importers).

## Acceptance criteria
- [ ] Export of demo data round-trips through `serde` into the same structs.

## Open questions
- Need JSON import in the MVP to make export useful?
