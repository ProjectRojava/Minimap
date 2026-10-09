# Meetings

**Meetings** are tasks that happen at a time. A meeting is a kind of [task](help:tasks) with a day, a start time and a length that **starts and ends on its own** and can have **follow-ups**. To see only meetings, use the *Type* filter on the [Tasks](help:tasks) screen.

## What a meeting is

A meeting is the built-in task type *Meeting* (the last in the type list; you can rename or recolour it, not remove it), and it differs from other tasks in four ways.

- **It always has a day and a start time.** *New meeting* (a button next to *New task* on the Tasks screen, or in the command palette `Ctrl/Cmd+K`) asks for a title, a date, a start time on the 24-hour clock (`09:30`, `14:00`) and a length (`30m`, `1h`, `1h30m`; it is 1 hour if you leave it). To turn a task you already have into a meeting, pick *Meeting* in its *Type* box: it asks when, and it isn't a meeting until you say. A meeting's pane shows *Date*, *Starts* and *Length* instead of the start date, due date and estimate. You can't clear the date or time of a meeting, and you can't make one with `type:meeting` in [quick-add](help:quick-add) (a line has no room for a time).
- **It moves itself.** At its start time a meeting that is *to do* becomes *in progress*, and when it ends (the start plus its length) it becomes *done*. A meeting that ended while Minimap was closed becomes *done* the next time you open it, finished at the time it ended. This happens while Minimap is open, about every half minute. To end one early, tick it; to cancel one, set it to *cancelled* (a *cancelled* or *blocked* meeting is left alone). **To postpone a meeting, change its time**: a meeting that had started or finished and is moved to a time that hasn't come becomes *to do* again. These automatic moves are not steps for [Undo](help:undo).
- **It isn't work to chase.** A meeting is never late, never "due today" in red on [This week](help:this-week) (it has its own *Meetings* card with the times), and it doesn't warm up on the board as the day nears. It takes no part in the [schedule](help:schedule), the critical path, [what-if](help:what-if), [capacity](help:capacity), health, the dependency graph or the [weekly review](help:weekly-review). It has no focus star.
- **It can have follow-ups.** See below.

On the board and the list a meeting shows its time (`10:30–11:30`); meetings on the same day are in time order.

## Follow-ups

A meeting can be **followed up** by other meetings, as many as you like, and each meeting follows up on **one** meeting at most. In the meeting's pane the *Follow-ups* section lists the meeting this one follows up on and the meetings that follow up on it, each with its day and time (click one to open it, ✕ removes the link).

- **Schedule follow-up…** makes a new meeting linked to this one: titled *Follow-up: …*, a week later at the same time (change the date, time or length in the dialog), with the same project, priority, assignee, objectives and reference links. It is one step for [Undo](help:undo). The button is also on the meeting's row on This week (*Follow-up…*).
- **Follows up on…** and **Add a follow-up…** link two meetings you already have. Linking a meeting that already follows another, or one that would make a loop, is refused with a message naming the meetings; so is linking something that isn't a meeting.
- **A repeating meeting links itself.** When a repeating meeting ends, the next one is made at the same time and follows up on the one that just ended, so a weekly meeting is a chain you can walk back through (see [Recurring items](help:recurring)).

A follow-up link moves no date and holds nothing up.

