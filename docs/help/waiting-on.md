# Waiting on

Leading means depending on others. **Waiting on** keeps track of what you have asked for and not yet received, how long ago you asked, and which ones need a nudge.

## The screen

Open items are listed **oldest first** with their age, so the ones that most need chasing are on top. Each row has **Resolve** (done: sets today as the resolved date) and **Snooze**. Filters: a person, and whether to show snoozed and resolved ones. Resolved and snoozed items carry a chip.

*New* adds one:

| Field | Notes |
|---|---|
| **Who** | The person you are waiting on (required) |
| **What** | A description |
| Asked on | Defaults to today |
| Expected by | Optional date |
| **About** | Optional: the task or project it concerns |

From the keyboard: `Ctrl/Cmd+K` then `wait @raj on "Security review sign-off" by:next-wed`. See [Quick-add and the palette](help:quick-add).

## When is something stale?

An open waiting-on is **stale** when it is older than your threshold (7 days by default; change it under *Settings → Thresholds*) **or** its expected date has passed. Stale ones are emphasised here, listed on [This week](help:this-week) and the [Overview](help:overview), and counted in the weekly review.

## Snoozing

*Snooze* hides an item until a date you choose ("check again Thursday"). It is out of the lists and not stale until that date, when it comes back on its own. Open it to clear the snooze early.

## Resolve and reopen

*Resolve* closes it; resolving twice keeps the first date. *Reopen* puts it back. The weekly review lists what was resolved in the week.
