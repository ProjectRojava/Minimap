# 08 — Waiting-on

Status: Implemented — awaiting manual check · Milestone: M3 · Priority: Must
Depends on: 03, 07

## Goal
Track what other people owe the user, so nothing silently stalls.

## Scope
**In**
- Fields: description, person_id, asked_on (default today), expected_by, resolved_on.
- Optional `about` edge to a task or project.
- List sorted by age (oldest first); stale (older than the **stale threshold setting**, default 7 days, or past expected_by) highlighted.
- One-click resolve; reopen.
- **Snooze / follow-up date**: hide until a date, then resurface.
- Shown on the person detail and on the linked task/project.
- Commands: `create_waiting_on`, `update_waiting_on`, `resolve_waiting_on`, `archive_waiting_on`, `get_waiting_on(open_only)`.

## Acceptance criteria
- [x] Create, resolve and reopen a waiting-on. *(command tests; screens unverified by hand)*
- [~] Stale items are highlighted and appear on Overview/This week. *(highlighted on the Waiting On screen; Overview (15) and This week (16) don't exist yet and will read `get_waiting_on` and its `stale` flag when they land)*
- [x] Snoozed items are hidden until their date. *(core + command tests, including the day it resurfaces)*

## Decisions
- **Stale threshold is a setting** (Settings → Waiting on, default 7 days, 1–365).
- **`follow_up_on` is its own column** (migration 0006), separate from `expected_by`: "expected by" is when the other person promised it; "snoozed until" is when you want it to reappear in your list.
- **Stale** = open, not snoozed, and either asked *more than* the threshold days ago (exactly 7 is not stale) or past `expected_by` (due today is not yet late). **Overdue** = open, not snoozed, past `expected_by`; overdue always counts as stale.
- **Snoozed** = open and `follow_up_on` is in the future. It is hidden from the default list, is never stale while snoozed, and resurfaces *on* the follow-up date. Resolving ignores any snooze.
- Default list: open, not snoozed, oldest first. "Show snoozed" and "Show resolved" reveal the rest.
- "Today" is the UTC date (like note and task defaults), so near midnight a user far from UTC can see a day's difference.
- Resolving twice keeps the first resolved date.

## Implementation notes
- **Core** `minimap-core::waiting_on`: `age_days`, `is_snoozed`, `is_overdue`, `is_stale`, `arrange` (filter + flags + order), with a proptest that the flags are consistent.
- **Store**: migration `0006_waiting_on_follow_up.sql`; `WaitingOn.follow_up_on` in create/update (changes are logged in the activity diff); `views::waiting_on_items` (person and `about` target); `settings.stale_waiting_days`; `store::today()`.
- **Commands**: `get_waiting_on(filter_by)`, `get_waiting_on_detail`, `create_waiting_on`, `update_waiting_on`, `resolve_waiting_on`, `reopen_waiting_on`, `snooze_waiting_on(id, days)` (`None` ends the snooze), `archive_waiting_on`. A waiting-on can't be created for, or moved to, an archived person.
- **UI**: the Waiting On screen (age, description, who, about, expected by; stale items in heavier text with a "stale" chip, overdue dates heavier; "until <date>" / "resolved <date>" chips; per-row Resolve / Reopen and a Snooze select with Wake; filters for person, snoozed and resolved; a New form with an optional About target) and a detail panel (status line with the same actions, editable fields including asked-on, expected-by, snoozed-until and resolved-on, archive). Keys on the list: `n` new, `x` resolve or reopen, `s` snooze three days.
- **About link**: set when creating, or any time under Links (About). The linked task or project shows its waiting-ons under "Waiting-ons" in its Links, and a person's panel lists their open ones.
- Settings screen gained "Stale after (days)" under a Waiting on heading.

## Not yet verified by hand
- `n`, fill the form (who from is required), optionally About a task or project; it appears and opens in the pane
- an item asked more than the threshold days ago (set Asked on in the pane) shows heavier text and a "stale" chip; past Expected by shows a heavier date
- Resolve then Show resolved then Reopen; `x` does the same
- Snooze 1 day: the row disappears; Show snoozed reveals it with "until <date>"; Wake brings it back; a follow-up of today or earlier means it is not snoozed
- change "Stale after (days)" in Settings: stale emphasis follows
- the linked task/project's Links lists it under "Waiting-ons"
