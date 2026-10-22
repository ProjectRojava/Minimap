# Pings: come back to anything on a day

A **ping** is a marker you leave on anything, with a day and, if you like, a line of text: *come back to this on Friday*. The vendor contract inside a project, a person you need to talk to about something, a decision you want to revisit. On that day it shows on [This week](help:this-week). You clear it with one key.

**Ping or focus?** [Focus](help:tasks) is for a *task* you want in front of you **every day** until you take it out. A ping is for **anything** you want to see again **on a day**. If you only want something in view today, a ping dated today does it for any item (a person, a project, a decision), but it never expires on its own: clear it when you are done. A task can be both; its ping is then shown on its row under *Focus* or *Needs attention* rather than twice.

A ping is not a task. It has no estimate, no owner and no due date, so it never enters the [schedule](help:schedule), [capacity](help:capacity), [health](help:overview) or a [what-if](help:what-if), and it changes nothing about the item it sits on. It is private bookkeeping for you: it is not in the weekly status report either.

## What you can ping

Any item: an objective, a project, a task, a person, a team, a note, a decision or a waiting-on. An item has **one live ping at a time**: pinging something that already has one changes that ping (its day and note) instead of adding a second. That keeps the pins readable.

## Leave a ping

- **On the keyboard**: highlight a row or card on any list, the board or This week and press `m`.
- **From the detail pane**: press **Ping…** at the top of the pane.
- **From the palette**: `Ctrl/Cmd+K`, then *Ping this…* pings the item open in the pane.

A small box asks *When* and for an optional *Note*:

| Button | Day |
|---|---|
| Today | today |
| Tomorrow | tomorrow |
| Friday | this Friday (not offered on Friday, Saturday or Sunday; *Next Mon* takes its place) |
| Next week | the Monday of next week |
| Pick a date… | any day from today on, typed as `2027-03-31` or chosen from the calendar |

Type the note (up to 200 characters, plain text) and press `Enter`. Pick today or later: a day that has already passed is refused (*Pick today or later*). If the item already has a ping the box opens with its day and note filled in, and **Clear ping** takes it away.

## From the palette: quick-add

A line that starts with `ping` and a space leaves a ping, like any [quick-add](help:quick-add) line. It takes the item to ping, the words of the note, and an optional `by:` day (today when you leave it out):

```text
ping #eu-region check data residency by:fri
ping @raj ask about the SOC 2 auditor by:tomorrow
ping task:"Release 1.2" chase the owner by:+2d
ping objective:"Launch EU" review the plan by:next-week
ping decision:"Postgres over Mongo" revisit once pricing is approved
```

Name the item with `#project`, `@person`, or `task:"…"`, `objective:"…"` or `decision:"…"`. Say exactly one. You can't ping yourself (`@me`): make a task instead. The preview shows the item, the day and the note before anything is added.

## Where pings show

- **A pin** next to the title of the item, wherever it appears: lists, board cards, the Overview and This week. A solid blue pin means its day has come; a grey pin means it is still to come. Point at it to read *Pinged for Fri 2027-03-05: check the vendor contract*.
- **The Pings card on This week**, after *Focus*: every ping whose day has come, oldest first (a ping on a task that is already a row under *Needs attention* or *Focus* is shown on that row instead, with its note and a ✕ to clear it). Each row has the pin, what kind of item it is, its name, your note, and when you made it (*pinged Mon*, or *pinged Mon · due 3 days ago* once its day has passed). Click the row to open the item. Under it, **Coming up (N)** lists the pings still to come, closed until you open it. The card is hidden when there are none, and when you look back at a past day.
- **The detail pane**: a strip under the item's name says *Ping due* or *Ping*, with its day and note, and has **Clear** and **Change…**.
- The palette's *Show pings* takes you to the card.
- **The status bar**: while at least one ping's day has come, a blue pin with the number due shows along the bottom. Click it to open This week. It is gone when none is due.

## Finishing a task clears its ping

Moving a task to **Done** clears its ping, so a finished task never comes back to you on This week. The ping stays in the history and the export like any cleared ping, and `Ctrl/Cmd+Z` on the finish brings the task and its ping back together. Other statuses (in progress, blocked, cancelled) leave the ping alone.

## Clear a ping

On the Pings card, press `x` on the highlighted row, or use **Clear**. **Tomorrow** and **Next week** put it off instead. The strip in the pane and the box have **Clear** too. Clearing again keeps the first day it was cleared. Pinging a cleared item makes a new ping.

## On the radar

A ping that has come due shows on the [radar](help:radar): a pin on its own when the item is not otherwise on it, or a small dot on the corner of the item's blip when it is. The radar's bottom line counts the pings due today.

## Good to know

- **Archived items.** A ping on an archived item is hidden, not deleted. Restore the item and the ping is back. You can't ping an archived item.
- **Undo.** Making, changing and clearing a ping can all be undone with `Ctrl/Cmd+Z`, and redone: see [Undo and redo](help:undo).
- **Google Drive.** Pings sync like everything else ([Google Drive](help:google-drive)). If you ping the same item on two computers while offline, one ping is kept (the newer) and the other is cleared; the sync note says so.
- **Export.** Pings are in `pings.json` ([Export your data](help:export)), cleared ones included.
- **Demo data** has three pings: one due today, one overdue and one coming up.
