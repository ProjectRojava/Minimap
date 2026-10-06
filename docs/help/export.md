# Export your data

You are never locked in. **Export your data**, under *Settings → Data & backup*, writes everything to a folder in open formats.

## Doing it

1. Tick **Also write Markdown** if you want readable files as well as the data.
2. Press **Export…** and choose where to put it.
3. Minimap creates a **new folder** named like `minimap-export-20270303-141500` inside the one you chose, and tells you what it wrote. Nothing in your folder is changed or overwritten; a second export makes a second folder.

## What is in it

**The data (always), as JSON:**

| File | Contents |
|---|---|
| `objectives.json`, `projects.json`, `tasks.json`, `people.json`, `teams.json`, `notes.json`, `decisions.json`, `waiting_on.json` | One list per kind of item |
| `edges.json` | Every link between items, with the ids it joins |
| `attachments.json` | The attached files (which item, name, size, checksum) |
| `activity.json` | The full history of changes, oldest first |
| `manifest.json` | What this export is: format and version, app and database version, when it was made, how many records each file holds |
| `README.md` | A description of the folder |
| `attachments/<id>/<name>` | The attached files themselves |

**Archived items and removed links are included**, marked by `archived_at`, so this is everything and not only what you see. Ids are UUIDs, dates are `YYYY-MM-DD`, timestamps are RFC 3339 in UTC. The files read back into exactly the structures the app uses.

**With Markdown ticked:**

- `projects/<name>.md`: one file per project with its details, owner, dates, objectives, dependencies, its tasks as a checklist (assignee, due date, estimate, priority, what blocks it, whether it repeats), the decisions that affect it and what you are waiting on;
- `inbox.md`: the tasks that belong to no project;
- `notes/<date>-<title>.md`: each note on its own, with mentions as `@Name` and links to pictures and files pointing into `attachments/`.

## Things to know

- **It is plain text.** The export is **not encrypted**, even if your database is. Keep it somewhere you trust.
- **Attached files** that are only on Google Drive and not on this computer can't be included (an export doesn't download from Drive); the result says how many, and they are still listed in `attachments.json`.
- **Importing** an export back into Minimap isn't built; the files are ordinary JSON and Markdown that other tools can read.
- For a copy you can **restore** in Minimap, use [Backup and restore](help:backup-restore).

See also [Settings](help:settings).
