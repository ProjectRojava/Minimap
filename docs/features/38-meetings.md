# 38 — Meetings

Status: Implemented — awaiting manual check · Milestone: — · Priority: Should
Depends on: 06, 27, 31, 32, 33 · ADR-0018

## Goal
A kind of task that happens at a time: it must have a day and a start, it moves itself to *in progress* when it starts and to *done* when it ends, and later meetings can follow it up.

## Scope
**In** (built)
- **Built-in type**: `MEETING_TYPE = "meeting"`, always in Settings' list (`task_types::ensure_meeting`, `normalise` un-archives it; renamable and recolourable; the settings row says "Built in"). New installs list it last (hue 75).
- **Data** (migration 0015): `tasks.start_minute` (0-1439) and `tasks.length_minutes` (none = `DEFAULT_MEETING_MINUTES` 60), both nullable, floating wall-clock (ADR-0018). The day is `due_date`. `Task::is_meeting`, `meeting_minutes`, `minutes_to_start/end`; `Clock{date, minute, utc_offset_minutes}`; `fmt_clock/parse_clock` (`9:30`, `0930`), `fmt_length/parse_length` (`45m`, `1h30m`), `fmt_range`. `UpdateTask.start_minute/length_minutes` are `Patch` fields (undo `nullable_fields`).
- **Mandatory time**: `core::meeting::validate` (a meeting needs a day and a start; start < 1440; length 1..=1440 minutes) runs in `store::tasks::validate`, so create and update both refuse. A task that stops being a meeting loses its time. A task becomes a meeting only through `set_meeting` (day and time with the type).
- **Moves itself**: `core::meeting::next_status(task, clock)`; `store::meetings::advance(conn, clock)` applies it in one transaction (done at the end instant, `tasks::update_in_tx_at`; a repeating meeting that finishes makes the next one). Command `advance_meetings(clock)` (not an undo step; clock more than 2 days off or an offset beyond ±14 h is refused); the screen calls it on mount and every 30 s (`MeetingClock`, which also keeps `NowClock` fresh). A meeting that ended while the app was closed goes straight to done. Blocked, done and cancelled meetings are left alone. Moving a started or done meeting to a later time reopens it (`meeting::reopens_when_moved`, in `meetings::set_time`).
- **Making and moving**: commands `create_meeting`, `set_meeting(id, when, clock)`, `schedule_follow_up(source, when?)`; *New meeting* button (Tasks board and list headers) and palette action open `MeetingDialog`; the task pane's Type box asks for the day and time when *Meeting* is picked (`MakeMeeting`) and a meeting's pane shows `MeetingFields` (date, start, length; saves on change) instead of start/due/estimate. Quick-add refuses `type:meeting` with a reason.
- **Follow-ups**: edge `follows_up` (follow-up -> original; matrix Task -> Task), rules in `core::meeting::check_follow_up` (one original, any number of follow-ups, no loop) and `store::meetings::check_follow_up` (both are meetings), called from `check_new_edge`. `meetings::create_follow_up` (one transaction, one undo step): title "Follow-up: …", same project, priority, assignee, objectives, links and length, a week later at the same time unless the dialog says otherwise. The pane's *Follow-ups* section (meetings only) lists the original and the follow-ups with day and time, *Schedule follow-up…*, and *Follows up on…* / *Add a follow-up…* pickers. A repeating meeting's next one follows up on the one that ended (`make_next`).
- **Out of the plan**: `store::tasks::list_planned` feeds schedule, impact, overview, capacity and the dependency graph; the weekly review filters meetings; no focus star; no deadline tint on the board or list; sorted by start time within a day (`core::tasks::sort`, board sorts).
- **This week**: `ThisWeek.meetings` (open meetings from today through Sunday by day then start; in no other list, never a red flag), `WeekDay.meetings` (the strip says "2 meetings"); a *Meetings* card under the strip (time range, *NOW*, "in 25 min" from `NowClock`, tick = end now, *Follow-up…*, the notes under each).
- **Export**: the Markdown task line reads `meeting 2027-03-05 10:30–11:30`; the JSON carries the new fields.
- **Demo data**: five meetings (today 16:00, tomorrow 14:00 with a follow-up next week, a weekly sync held Monday 10:00 and its repeating next one linked).

**Out**
- Attendees (a task has one assignee), reminders/notifications before a meeting, calendar import, capacity counted from meeting length, a quick-add time token, a "missed" status (missed meetings become done, by decision), a setting for the default length (60 minutes for now), a time on ordinary tasks.

## Acceptance criteria
- [x] A meeting can't be made or changed without a day and a time; a task isn't a meeting by changing its type alone; leaving the type drops the time (`store tests/meetings.rs`).
- [x] The clock starts and ends meetings (own length, default hour, across midnight), catches up a missed one at its end instant, and leaves blocked/done/cancelled/non-meetings alone (`core::meeting`, `store tests/meetings.rs`, `commands::meetings`).
- [x] Moving a started or done meeting later reopens it; the same time again doesn't.
- [x] A repeating meeting makes the next one at the same time, linked to the last (`store`).
- [x] Several follow-ups; one original; no loops; only meetings; the follow-up takes project, priority, assignee, links; undo takes each step back exactly (`store`, `undo`).
- [x] Meetings are out of the schedule, capacity, health, graph, impact and review even when linked to tasks (`commands::meetings`).
- [x] This week: own list by day and time, never flagged, strip counts; notes under them (`core::this_week`, demo test through real data).
- [x] Pure text of the screens; Help describes it (the `meetings` page).
- [ ] Click-through (see below).

## Not yet verified by hand
- *New meeting* on the Tasks screen: title, date, `10:30`, `1h`: it opens in the pane with Date / Starts / Length, and shows on This week under the strip
- At the start time the card moves to *In progress* and *NOW* shows; an hour later it is *Done* (try a meeting two minutes ahead with length `2m`)
- Close the app with a meeting in the past still to do, reopen: it is done
- Type box on an ordinary task → *Meeting*: asks when; *Cancel* puts the Type box back
- *Schedule follow-up…*: a new meeting a week on is linked; the *Follow-ups* section lists both ways; a second one works; linking a meeting that already follows another is refused
- Give a meeting a weekly repeat, end it: the next one is there at the same time and follows up on it
- A meeting that blocks a task changes nothing on Schedule or Capacity
- Ctrl/Cmd+Z after a follow-up undoes the follow-up, not a status the clock moved
