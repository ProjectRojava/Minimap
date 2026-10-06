# Questions and troubleshooting

## Where is my data?

In one database file on this computer. *Settings → Data & backup → Data location* shows where, and **Show in folder** opens it. See [Settings](help:settings).

## The app starts with an unlock screen

Your database is [encrypted](help:encryption) and the key isn't in the keychain. Type your passphrase **or your recovery key**. A wrong key makes you wait a moment before trying again.

## I forgot my passphrase

If you saved the **recovery key**, type that on the unlock screen instead. If you have neither, the data can't be opened: there is no reset. Restore an older [backup](help:backup-restore) that was made *before* you turned encryption on, if you still have one.

## "Can't add this link: this would create a loop"

*Blocks*, *depends on*, *reports to* and *supersedes* links can't go round in a circle (A blocks B blocks A). The message names the path. Remove or reverse one link on it. See [Links between items](help:links).

## A project is red or amber and I don't see why

Open the project (or the Overview row): the **reasons** are listed in words. The usual causes are lateness against the target date (see the critical path in [Schedule](help:schedule)), too many overdue or blocked tasks, or many tasks with no estimate. The [thresholds](help:overview) are in *Settings → Thresholds*.

## The forecast looks wrong

Check, in this order: do tasks have **estimates** (unestimated ones count as a day), are the real dependencies there as **blocks** links (also between projects), does a task have a **start date** in the future, and are the **working days and hours per day** right in Settings? [Schedule and critical path](help:schedule) explains how it is worked out.

## Everyone looks overloaded

Compare each person's **weekly capacity** (People) with your **hours per working day** (Settings): capacity in days is hours ÷ hours per day. Check that estimates aren't too big and that **allocation** percentages on assignments are right. [Capacity](help:capacity) has the formula.

## A task I didn't create appeared

It was probably made by a **repeating** task you finished. See [Recurring items](help:recurring); `Ctrl/Cmd+Z` takes it back, and clearing the *Repeats* box stops it.

## A date landed on a Monday when I said Saturday

The schedule only uses working days, so a weekend start becomes the next working day. Due dates you type stay exactly as typed.

## Quick-add didn't add anything

The preview card says why: an unreadable date, a bad estimate, an unclosed quote, or a question still waiting for your answer (pick, *Create* or *Skip*). See [Quick-add and the palette](help:quick-add).

## Google Drive says it needs attention

- **Sign in again**: your access expired or was revoked. Press *Sign in with Google*.
- **The recovery key doesn't match**: you typed the key of another Drive, or mistyped it. It is 64 letters and digits, usually in groups of four.
- **Newer version**: another computer has a newer Minimap and Drive holds data this version can't read. Update this computer.
- **Offline**: nothing is wrong; it will catch up. A warning appears after a few days.

More in [Google Drive](help:google-drive).

## Two computers edited the same thing

The **newest edit wins**, item by item, and the merge reports anything it had to repair. There are no conflict dialogs. If you need an older state, recover a [checkpoint](help:google-drive).

## I archived something by mistake

Press `Ctrl/Cmd+Z` straight away ([Undo](help:undo)): it comes back with its links. After the app has been closed there is no undo; the item is still in the database (switch *Archived* on in [Search](help:search) to find it) and in a [backup](help:backup-restore) or [export](help:export), but Minimap has no "unarchive" button.

## Can I get deleted items back?

Deleting for good (only possible after archiving, with a confirmation) can't be undone. Restore a [backup](help:backup-restore) from before.

## I can't find the demo data

It exists only in development builds (*Settings → Developer*), on an empty database.

## Glossary

| Term | Meaning |
|---|---|
| **Allocation** | The share of a person's time a task takes (1–100%) |
| **Archive** | Hide an item and its links without deleting it |
| **Blocks** | A link meaning "this must finish before that can start" |
| **Capacity** | A person's weekly hours; load is scheduled work ÷ capacity |
| **Checkpoint** | An hourly or daily copy kept on Google Drive |
| **Critical path** | The chain of tasks that decides a project's finish date |
| **Handle** | A project's short unique name, such as `api-launch` |
| **Item** | Anything you record: objective, project, task, person, team, note, decision, waiting-on |
| **Lag** | Extra waiting working days on a *blocks* link |
| **Recovery key** | A one-time key that opens your encrypted data or your Drive data on a new computer |
| **Slack** | Working days a task can slip without moving the finish |
| **Snooze** | Hide a waiting-on until a date |
| **Stale** | An open waiting-on that is old or past its expected date |
| **Superseded** | A decision replaced by a newer one |
| **Unestimated** | A task with no estimate, scheduled as one day |
| **Working day** | A day counted by the schedule (Monday–Friday by default) |
