# 23 — Settings

Status: Implemented — awaiting manual check · Milestone: M4 · Priority: Must
Depends on: 02

## Goal
One place for app configuration.

## Already built (from earlier specs)
- The `settings` key/value table (migration 0005), `get_settings` / `update_settings`, and a Settings screen that grew with each spec: hours per working day (06), waiting-on stale threshold (08), appearance (colour theme: dark default, System, 17 built-in themes; `docs/design.md`), capacity open-task limit (17), project health thresholds (15), status report template (19), backups (20), encryption (21), Google Drive (22).

## Scope
**In** (built)
- **Tabs by kind of setting.** The screen is a tab strip (`ui/src/settings_tab.rs`, panels in `ui/src/pages/settings.rs`), and the selected tab lives in the address (`/settings?tab=data`), so other screens can link to one (the Drive status bar and banner open *Data & backup*, the review's "Edit template" opens *Reports*). All panels stay mounted (hidden when not shown), so a half-typed report template survives a visit to another tab. Arrow keys, Home and End move between tabs.
  | tab | holds |
  |---|---|
  | **General** | Time (hours per working day, **working days**, **default weekly capacity**) and Appearance (theme: System plus 17 themes, 11 dark and 6 light) |
  | **Thresholds** | Waiting on (stale after N days), Capacity (open-task limit), Project health (amber/red thresholds) |
  | **Reports** | The Markdown template of the weekly status report |
  | **Data & backup** | **Data location** (new), Google Drive (22), Backup and restore (20) |
  | **Security** | Encryption (21) |
  | **Developer** (debug builds only) | Demo data (24) |
- **Working days** (new, `Settings.work_week`, stored as a list of weekday numbers, Monday = 0; default Monday to Friday, at least one day). Threaded through core as `WorkWeek`: the schedule/critical path (`schedule::working_index/end_index/date_of` now take the week), impact analysis (incl. `late_working_days`), capacity (a week has as many working days as the work week; capacity days = weekly hours / hours per day, unchanged), the Overview, the dependency graph and the weekly review's slip counts. The Gantt axis only contains working days and marks a new week at its first working day. Weeks themselves still run Monday to Sunday (This week, the review, the heatmap columns). Quick-add dates (`fri`, `+3d`) are calendar dates and unchanged. Synced between devices like any other non-device setting.
- **Default weekly capacity** (new, `Settings.default_weekly_capacity_hours`, > 0 and <= 168, default 40): used by `people::create` when no capacity is given (UI "New person", quick-add, `@name` creation). Existing people are untouched.
- **Data location** (new): `get_data_info` (database path, folder, size, schema version, encrypted, log path) and `show_data_folder` (opens the app's own data folder in the file manager; takes no path, so it can only open that folder). Read-only.
- Number fields (`NumberSetting`) save when they lose focus, say why when the text isn't acceptable, and put the stored value back, so a box never shows something that isn't saved.
- Commands: `get_settings`, `update_settings` (unchanged names; all-or-nothing validation), plus `get_data_info`, `show_data_folder`.

**Out / later**
- **Developer tab** (seed demo data in debug builds): added by spec 24 (`Tab::Developer`, `components/developer_settings.rs`).

## Acceptance criteria
- [x] Every setting persists across restarts (rows in the `settings` table; tested in the store) and takes effect without a restart: the work week, hours per day, thresholds and default capacity are read by each command call, the theme is applied at once.
- [x] Settings are divided into tabs by type.
- [ ] Click-through on the real app (see below).

## Decisions
- **Open question "Allow moving the DB to a different location?": no.** The folder holds more than the database: the media cache, backups by default, the log, and files the encryption swap (ADR-0010) and Drive snapshots (ADR-0011) create next to the database. A move would have to cope with WAL files, an interrupted move, a keychain entry tied to the file, a target on a network or synced drive (which SQLite must never live on), and the pre-migration backup path. The safe ways to put data somewhere else already exist: make a backup and restore it there (20), or connect Google Drive (22). The Data tab says so, and the location is shown read-only with a *Show in folder* button. This also retires the spec 20 idea of warning when the live DB is moved into a synced folder.
- **Theme "system/light/dark"** from the original scope was already covered by the richer picker (System, plus every light and dark theme), so no separate control was added.
- Spec 20's backup folder and auto-backup, and the theme, stay per-device settings (not synced); the work week and default capacity are shared by every device.

## Implementation notes
- Types: `WorkWeek` (`work_week.rs`: `from_days`, `contains`, `with`, `days_per_week`, `before`, `nth`, `summary`; serde as a list of weekday numbers), `WEEKDAY_NAMES`, `Settings.work_week` / `default_weekly_capacity_hours` (+ `UpdateSettings`), `DataInfo`.
- Core: `schedule::compute/compute_with` take a `WorkWeek` after `today`; `GraphInput`, `CapacityInput`, `OverviewWorld`, `impact::World`, `ReviewInput` carry `work_week`. Calendar arithmetic generalises Monday-to-Friday: `index = week * k + (working weekdays before the day)`.
- Store: `settings::default_weekly_capacity_hours(conn)`; a stored work week that no longer parses falls back to Monday to Friday.
- UI: `settings_tab::Tab` (ids, labels, `step`, `path`), `pages/settings.rs` (`TabBar`, `NumberSetting`, `WorkWeekPicker`, `TimeSettings`, ...), `components/data_settings.rs`.

## Not yet verified by hand
- Each tab shows its cards; the address changes with the tab (`?tab=`), reload keeps the tab, arrow keys move between tabs
- the status bar's "Connect Google Drive" opens *Data & backup*; the review's "Edit template" opens *Reports*
- Working days: switching Friday off changes the Gantt axis, the Capacity heatmap's weekly capacity in days and project finish dates at once; the last day can't be switched off
- a new person gets the default weekly capacity (and an existing one keeps theirs)
- Data location: the paths are right, *Show in folder* opens the folder in the file manager, encrypted/not encrypted follows Security
