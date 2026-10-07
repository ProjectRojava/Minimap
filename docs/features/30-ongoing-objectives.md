# 30 — Ongoing objectives

Status: Implemented — awaiting manual check · Milestone: — · Priority: Should
Depends on: 04, 15, 16, 19

## Goal
Some objectives never end ("keep internal systems healthy"). Say so, judge them by their work and a regular review instead of a date, and keep them from being counted as late or done.

## Scope
**In** (built)
- **Kind** on an objective: *Goal* (has an end) or *Ongoing* (`Objective.ongoing`, migration 0011). Ongoing: no target date, never "done" (archive it), both enforced in the store; switching a goal to ongoing clears its date and turns "done" into "on track". The new-objective form has an *Ongoing* box (monthly review by default); the pane has *Kind*.
- **Review rhythm**: `review_every_days` (1-365; the pane offers never, week, 2 weeks, month, quarter, 6 months, year, or shows an odd existing value) and `last_reviewed_on`; the next review is due one rhythm after the last (or after creation); *Mark reviewed* sets today.
- **Health**: no lateness; an overdue review holds it at amber with "review overdue by N days (due D, every M days)"; the contributor roll-up is as before. Pills in the pane: *ongoing*, and the review date (grey, amber in its last week, red when overdue).
- **Lists**: the Objectives list has an *Ongoing* group last in either layout and an *ongoing* / amber *review due* pill in the date column; the Overview, the weekly status report ("Ongoing, review overdue 5d") and the weekly review's objective table say "ongoing".
- **Prompts**: *This week* has an "Ongoing objectives to review" section and the Weekly review a new step 7, "Ongoing" (the report is step 8); both list reviews overdue or due by Sunday, with *Mark reviewed*.
- Undo covers kind, rhythm and reviewing; the full export carries the fields (`objectives.json`); the demo data has "Keep internal systems healthy" (ongoing, monthly, last reviewed 35 days ago, served by the two repeating security tasks).

**Out**
- A "nothing finished in N days" signal; review reminders outside the app; a review history (only the last date is kept; the activity log has the rest); a review cadence on goals.

## Acceptance criteria
- [x] An ongoing objective can't have a target date or be done; switching kind normalises both (store `tests/ongoing_objectives.rs`).
- [x] Review due dates and overdue days are right (`Objective::review_due`, core `objectives` tests); an overdue review makes it amber with the reason (core `health`, demo snapshot).
- [x] It appears in its own list group, on This week and in the weekly review when due (core `objectives`, `this_week`, `weekly_review` tests).
- [x] Everything is undone and redone (store `undo.rs`).
- [ ] Click-through (see below).

## Decisions
- See ADR-0014 (fields on the objective; amber for a late review; no staleness signal).

## Not yet verified by hand
- Add an objective with *Ongoing* ticked: the list shows it under *Ongoing* with an *ongoing* pill; open it: *Kind* is Ongoing, *Review* every month, no target date, *Done* is not offered in the assessment
- Set *Review* to every week and mark nothing: after a week (or in the demo data, which is 5 days overdue) the pane shows *review overdue*, the Overview row is amber, and This week lists it; *Mark reviewed* clears all three
- Switch a goal with a target date to Ongoing, then back: the date is gone; set a new one
