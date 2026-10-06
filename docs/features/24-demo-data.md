# 24 — Demo data

Status: Implemented — awaiting manual check · Milestone: M2 · Priority: Must
Depends on: 03–07

## Goal
A realistic dataset for manual testing, snapshot tests and exploration.

## Scope
**In** (built)
- `seed_demo_data` command, **debug builds only**: the backend refuses in a release build (`cfg!(debug_assertions)`), and the **Settings → Developer** tab that offers it is only shown in debug builds (`cargo tauri dev`; the release `trunk build` drops it). It is also refused while **Google Drive is connected**: the seed is written like any other change, so it would be saved to the user's Drive and copied to their other computers.
- **Seed builder in `minimap-store`** (`demo::seed(conn, today)`), so tests reuse it. It fills an **empty** database (nothing but, possibly, first run's "me"; otherwise `invalid`) in **one transaction**, all or nothing. "Me" is one of the 8 people.
- **Deterministic for a given `today`**: every date is relative to it (weekend days move to a weekday), so a fixed date gives the same titles, dates, statuses, estimates and links every time (the snapshot tests use Wednesday 2027-03-03). The generated ids are the only thing that differs between runs.
- Contents: 2 objectives; 3 projects (**EU Region** is amber: projected 2 working days late, one overdue and one blocked task of 12; Platform Cost Reduction and Security and Compliance are green); 40 tasks (14 + 14 + 10 + 2 in the inbox) with 23 `blocks` links, 4 of them across projects, a few tasks without estimates, one overdue, one blocked, one cancelled, five done; 8 people in 2 nested teams (Engineering > Platform) with 7 reporting lines, **one overloaded** (Tomás Alvarez, 220% in the first week); 3 notes (a 1:1 with mentions and a checklist, a meeting, a general note); 5 decisions (one proposed, one replaced by a newer one); 3 waiting-ons (one stale, one resolved this week).
- A realistic history: the creation activity is dated three weeks back, and **this week's events** (a task finished, two due dates moved later, a task newly blocked, a decision made that replaces an older one, a waiting-on resolved) are dated today, so the weekly review has something to say.

**Out**
- **"Load demo data" on first run and "Clear demo data"**: not built. Decision below.

## Acceptance criteria
- [x] Seeding an empty DB produces the counts above (store test `seeding_an_empty_database_produces_the_documented_counts`; also: every link passes the edge rules and nothing loops, only an empty database is accepted and a refusal changes nothing, any day of the week seeds cleanly, the history reads like a real week).
- [x] Snapshot tests for 13, 14, 15 and 19 use this dataset (`src-tauri/src/commands/demo.rs`, snapshots in `src-tauri/src/commands/snapshots/`; the screens' `*_at(conn, today, ..)` functions take a fixed date). Review a change with `INSTA_UPDATE=always cargo test -p minimap demo` and read the `.snap` diff.
- [ ] Click-through (see below).

## Decisions
- **Offer demo data to end users on first run, or developer-only?** (your answer) **Developer-only.** Nothing on first run changes; the offer exists only in the Developer tab of a debug build.
- **No "Clear demo data".** The data is ordinary data (no marker), because a marker would have to survive merges and hard-delete tombstones for something only developers use. To start again, use a fresh data folder or restore an empty backup.
- **Ids are not fixed.** The repositories generate their own ids; making them deterministic would mean rewriting ids across every table, link, activity row and search index. The snapshot tests render titles and dates, never ids.

## Implementation notes
- Types: `DemoSummary` (counts, `describe()`).
- Store: `demo::seed`; the repositories gained `teams::create_in_tx`, `waiting_on::update_in_tx` and `decisions::supersede_in_tx` so the whole seed shares one transaction. The history is dated with a final pass of direct SQL inside the same transaction (activity, created/updated times, completion times).
- Commands: `seed_demo_data` (`commands/demo.rs`); `schedule_at`, `with_world_at`, `overview_at`, `review_at`, `report_at` (the existing `*_impl` functions call them with today).
- UI: `components/developer_settings.rs`, `Tab::Developer` (`Tab::visible()`), `api::seed_demo_data`.

## Not yet verified by hand
- In `cargo tauri dev`, Settings → Developer shows the card; **Add demo data** on a fresh data folder fills every screen (This week, Overview, Projects, Dependencies, Capacity, People, Weekly review) and the toast lists the counts; pressing it again says the database is not empty
- with Google Drive connected the button is refused with the reason
- a release build shows no Developer tab and `/settings?tab=developer` opens General
