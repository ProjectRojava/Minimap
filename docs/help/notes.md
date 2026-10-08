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

A note that has text **opens as it will read**: headings, lists, bold, links and mentions formatted, with a **pencil** at the top right of the text. Click the pencil (or double-click the text) to edit; press **Done** to go back to reading. A new, empty note opens ready to type in. While you edit, the note **saves by itself** about a second after you stop typing and when you leave the box (and when you press *Done*); there is no Save button.

The notes you write on a task, project or objective (*Notes and findings*) are ordinary notes, so they work the same way; see [Tasks](help:tasks).

## The text box

Every place you write longer text uses the same box, modelled on a GitHub comment: the **description** of a task, project, objective or team, the **context, decision and rationale** of a decision, the **notes** on a person, a note's text, and the box for a new note on an item. Descriptions and decision text read as formatted text with a pencil; press the pencil, edit, then **Save** (or `Ctrl/Cmd+Enter`) or **Cancel**. `Esc` also cancels, but only when you haven't typed anything yet, so a slip never throws text away.

Above the text are two tabs, **Write** and **Preview** (what it will look like), and a toolbar:

| Button | What it does |
|---|---|
| **H** | Makes the line a heading; press again to undo |
| **B**, *I* | Bold and italic around the selection (`Ctrl/Cmd+B`, `Ctrl/Cmd+I`); press again to take it off |
| ❝ | Quotes the selected lines |
| `<>` | Code: inline for a word, a block for several lines |
| Chain | A link, with the address ready to type over (links open in your browser or mail program when you click them in the formatted text) |
| •, 1., ☑ | Bulleted, numbered and task lists for the selected lines. Pressing `Enter` in a list starts the next item; `Enter` on an empty item ends the list |
| **@** | Mention a person, project or task |
| Paperclip | Attach a file and put a link to it in the text |

Everything the toolbar does can also be typed as plain Markdown, and `Ctrl/Cmd+Z` inside the box steps back through toolbar changes like any typing. A single line break shows as a line break, and a blank line starts a new paragraph. Raw HTML is shown as text, and addresses are limited to web and email links.

Mentions in a description or decision are shown with the item's current name and open it when clicked, but they do not create a link on the graph: only notes do (below).

## Mentions with `@`

Type `@` and a picker lists **people, projects and tasks** (type to filter; `↑`/`↓` to choose; `Enter` or `Tab` to insert; `Esc` to cancel). A mention is stored as `@[Name](node:...)`, shown by its **current name** when the note is read, and it makes a *mentions* link from the note to that item. Click a mention in the formatted note to open the item. Removing the mention from the text removes the link.

## Checklists become tasks

Any line like `- [ ] Send the plan to @raj` is an open item. The **Checklist** section lists them with **Convert to task**:

- the new task's assignee is the first **person** mentioned on that line (otherwise you), and its project is the first **project** mentioned;
- the line becomes `- [x] @[the task](...)` so it is ticked and linked to the new task;
- if you changed the line in the meantime, Minimap refuses rather than converting something different.

Open `[ ]` items are also **carried over** to the next note of a repeating series; see [Recurring items](help:recurring).

## Attachments

Paste or drop a file onto the editor (or use *Attach file…*) and a link is inserted where the cursor is. Pictures show when the note is read. See [Google Drive](help:google-drive) for the file types, size limit and where files are kept.

## Privacy and undo

Note text is never written to the activity history (only its size) and is never logged. Because the text isn't in the history, note edits are not part of [Undo](help:undo): the editor has its own undo for typing (`Ctrl/Cmd+Z` inside the text box). Creating and archiving a note can be undone.
