# Decisions

A **decision** records what was decided, why, and what it affects, so that in six months you can answer "why did we do it that way?"

## The screen

Decisions are listed newest first, each with its date, status, title and a short excerpt, plus what it **affects** and what **replaced** it. Search by words and filter by status. *New decision* creates "Untitled decision" and opens it. Superseded decisions are shown muted and struck through.

## Archiving a decision

Point at a decision in the list and click **Archive** at the right of its row, or open it and use **Archive decision…** at the bottom of its pane. Either way a dialog asks "Archive this decision?" first (Cancel is selected). An archived decision leaves the list and its links go with it. Press Ctrl/Cmd+Z to bring it back.

## Fields

| Field | Notes |
|---|---|
| Title | The decision in a few words |
| Context | The situation and the options |
| Decision | What was decided |
| Rationale | Why |
| Decided on | A date. Moving a decision to *decided* with no date **stamps today** |
| Status | **proposed**, **decided** or **superseded** |

## Links

- **Affects**: link a decision to the projects, tasks or objectives it concerns. The decision then appears under *Decisions* in each of those panes, which is how you find the reasons behind a project.
- **Supersedes**: when a newer decision replaces an older one, link the new one *supersedes* the old one. The older decision's status becomes **superseded** automatically, and both show the connection. Loops are refused. (Undo takes both the link and the status back.)

See [Links between items](help:links).

## Decisions from tasks

When you give a task the type **Decision**, a decision is created for it in this log, **proposed**, with the task's title (and its description as the context). It is linked to the task and to the task's project, so it shows what it **affects**. Open the task and write what was decided under **Decision record**: the status, the date and the three text boxes are the same ones you see here.

The task is the work of deciding; the record is what lasts. When you finish the task it leaves the board and This week, but **the decision stays here** with everything you wrote.

**Finishing a Decision task.** A Decision task can't be moved to Done until the record has both **Decision** (what was decided) and **Rationale** (why). If either is empty, moving the task to Done (from its pane, by dragging on the board, with `x` on a row, or the tick on This week) opens **Finish decision**: write the two texts and press *Decide and mark done*. That saves them, finishes the task and marks the record **decided** with today's date, as one step (`Ctrl/Cmd+Z` undoes all of it). Cancel leaves the task where it was. Cancelling the task itself needs no decision.

**What Minimap fills in, and what you set.** Under the Status and Decided on boxes a small tag tells you who set each: **AUTO** for what Minimap filled in (the status, when finishing the task decided it; the date, when it was stamped for you, whether by finishing the task or by moving a decision to *decided* without a date) and **YOU** for what you chose or typed. Whatever you set yourself is never overwritten: a date you typed stays when the task is finished, and a decision you already marked decided or replaced is left alone. Change an automatic value yourself and its tag becomes YOU.

- Changing the task's type to something else, or back to Decision, never removes the record and never makes a second one.
- Archiving the task keeps the decision; it just no longer points at the task.
- A task that was typed Decision before this existed shows **Create decision record** in its pane.
- Ctrl/Cmd+Z after setting the type takes the record away again (and Ctrl/Cmd+Shift+Z brings the same one back).

## Decisions in the weekly review

The [Weekly review](help:weekly-review) lists the decisions *made* that week (proposals are left out) and the status report includes them.

## Tip

If you have to decide something **by a certain day**, track the deciding itself as a task of type *Decision* with a due date: the task shows whether you decided on time or how many days late, and keeps the date you first planned. See [Tasks](help:tasks). The task's **Decision record** section (see above) is where you write what was decided and why, and it stays in this log after the task is done.

Record the decision when it is made, even in one line. A proposal can wait as *proposed* until it is decided.
