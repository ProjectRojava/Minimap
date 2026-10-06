# 15 — Health scoring and Overview

Status: Implemented — awaiting manual check · Milestone: M3 · Priority: Must
Depends on: 13

## Goal
The "minimap": one screen that tells an executive what's at risk and why, without them asking.

## Scope
**In**
- Project health in core from: projected finish vs target, share of tasks blocked or overdue, share of unestimated work. Output: level (`green`, `amber`, `red`), score, and reasons ("3 tasks overdue; projected 6 days late").
- Objective health: weighted roll-up of contributing projects (`contributes_to.weight`).
- Overview screen: objectives with health; projects nested under them; top-5 risks; overloaded people (from 17, or task-count flag if 17 isn't built); stale waiting-ons.
- Command: `get_portfolio_overview()`.

## Acceptance criteria
- [x] Demo data's at-risk project shows amber/red with correct reasons. *(a hand-built "at risk" project in the core tests: 3 working days late, one overdue task, one unestimated, with the exact reason texts; red with stricter thresholds. With the real demo data (spec 24) EU Region is amber, projected 2 working days late with one overdue and one blocked task of 12, and the rest green; the `the_overview_of_the_demo_data` snapshot also asserts exactly one overloaded person and one stale waiting-on.)*
- [x] Clicking any item opens its detail pane. *(every row opens its node; unverified by hand)*
- [x] Overview loads in < 200 ms with demo data. *(40 projects / 2,000 tasks / 30 people: 7 ms release, 29 ms debug; `cargo test -p minimap --release overview_speed -- --ignored --nocapture`)*

## Decisions
- **Thresholds are configurable** (your answer): Settings → Project health has an amber and a red value for each of three signals, validated (amber ≤ red; days 1-365, shares 1-100), stored as one settings value, with "Reset to defaults". Defaults: projected late **1 / 5** working days past the target; blocked or overdue **15 / 35 %** of open tasks; no estimate **50 / 80 %** of open tasks.
- **How a project is scored.** Each signal gets its own 0-100 score from its thresholds (green 61-100 below amber; amber 26-60 from amber to red; red 0-25 from red, bottoming out at twice red). The project's score is the **worst signal's**, and the level follows the score (above 60 green, above 25 amber, else red), so the level always matches the reason that caused it. Reasons are listed worst first: "projected 3 working days late (target 2027-03-05)", "3 tasks overdue, 1 blocked (3 of 10 open)", "6 of 10 open tasks have no estimate"; a healthy project says why too ("on track: projected 2027-03-02, target 2027-03-12 (8 working days to spare)", or "no target date, so lateness can't be judged").
  - Lateness is the schedule's (spec 13) projected finish against the target. Overdue = open with a due date before today; blocked = status Blocked; a task that is both counts once.
  - **Not scored ("idle")**: done, cancelled, paused, or no open tasks. Planned projects are scored like active ones.
- **Objective health = weighted roll-up** of contributing projects (by `contributes_to.weight`, default 1; a done project counts as healthy, paused/cancelled/idle ones are skipped) **and directly contributing tasks** (overdue red, blocked amber, done healthy). Two additions to a plain average, because an average hides trouble: **one red contributor caps the objective at amber**, and an objective with its own target date is also judged on its **projected lateness** (the latest finish of what feeds it against that target, with the same late thresholds). Marked-done objectives and ones with nothing active are not scored. Your own status (on track / at risk / ...) is shown next to the computed health, not replaced.
- **Top-5 risks: severity × importance, not literally days late × priority.** `risk = (100 − health score) × priority weight × scope weight`. Severity from the health score already folds days late in *relative to your thresholds* (6 days late on a 2-week project is worse than on a 6-month one only if you tune it so, and 5 days is red by default whatever the priority), and it also covers overdue/blocked work. Priority weights: P1 1.5, P2 1.25, P3 1.0, P4 0.8, P5 0.6. A project **borrows the priority of the highest-priority live objective it feeds** ("P1 via North star"), so a late project behind a top objective outranks an equally late side project. Scope weight: a project 1.0, a single task 0.6. Candidates: every amber/red project, and open **priority 1-2 tasks that are overdue or blocked** (so a P1 overdue task shows up, but ranks below a red project of ordinary priority). Ties: lower health score, then name. The overview shows five and says how many more there are.
- **Overloaded people (17 isn't built)**: scheduled work in the **next five working days** (each task's overlap with that window × its `allocation_pct`) against capacity (`weekly_capacity_hours / hours per day` days), flagged above 100%. Spec 17 grows this into the week-by-week heatmap; this is its first week.
- **Stale waiting-ons** reuse the waiting-on rules (age setting, expected date), oldest first.
- **Health marks** in a monochrome app: red = filled dot in the danger colour, amber = half-filled dot, green = empty dot, not scored = dash; the word is the tooltip and the accessible name.
- A loop in `blocks` (the UI prevents one) doesn't break the overview: it shows a warning, and lateness and workload are skipped.

## Implementation notes
- Core `health` (signal scoring, `project_health`, `task_health`, `objective_health`, risk weights; levels from scores), core `overview::build(world)` (rows, objectives with nested projects worst-first, unlinked active projects, risks, overload). Types in `minimap-types::health` (`Health`, `HealthLevel`, `HealthReason`, `HealthThresholds` + `validate`, `PortfolioOverview`, ...). `Settings.health` stored as one JSON value. Command `get_portfolio_overview()` adds the stale waiting-ons.
- UI: the **Overview** screen (replaces the ping demo): counts, top risks, objectives with their projects, projects without an objective, overloaded people, stale waiting-ons, a legend of the current thresholds. A **Health** section in the project and objective panels (level, score, reasons). `components/health.rs` (marks and wording, tested), Settings → Project health.
- "Computed-health marker" promised by spec 04 is the objective's Health section and the Overview's marks.

## Not yet verified by hand
- Overview with a project past its target date and an overdue task: it shows amber/red with the reasons, appears under Top risks, and nests under its objective; clicking any row opens the pane
- a project feeding a priority-1 objective ranks above an equally late ordinary one ("P1 via …")
- project panel → Health shows level, score and reasons; objective panel too
- Settings → Project health: change late-amber/red; the Overview verdicts and the legend follow; amber above red is refused with a message; Reset restores the defaults
- a person with more than a week of work scheduled in the next five days appears under Overloaded; a stale waiting-on appears under Stale waiting-ons and opens its pane
- an empty workspace shows the "Nothing to show yet" hint
