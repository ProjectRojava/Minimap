# Archive

The **Archive** (`g v`, under *Log*) holds everything that has been archived, whatever it is: tasks, projects, objectives, people, teams, notes, decisions and waiting-ons. Nothing is deleted by archiving; it is only put away, and it comes back with one click.

## The screen

Newest first, with the kind, the name, what a task was left as (its state and project), and the day it was archived. *Search* looks in the name and in that detail, and the kind box narrows the list to one kind of item. Click a name to open the item in the pane and read it. **Restore** puts it back, with the links that were archived along with it (a task's assignee, what it blocks).

## Finished tasks archive themselves

A task that has been done for a while is archived by Minimap, so Done doesn't fill up for ever. By default that is **30 days after it was finished**, and **2 days for a meeting** (they pile up fastest). Change or switch it off under *Settings → Thresholds → Finished tasks* (0 means never). It runs when Minimap starts and then every hour, and it finishes within a day of a task coming of age even if the app was closed.

Some finished tasks are kept because archiving them would change something that is still live:

- a sub-task whose parent is still open, and a parent that still has open sub-tasks (the "2 of 5 done" would shrink);
- a task that blocks something still open (it would drop out of that task's *Blocked by* list);
- a task with a ping that has not been cleared.

When what held a task back is finished too, it goes on the next sweep.

## On the board

Before they are archived, finished tasks fade on the Tasks board, counted from the day they were finished: full cards for **3 days**, then small and dimmed (title and project only), then folded into one **Earlier** group after **7 days**, and out of sight after **14 days** behind *Show older*. Hover a dimmed card to see it clearly; drag one to another column to reopen it.

## Reports still count them

The [weekly review](help:weekly-review) counts what you finished in a week even if the task has since been archived, so a quick archive never takes work out of a report.

## Good to know

- Restoring a task does not restore its project, if that was archived too. Restore the project as well.
- Archiving a project can archive its tasks with it; restoring them is done here, one by one.
- Archived items are kept in your exports and backups like everything else. See [Export your data](help:export).
