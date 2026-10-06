# Search

The **search box** at the top of the sidebar finds anything you have written down: the titles and text of tasks, projects, objectives, people, teams, notes, decisions and waiting-ons.

## Using it

- Press `/` (outside a text box) to jump to it, type, and the results appear under the box: the kind of item, its name and a snippet of the matching text.
- `↑`/`↓` move through the results; `Enter` (or a click) opens the item in the detail pane; `Esc` clears the search.
- The **Archived** toggle under the box shows or hides archived items (hidden by default).

The command palette (`Ctrl/Cmd+K`) searches too, next to its actions; see [Quick-add and the palette](help:quick-add).

## How matching works

- Every word you type must match, in any order, and each word matches the **start** of a word: `deploy` finds *Deployment plan* and `pri` finds *Priya*.
- Titles count for more than body text when results are ranked.
- If nothing matches, Minimap tries again **allowing typos** (one slip in words of 4 to 7 letters, two in longer ones; a swap of two letters counts as one), but only for words that match nothing as typed. Very short words are never "corrected".
- In notes, a mention is searched by the **person or item's name**, not by its internal id.

## Tips

- Search for a person's name to find their 1:1s, tasks and waiting-ons in one go.
- Looking for something you archived? Switch *Archived* on.
- For a precise slice, such as "my tasks due this month", use the filters on the [Tasks](help:tasks) screen instead.
