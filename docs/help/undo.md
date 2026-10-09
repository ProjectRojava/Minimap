# Undo and redo

Made a mistake? `Ctrl/Cmd+Z` takes back your last change and shows what it did ("Undone: archived task Fix login"). `Ctrl/Cmd+Shift+Z` (or `Ctrl+Y`) puts it back. Both are also in the command palette.

## What counts as one step

**One action is one step**, however many things it changed: a quick-add, pasting a list of tasks, archiving a project with its tasks, applying a [what-if](help:what-if), finishing a repeating task (which also made the next one). Undo takes back the whole step. Minimap keeps the **last 20 steps** of the current session.

## What can be undone

Creating, editing, archiving and restoring items, adding and removing links, changing a manager, finishing a task, superseding a decision, making a task repeat. Undoing a creation **archives** the item (it isn't deleted, so redo can bring it back and nothing is lost).

## What can't

- **Deleting for good** (only possible for archived items). The step is kept so you are told: *Can't undo deleted task: deleted items can't be brought back.* The next `Ctrl/Cmd+Z` goes on to the step before it.
- **Attached files** (adding or removing) and the **details of a link** (its weight, lag, role).
- **Note text.** What you type in a note has its own undo inside the editor. Creating and archiving a note can be undone.
- **Settings.**
- **A meeting starting and ending on its own.** The clock moves meetings to *in progress* and *done*; that is not a step. Scheduling a follow-up or moving a meeting yourself is.

## When things have changed

Undo never overwrites a newer change. Before putting a value back it checks the value is still what your step left. If something else changed it since (a later edit, or a change that came in from your other computer through [Google Drive](help:google-drive)), that step is refused with the reason, for example *"Fix login"'s due date was changed since*, **nothing is changed**, and the next `Ctrl/Cmd+Z` goes on to the step before it. Putting a removed link back checks the rules again, so it won't create a loop.

## Good to know

- Undo is for this session: after you close the app there is nothing to undo. The permanent history of every change is each item's **Activity** list.
- A new change clears what you could have redone.
- The history is cleared when your data is replaced wholesale: restoring a backup, recovering a Drive checkpoint, or connecting to a Drive that already has data.
- To get back something from earlier, use [Backup and restore](help:backup-restore).
