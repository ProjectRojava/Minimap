# Settings

**Settings** (`g s`) is split into tabs by kind of setting. The tab you are on is kept in the address, so a link can open a specific tab. Changes take effect straight away; there is no Save button except for the report template.

## General

**Time**

| Setting | What it does |
|---|---|
| **Hours per working day** (default 8) | Turns estimates like `4h` into days (4h = 0.5 days at 8), and weekly hours into days of capacity. Estimates you already entered keep their value; only new ones use the new setting |
| **Working days** (default Monday–Friday) | Which weekdays count as working days for the [schedule](help:schedule), [capacity](help:capacity), [what-if](help:what-if) and slip counts in the [weekly review](help:weekly-review). At least one must stay on. A Sunday-to-Thursday or four-day week works. Weeks themselves still run Monday to Sunday |
| **Default weekly capacity** (default 40 hours) | What a new person gets when you don't say. People already added keep their own |

**Appearance**: choose a colour theme. *System* follows your computer's light or dark setting; there are 17 themes, dark by default. The choice applies at once.

## Thresholds

- **Waiting on, stale after (days)** (default 7): when an open [waiting-on](help:waiting-on) counts as stale.
- **Capacity, too many open tasks above** (default 10): flags a person on [Capacity](help:capacity) and the Overview whatever the estimates.
- **Project health**: when a project turns amber or red, for lateness, overdue-or-blocked share and unestimated share. Amber can't be above red. *Reset to defaults* restores them. See [Overview and health](help:overview).

## Reports

The Markdown template behind the [weekly status report](help:weekly-review). *Save template* checks it (known placeholders only, every `{{` closed) and refuses a bad one without changing anything; *Reset to default* restores the built-in template.

## Data & backup

- **Data location**: where the database file is, its size, its folder, the log file, and whether it is encrypted, with **Show in folder** to open it. The location is fixed, so backups, the attachment cache and encryption keep working. To use your data on another computer, use a backup or Google Drive.
- **Google Drive**: see [Google Drive](help:google-drive).
- **Backup**: see [Backup and restore](help:backup-restore).
- **Export your data**: see [Export your data](help:export).

## Security

**Encryption**: see [Encryption](help:encryption).

## Developer

Only in development builds: *Add demo data* fills an empty database with a sample company. Not shown in the released app.

## What is shared between computers

If you use [Google Drive](help:google-drive), hours per working day, working days, default capacity, thresholds and the report template are shared by all your computers. The **theme**, the **backup folder** and the **automatic backup** switch belong to each computer.
