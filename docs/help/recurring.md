# Recurring items

Regular work, such as a weekly 1:1 or a monthly board update, can repeat by itself.

## Making something repeat

Give a task or a note a **rule**:

| Type | Meaning |
|---|---|
| `day` | Every day |
| `mon` ... `sun` | Every week on that day |
| `week` | Every week, on the weekday of the due date (or of today) |
| `2w`, `3w` ... | Every N weeks (1 to 52), on the weekday of the due date |
| `2w:fri` | Every 2 weeks on Friday |
| `month` | Every month, on the day of the month of the due date |
| `month:15` | Every month on the 15th (a shorter month uses its last day) |

Set it in the **Repeats** box in the task or note's detail pane (empty or `none` stops it), or in [quick-add](help:quick-add) with `every:`:

```text
task Board update every:mon
task Pay invoices due:2027-03-31 every:month
note 1:1 @priya every:wed
```

Without a due date (for a note: a date) the first one starts on the rule's first date: `every:mon` on a Wednesday is due the coming Monday. A ↻ marks repeating tasks in the list.

## Tasks repeat when you finish them

When you mark a repeating task **done**, from anywhere, Minimap creates the next one straight away:

- same title, description, project, estimate, priority, assignee and objectives;
- status *to do*, due on the rule's **next date after the one it was due**, and never in the past (finish a weekly Monday task on Wednesday and the next is due next Monday; finish it weeks late and it is due the first Monday that is not past);
- the same gap between start and due dates, if it had a start date;
- **not** the links that block it: those belong to the one that was blocked.

The rule moves to the new task, so the finished one stays as history. It is one step for [Undo](help:undo): `Ctrl/Cmd+Z` takes the new task away and makes the finished one open and repeating again.

Cancelling a repeating task ends the series. If the project was archived since, the next task goes to the inbox.

## Notes repeat on their date

A note can't be "finished", so a repeating note makes its next one **on the date**. When the rule's next date arrives (Minimap checks at start and every ten minutes, so it is there on the day even if the app was closed), a new note is created for it:

- same title and kind, and the date of the occurrence;
- starting with the **template**, and then the previous note's **open checklist items** under *Carried over*;
- the rule moves to the new note.

After a long absence you get **one** note for the latest date, not a pile for the weeks you missed.

### The template

A repeating note has a **Template for the next notes** box. When you switch repeating on it starts as the note's own text, so a 1:1 that mentions Priya keeps mentioning her in every new one (which is what makes it show as her 1:1). Edit it to what each new note should start with, for example an agenda. Mentions in the template are real links.

## Good to know

- Repeats are never created ahead of time, so your lists don't fill with future copies.
- Changing a rule affects the *next* one; nothing existing is rewritten.
- Skipping one occurrence, "end after N times" and rules like "last Friday of the month" are not available.
