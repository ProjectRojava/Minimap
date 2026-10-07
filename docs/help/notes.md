# Notes and @mentions

**Notes** are for meetings, 1:1s and anything you want to write down. They are Markdown, they can **mention** people, projects and tasks, and an unchecked checkbox in a note can become a real task.

## Creating and finding notes

*New note* creates an "Untitled note" and opens it. The list is newest first (by the note's date). Filters: text, kind, a mentioned item, and a date range.

Every note has a **title**, a **date**, and a **kind**:

| Kind | Use |
|---|---|
| **1:1** | A one-to-one. A 1:1 that mentions a person shows on [This week](help:this-week) for the week it is dated |
| **Meeting** | A meeting |
| **General** | Anything else |

Tasks, projects and objectives have a **Notes and findings** box in their detail pane: what you type there becomes a note that mentions the item.

From the keyboard: `Ctrl/Cmd+K` then `note 1:1 @priya` creates a 1:1 (its title becomes "1:1 with Priya").

## Writing

The editor has **Write** and **Preview** tabs. It **saves by itself** about a second after you stop typing and when you leave the box; there is no Save button. Write ordinary Markdown: headings, lists, bold, links, tables.

Preview is safe: raw HTML is shown as text, and external links don't navigate away (they show their address when you hover).

## Mentions with `@`

Type `@` and a picker lists **people, projects and tasks** (type to filter; `↑`/`↓` to choose; `Enter` or `Tab` to insert; `Esc` to cancel). A mention is stored as `@[Name](node:...)`, shown by its **current name** in the preview, and it makes a *mentions* link from the note to that item. Click a mention in the preview to open the item. Removing the mention from the text removes the link.

## Checklists become tasks

Any line like `- [ ] Send the plan to @raj` is an open item. The **Checklist** section lists them with **Convert to task**:

- the new task's assignee is the first **person** mentioned on that line (otherwise you), and its project is the first **project** mentioned;
- the line becomes `- [x] @[the task](...)` so it is ticked and linked to the new task;
- if you changed the line in the meantime, Minimap refuses rather than converting something different.

Open `[ ]` items are also **carried over** to the next note of a repeating series; see [Recurring items](help:recurring).

## Attachments

Paste or drop a file onto the editor (or use *Attach file…*) and a link is inserted where the cursor is. Pictures show in the preview. See [Google Drive](help:google-drive) for the file types, size limit and where files are kept.

## Privacy and undo

Note text is never written to the activity history (only its size) and is never logged. Because the text isn't in the history, note edits are not part of [Undo](help:undo): the editor has its own undo for typing (`Ctrl/Cmd+Z` inside the text box). Creating and archiving a note can be undone.
