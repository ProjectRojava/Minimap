# 56 — Waiting on, from the task

Status: Implemented — awaiting manual check · Milestone: — · Priority: Should
Depends on: 06, 08, 24, 25, 41, 42

## Goal
A task that is stuck on someone shows it. From the task's pane you can say "I'm waiting on Raj for this, by Friday", see who and how long on the board and This week, and clear it when it arrives. The waiting-on stays what it is today (a record in *Waiting On* with a person, an age and a stale flag); the task now knows about it.

## How this differs from decisions (41, 42)
A Decision task and its record are 1:1 and the record is the outcome of the work. A waiting-on is not a kind of task: a task can be waiting on several people at once, and many waiting-ons have no task. So there is **no task type, no mark on the link and no gate**. We reuse the ideas (a pane section, a prompt when finishing, undo as one step, the record outliving the task), not the structure.

## Scope
**In**
- **Which waiting-ons belong to a task.** Every active waiting-on whose `about` link points at the task, however it was made (this pane, the Waiting On screen, the Links editor). No new edge attribute, no `edge_rules` change, no migration. A waiting-on has one `about` target, so it belongs to at most one task.
- **Waiting on section in the task pane** (`components/task_waiting.rs`, next to *Decision record*; like the pane's other optional sections it is hidden while the task has none and the Add row's **Waiting on** button reveals it with the form open — decided while building, instead of an always-visible empty section). Lists open waiting-ons (person, what, asked N days ago, expected by, a *stale* chip, a *snoozed until* chip), each with Resolve, Snooze and open-in-pane; finished ones sit under *Resolved (n)*. **Add waiting-on** takes the person (required, not an archived person), what (defaults to "Waiting for <task title>"), expected by (optional); asked on is today. It makes the waiting-on and its `about` link in one transaction (`store::waiting_on::create_for_task`), one undo step.
- **Status is suggested, never forced.** With an open waiting-on and a task that is `todo` or `in_progress`, the section offers *Mark blocked*. When the last open waiting-on of a `blocked` task is resolved, it offers *Move to In progress*. Both are plain updates of the task (own undo step); nothing changes the status on its own.
- **The task knows (badge).** The task row read model gains a `waiting` summary: open count, first person's name, oldest age, and whether any is stale (`core::waiting_on` decides stale; a snoozed one counts as open but is not stale). Board cards, the Tasks list rows and This week rows show an clock badge: "Raj", "Raj +1", amber when stale; the tooltip lists who and how long. A resolved or archived waiting-on, or a waiting-on whose person is archived, still shows if open (the person is still owed to you), with the name as is.
- **This week.** A task row in *Blocked* names who it waits on ("waiting on Raj, 5d"). The *Waiting on* card is unchanged except that a row about a task says "for <task>" (already shown).
- **Finishing a task asks about its waiting-ons.** Moving a task to Done is never held up. Afterwards, if it has open waiting-ons, a small dialog (`components/finish_waiting.rs`, opened from `state::finish_task` at the same sites as the decision dialog) asks *Resolve them too?* with the list, *Resolve* / *Leave open*, Esc = leave open. *Resolve* (`resolve_task_waiting_ons`) resolves all of them today in one transaction, one undo step. Cancelling a task does not ask.
- **Archiving a task** archives its `about` links (existing rule), so its waiting-ons then stand alone, still open and on *Waiting On*. The archive dialog (spec 42) says "2 open waiting-ons stay in Waiting On".
- **Undo/redo** follow the activity log: undoing *Add waiting-on* archives the waiting-on and its link; redo brings the same ones back.
- **Markdown export.** The project file's task checklist adds "waiting on Raj" to open tasks that have an open waiting-on. The full export needs nothing (nodes and links are already in it).
- **Demo data.** The seed already had waiting-ons about two to-do tasks (a stale one on "Security review sign-off", a fresh one on "EU pricing") and one resolved about a project; it gains **"Support contact list for the runbook"** (Maya, fresh) about the blocked "EU support runbook" task. It is fresh on purpose: the demo's story is *one* stale waiting-on. Counts: 4 waiting-ons, 120 links (tests and the Developer card text updated).
- **Help.** Task page: the Waiting on section, the badge and what finishing asks. Waiting On page: where the task link shows. Both say plainly that **an expected-by date does not move the schedule** (it is a reminder, like a ping).

**Out**
- Letting `expected_by` hold up the task in the schedule, impact or health (a possible follow-up; it changes what the schedule is derived from and would need its own ADR).
- A task type for waiting, a gate on finishing, auto-changing the status, auto-resolving without asking.
- Quick-add (`task Fix login waiting:@raj`): `by:` already means the due date on a task, so the key needs its own design.
- Converting a task into a waiting-on or back.
- A warning when the person is the task's assignee (allowed; the assignee does the work, the waiting-on says you are waiting for them).
- Waiting-ons on projects get no pane section (they keep their Links entry).

## Acceptance criteria
- [x] `create_for_task` makes the waiting-on and the `about` link in one transaction; an archived person or an archived/missing task is refused; blank "what" is refused (`store/tests/task_waiting.rs`).
- [x] A task's waiting-ons are exactly the active ones about it; a waiting-on about a project, or whose link was removed, is not (`task_waiting.rs`).
- [x] The row summary: open count, oldest age, stale only for non-snoozed; resolved ones are left out; a proptest that `stale` implies `count > 0` (`core`).
- [x] *Resolve them* resolves all open ones today, keeps first resolved dates, and is one undo step; undo restores them (`store/tests/undo.rs`).
- [x] Add, undo, redo leave exactly one waiting-on and one link.
- [x] Archiving the task leaves its waiting-ons open and unlinked; the archive dialog's text counts them (UI test).
- [x] The two status offers appear only in the cases above, and neither changes the task without a click (UI tests).
- [x] The badge shows on board, list and This week rows with the right text and tone (UI tests).
- [x] Markdown export line; demo data and its snapshots reviewed (story unchanged: EU Region amber, one overloaded person, one stale waiting-on).
- [x] Help pages updated; changelog line; knowledge graph updated (§14); ADR check done (expected: no ADR change, a waiting-on still never changes the schedule).
- [ ] Click-through (see below).

## Decisions (the open questions, answered by building as proposed)
- The badge counts snoozed waiting-ons as open but never stale (the task is still waiting; you only asked not to be reminded).
- The button reads **Mark as blocked** (docs/voice.md: plain words).
- "Just finished" (for the *Resolve them too?* box) is decided in the UI: the change returned a task that is done with a completion time within 15 seconds (`state::just_finished`), so editing a task finished long ago never asks again.
- The dialog and the status offers are suggestions only: no backend gate, no automatic status.
- Capabilities: the three commands are in `capabilities/default.json` (the fourth place of §7's rule) and `build.rs`.

## Not yet verified by hand
- Open a task, *Add waiting-on*: Raj, expected by Friday. The section lists it, the card on the board shows the hourglass and "Raj", the Waiting On screen has it with "for <task>"
- *Mark as blocked* moves the card to Blocked; resolving the waiting-on offers *Move to In progress*
- Add a second one: the badge reads "Raj +1"; one stale turns it amber
- Move the task to Done with one open: the dialog lists it; *Resolve* closes it and Ctrl/Cmd+Z puts both back; *Leave open* keeps it on Waiting On
- Archive the task: the waiting-on is still on Waiting On, open, with no task
