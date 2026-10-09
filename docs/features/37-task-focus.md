# 37 — Focus: keep a task on This week whatever its deadline

Status: Implemented — awaiting manual check · Milestone: — · Priority: Should
Depends on: 06, 16, 36

## Goal
Let the user choose tasks to see on This week **every day**, even when the due date is months away or missing, without turning focus into a date, a status or a plan.

## Scope
**In** (built)
- **Data**: `tasks.focus` (migration 0014), JSON text or NULL: `{}` = in focus until taken out, `{"until":"YYYY-MM-DD"}` = in focus through that day (inclusive). Types: `Focus { until: Option<Date> }` (`PINNED`, `is_active(today)`, `describe`), `FocusChoice { Off, Pinned, Today, Until{date} }` (what the box offers; `FocusChoice::of(focus, today)` reads a stored focus back), `Task.focus`, `CreateTask.focus`, `UpdateTask.focus: Patch<Focus>`. An unreadable stored value is no focus (the task still loads). Focus is a way of looking: it never touches dates, schedule, health, capacity or impact.
- **Choosing**: `set_task_focus(id, choice)` (core `focus::resolve(choice, today)`: *Today* = until today; a past date is refused with the date named). A normal logged update, so it is in the history and **undoable** (`focus` is in `store::undo::nullable_fields`). `update_task` can set it too.
- **This week** (core `this_week::build_at`): a task is in focus on a day when `focus` is set, not past `until`, and the task is open. `ThisWeek.focus: Vec<FocusTask{task, quiet_days}>` holds the focused tasks that have **no red flag**, ordered like the plan (priority, then due date, undated last); `ThisWeek.focus_count` counts every open focused task, flagged or not. **Red flags win** (Option A): a focused task that is overdue, due today or blocked stays in *Needs attention* (its star is lit), and a focused task due later this week is in *Focus*, not repeated under *Priorities this week*. Every task is still listed once. A past-day view judges `until` against that day.
- **Nudges**: more than `FOCUS_SOFT_LIMIT` (5) in focus shows a line over the section ("N tasks are in focus. Past 5 nothing stands out: take some out."); a focused task whose fields and notes have not changed for `FOCUS_QUIET_DAYS` (14) says "No change for N days. Still the one?" (`quiet_days` = days since the later of the task's `updated_at` and its newest note).
- **Screens**: a *Focus* card on This week between *Needs attention* and *Priorities this week* (rows: star, tick, title, project, P1/P2, status, "due in 5 weeks", "in focus through …", quiet nudge, the usual reschedule buttons, and the task's latest notes like the other rows); an **In focus** count tile; a `FocusStar` (hover to show, lit when on) on board cards, list rows and the This week rows; `f` toggles on the highlighted row of Tasks (board and list) and This week; a *Focus* box in the task's pane (Not in focus / Every day, until I take it out / Today only / Every day, until a date…) with a sentence saying what will happen; a *★ In focus* pill among the pane's pills.
- **Repeating tasks**: the next task keeps a focus with no end date; a focus with a last day stays with the finished one.
- **Demo data**: "Go-live checklist" is pinned and "Gateway: migrate the first five services" is in focus for two weeks; both are weeks from their due date.

**Out**
- A weekday cadence ("Mondays and Thursdays"), a quick-add token, an automatic "pick today's focus" morning prompt, a Weekly review step for stale focus, a Markdown export line (the JSON export carries `focus` with the task), clearing focus when a task is finished (it just stops showing; reopening brings it back).

## Acceptance criteria
- [x] Focus is stored, changed, cleared, logged and undone/redone exactly (`store` tests `task_focus`, `undo`); an unreadable value does not stop the task loading.
- [x] `set_task_focus` maps each choice and refuses a past day without changing anything (`commands::tasks` test).
- [x] This week: any due date shows; red flags win and nothing repeats; an ended or finished focus is not shown; quiet days count notes; a past day is judged against that day; focus rows get notes (`core::this_week` tests, and the demo test through real data).
- [x] A repeating task's next one keeps a pin, not a dated focus (`store` test).
- [x] Pure text of the screens (`ui::components::focus`, `ui::pages::this_week` tests); Help describes it (`this-week`, `tasks`, `keyboard`, `recurring`).
- [ ] Click-through (see below).

## Not yet verified by hand
- Hover a card on the Tasks board: a star shows next to ⋯; click it: it lights, and This week has a *Focus* card with the task (even with a due date months away or none)
- On This week press `j` to a row, then `f`: it leaves or joins Focus; `Ctrl/Cmd+Z` puts it back
- In a task's pane choose *Every day, until a date…*, type a date: the sentence under the box says through when; a past date shows an error toast
- Choose *Today only*, then check tomorrow (or set the clock): it is gone
- Put 6 tasks in focus: the amber line appears; a task untouched for 14 days says *Still the one?*
- A focused task that is overdue stays under *Needs attention* with a lit star and is not repeated under Focus
- Click a past day in the strip: the Focus card follows that day
