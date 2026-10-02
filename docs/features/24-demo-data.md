# 24 — Demo data

Status: Draft · Milestone: M2 · Priority: Must
Depends on: 03–07

## Goal
A realistic dataset for manual testing, snapshot tests and first-run exploration.

## Scope
**In**
- `seed_demo_data` command (debug builds, and Settings → Developer). Deterministic: fixed ids/dates relative to a fixed "today" for tests.
- Contents: 2 objectives; 3 projects (one at risk); ~40 tasks with cross-project `blocks`; 8 people in 2 nested teams with a reporting structure, one overloaded; a few notes (one 1:1 with mentions), decisions and waiting-ons (one stale).
- Seed builder lives in `minimap-store` (or a test-support module) so tests can reuse it.
- Optional: "Load demo data" offer on first run with an empty DB, plus "Clear demo data".

## Acceptance criteria
- [ ] Seeding an empty DB produces the counts above.
- [ ] Snapshot tests for 13, 14, 15, 19 use this dataset.

## Open questions
- Offer demo data to end users on first run, or developer-only?
