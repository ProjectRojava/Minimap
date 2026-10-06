# 27 — Recurring items

Status: Implemented — awaiting manual check · Milestone: — · Priority: Later
Depends on: 06, 08

## Goal
Regular work (weekly 1:1s, monthly board update) appears automatically.

## Scope
**In** (built)
- **A repeat rule on tasks and notes** (`Task.recurrence`, `Note.recurrence`, migration 0009, stored as JSON; none = does not repeat): `Daily`, `Weekly{every N weeks (1-52), weekday}` and `Monthly{day of month}`. A month without that day uses its last day (the 31st gives Feb 28/29). Reads as "every day", "every Monday", "every 2 weeks on Friday", "monthly on the 15th".
- **Tasks make the next one when finished.** Setting a task to done (from any screen: the row's check, the status dropdown, This week, the review, a board drag, the `x` key) creates, in the same transaction and the same undo step, a new task with the same title, description, project, estimate, priority, assignee and objectives, status todo, due on the rule's next date, with the same gap between start and due; the rule moves to the new task (the finished one no longer repeats). The next date is counted from the due date, not from when you got round to it, and is never in the past: finishing a weekly Monday task on Wednesday gives next Monday; one finished weeks late gives the first Monday that is not past.
- **Notes make the next one on its date.** When the rule's next date after a repeating note's has come, a new note is made for it (title and kind kept) that starts with the rule's **template** and the previous note's open `[ ]` items under "Carried over"; the rule moves to the new note. Only the latest due date is made, never one per missed week. This runs at start and every ten minutes (`recurring-notes` thread), so the note is there on its day whether or not the app was open.
- **Quick-add**: `every:day`, `every:mon`, `every:2w`, `every:month` (`2w:fri`, `month:15`) on `task` and `note` lines (grammar in `docs/quick-add-grammar.md`). Without a due date (note: date) the first one starts on the rule's first date, so `task Board update every:mon` is due the coming Monday. The preview shows "Repeats: every Monday".
- **Editing**: a *Repeats* field in the task and note panels (type `day`, `mon`, `2w`, `month`, ...; empty or `none` stops it) with a sentence saying what it will do, and for a note the template; a ↻ chip after the title of a repeating task in the lists. Command `set_recurrence(node, text, template?)`.
- Undo covers setting, clearing and finishing (finishing is one step that takes the next task back too); the full export carries the rule (JSON) and says "repeats ..." in the Markdown; the demo data has a monthly task, a task every 12 weeks and a weekly 1:1 note.

**Out**
- Skipping one occurrence without finishing it, "end after N times / on a date", exceptions, working-days-only (`every weekday`) and "last Friday of the month" rules.

## Acceptance criteria
- [x] Completing a weekly task creates the next week's one with the same fields (store `finishing_a_weekly_task_makes_next_weeks_with_the_same_fields`: assignee, objective with its weight, project, estimate, priority, start/due gap; blockers are not copied).
- [ ] Click-through (see below).

## Decisions
- **Generate on completion or on schedule?** Both, by kind. A task has a natural moment (you finish it) and a "next" that should only exist once this one is done, so tasks generate **on completion**. A note is never "completed" and a 1:1 should simply be there on the day, so notes generate **on their date**. Making tasks ahead of time on a schedule would fill the lists with future copies and duplicate work whenever a rule changes.
- **Recurring 1:1 notes: template body? Yes, a per-rule template.** Copying the previous note would drag last week's discussion along; an empty note loses who it is with (so it would not show as that person's 1:1). The rule carries a **template** that starts as the note's own text when repeating is switched on (or the quick-add line's mentions) and is editable; the previous note's open checklist items are carried over under a heading. A template that mentions a person keeps every new 1:1 linked to them.
- **The rule lives on the newest instance** and moves on when the next one is made, so there is exactly one live item per series and no duplicates when a task is reopened (its rule went with the first completion). Only *done* continues a task's series: cancelling it ends the series.
- **Anchored to the rule, never in the past.** The next due date is the rule's next date after the old due date, but not before today.
- **Stored as JSON in one column** per table: the rule is one value that is read and written whole, and a damaged or newer rule reads as "does not repeat" instead of stopping the item from loading.

## Implementation notes
- Types: `Cadence`, `Recurrence{cadence, template}` (`describe()`, `shorthand()`); `Task.recurrence`, `Note.recurrence`, `Create*.recurrence`, `Update*.recurrence: Patch<Recurrence>`; `QuickMain::Task/Note.recurrence`.
- Core `recurrence` (pure, proptested): `parse_every`, `next_after`, `first_on_or_after`, `next_for_task`, `due_note_date`, `new_note_body`, `validate`.
- Store: `tasks::update_in_tx` finishes a repeating task and calls `make_next`; `notes::generate_due(conn, today)`; `convert::{recurrence_s, col_recurrence}`; `undo::nullable_fields` knows `recurrence`.
- Commands: `set_recurrence` and the `generate_due_notes_tick` (`commands/recurrence.rs`, thread in `main.rs`).
- UI: `components/repeat_field.rs` (`RepeatField`, `help_text`), the ↻ chip in `task_list.rs`.

## Not yet verified by hand
- Quick-add `task Board update every:mon` (Ctrl+K): the preview says "Repeats: every Monday" and a due date; add it; tick it done in the Tasks list: a new one appears, due the next Monday, with the ↻ chip, and the finished one has none
- the same from This week and from the status dropdown; Ctrl+Z takes the new one away and makes the finished one open and repeating again
- open a task, type `2w` in *Repeats*: the sentence changes; clear it: it stops
- make a 1:1 note repeat (`mon`), edit its template; set the computer's date forward (or wait for the day): a new note for that date appears with the template and last note's open items, and the old one stops repeating
- the full export names the rule; a restored older backup (before this version) still opens with nothing repeating
