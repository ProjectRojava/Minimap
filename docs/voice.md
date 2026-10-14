# Voice: how Minimap talks

This guide is for whoever writes a string a person will read: a label, a toast, an error, an empty state, a Help page, the CHANGELOG. The product is called Minimap because it is meant to be **glanced at, not lived in** (`docs/product-design/README.md`); the words should work the same way.

## Who we are talking to
A busy senior person between meetings. They glance. Every string should be readable in one look, and tell them what is true and what, if anything, to do.

## Rules
1. **Say what is true, then what to do.** "3 tasks overdue. Reschedule or finish them." Not "Uh oh!".
2. **Plain words, short sentences, active voice.** "Minimap saved a backup", not "A backup has been created".
3. **Minimap words only where the feature has that name.** The spatial words are *fog*, *clear*, *safe zone*, *approaching*, *route*, *recalculated*, *ping*, *save point*, *zoom*, *radar*, *Portfolio view* and *My view*. Use each for its own feature and nowhere else. Do not sprinkle metaphors ("Your quest log is empty" is banned; so is "scout", "mission", "level up", "enemy", "player").
4. **Never judge people.** Say *load*, not performance. "Over capacity", never "overworked", "slacking" or "underperforming". Never "health" or "score" for a person (a project has health; a person has a load).
5. **Stay calm.** No exclamation marks, except for a real success the person waited for (there is none today). No "Oops", "Whoops", "Uh oh", "Awesome", "Great job".
6. **Numbers are digits; dates are `YYYY-MM-DD`.** Durations say *working days* when they are working days.
7. **An empty state is a title and one sentence**: what the screen would show, and how to get it there.

## Word list
| Say | Not |
|---|---|
| archive (when it can be undone) | delete |
| link | edge (in the UI; "edge" is a developer word) |
| task | ticket, item (when it is a task) |
| person | user, resource |
| load, over capacity | utilisation, overworked |
| slip (verb) | delay |
| target date (a project or objective) | deadline |
| due date (a task) | deadline |
| working days | business days |
| save point | snapshot, restore point |
| Portfolio view, My view | global view, personal view |
| waiting-on | pending, blocked-on |
| still accurate (the confirm button) | verified, approved |

## Examples
The first seven are strings that were in the app and are now different; the last three are slips to avoid.

| Before | After | Rule |
|---|---|---|
| Overview empty state: **Nothing to show yet** | **No health to show yet**: "Add objectives and projects, link each project to an objective (Links → Contributes to), and give them target dates and task estimates. Their health appears here." | 7: say what would show |
| Notes: "Press n to write one, or clear the filters." | "Clear the filters to see all notes, or press n to write one." | 7 |
| Decisions: "Press n to record one, or clear the filters." | "Clear the filters to see all decisions, or press n to record one." | 7 |
| Tasks: "Press n to add one, or adjust the filters." | "Clear the filters to see all tasks, or press n to add one." | 7 |
| Projects: "Add one with New project, or clear the filters." | "Clear the filters to see all projects, or add one with New project." | 7 |
| Board: "No tasks match the filters." | "No tasks match the filters. Clear them to see all tasks." | 1, 7 |
| Sort menu: "Sort: deadline, soonest" | "Sort: due date, soonest" | word list |
| "A backup has been created." | "Minimap saved a backup." | 2 |
| "Uh oh! 3 tasks are overdue!" | "3 tasks overdue. Reschedule or finish them." | 1, 5 |
| "Alex is underperforming this week." | "Alex is over capacity this week: 120% of 40 hours." | 4 |

## Where it applies
- UI strings, toasts, errors and empty states: all of it.
- Help pages: the same rules, written for a user (see `CLAUDE.md` §11).
- The CHANGELOG: one line, active voice, user-facing language. Commit messages are not covered.

The design rules for how things *look* are in [`design.md`](design.md); the product's principles are in [`product-design/README.md`](product-design/README.md).
