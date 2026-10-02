# 23 — Settings

Status: Draft · Milestone: M4 · Priority: Must
Depends on: 02

## Goal
One place for app configuration.

## Scope
**In**
- Sections: General (theme: system/light/dark; working days; hours per day; default weekly capacity; stale waiting-on days), Data (DB location read-only + "Show in folder"), Backup (20, 22), Security (21), Developer (seed demo data in debug builds, 24).
- Settings stored in a `settings` table (key/value JSON) in the DB.
- Commands: `get_settings`, `update_settings`.

## Acceptance criteria
- [ ] Every setting persists across restarts and takes effect without restart where possible.

## Open questions
- Allow moving the DB to a different location?
