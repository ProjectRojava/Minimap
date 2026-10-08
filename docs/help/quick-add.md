# Quick-add and the command palette

Press `Ctrl/Cmd+K` anywhere (even in a text box) to open the **command palette**. It does three things: jumps to a screen, finds an item, and **adds things from one line of text**.

## What the palette does

- Type words to filter **actions and screens** ("new task", "go to capacity", "undo") and **search results**. `↑`/`↓` choose, `Enter` runs, `Esc` closes.
- Type a line that **starts with a keyword and a space** (`task `, `project `, `wait `, `note `, `decision `) and it becomes a **quick-add**: a preview card shows how your line was understood, and `Enter` adds it.
- Anything else ends with an *Add task…* row, so a bare line becomes a task.
- *Resolve waiting-on…* lists your open waiting-ons so you can close one without leaving the keyboard.

## The shape of a line

```text
task Fix login timeout @priya #api-launch !2 due:fri est:3d blocks:"Release 1.2"
project Q1 EU region owner:@me target:2027-03-31 for:"Launch EU"
wait @raj on "Security review sign-off" by:next-wed
note 1:1 @priya
decision "Postgres over Mongo" affects:#api-launch
task Board update every:mon
task Pick the data store type:decision due:fri
```

- **Keyword first**: `task`, `project`, `wait` (or `waiting`), `note`, `decision`. No keyword means a task.
- The rest is **title words** mixed with **markers**; markers can go anywhere. Quote text to keep spaces: `"Release 1.2"`, `@"Priya Shah"`.
- A token that *starts* inside quotes is plain text (`"#123 is a bug"`), and anything unrecognised is just a word.

## Markers

| Marker | Meaning | For |
|---|---|---|
| `@name` | a person (`@me` is you) | task: assignee (default you); project: owner; wait: who you wait on; note: mentioned |
| `#handle` | a project, by handle or name | task: its project (default inbox); wait: what it is about; note: mentioned; decision: affected |
| `!1`–`!5` | priority, 1 highest | task, project |
| `due:` / `by:` | due date | task; `by:` is also a wait's expected date |
| `start:` | start date | task, project |
| `target:` | target date (`due:`/`by:` also work) | project |
| `est:` | estimate: `3d`, `1.5d`, `4h` | task |
| `blocks:` | a task this one blocks (repeatable) | task |
| `for:` | an objective it contributes to (repeatable) | task, project |
| `owner:` | the owner (same as `@name`) | project |
| `about:` | the task or project a wait is about | wait |
| `affects:` | a project, task or objective (repeatable) | decision |
| `date:` | the note's or the decision's date | note, decision |
| `kind:` | `1:1`, `meeting`, `general` | note |
| `status:` | `proposed` or `decided` | decision |
| `every:` | repeats: `day`, `mon`, `2w`, `month` (see [Recurring items](help:recurring)) | task, note |
| `type:` | the task's type, by name or id: `type:decision`, `type:"Legal review"` (see [Tasks](help:tasks)) | task |
| `1:1` | a bare word: makes the note a 1:1 | note |

A marker a kind doesn't use is reported ("`est:` isn't used for a project"), never silently dropped.

## Dates

| You type | You get |
|---|---|
| `today`, `tomorrow` | that day |
| `fri` (any weekday) | that day this week, **today if it is that day** |
| `next-wed` | that day in **next** Monday–Sunday week |
| `next-week` | Monday of next week |
| `+3d`, `+2w` | that many days or weeks from today |
| `2027-03-31` | exactly that date |

## Names

`@name` finds people by **exact name, whole word, then prefix**, so `@pri` finds Priya. A near miss (a typo) is only ever **offered as a guess**, never taken silently. If more than one person fits, or none does, the preview asks.

## The preview and what it asks

The card shows each detail it understood (title, due, estimate, project, assignee...). Anything unclear is a **question** under it: pick one of the candidates, **Create** a new person or project from the words you typed, or **Skip** it. `↑`/`↓` and `Enter` answer. Required references can't be skipped (a waiting-on needs its person), and nothing is added until every question is answered. Adding is **all or nothing**, including anything you chose to create.

If something is wrong (an unreadable date, a bad estimate, an unclosed quote) the card says so in plain words and `Enter` does nothing until it is fixed.

> **Tip:** `note 1:1 @priya` makes a note titled "1:1 with Priya" that mentions her, and it shows on [This week](help:this-week).
