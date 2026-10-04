# 19 — Weekly review and Markdown export

Status: Implemented — awaiting manual check · Milestone: M4 · Priority: Should
Depends on: 15, 16

## Goal
A guided 15-minute weekly ritual that ends with a status report the user can send.

## Scope
**In**
- Steps (each a screen, `n`/`p` to move): what slipped, what got blocked, overloaded people, stale waiting-ons, decisions made, what got done. Built from the `activity` table and current state.
- Each step allows quick fixes inline (reschedule, resolve, reassign).
- Final step: "Export as Markdown status report" → save dialog → `.md` file. Also "Copy to clipboard".
- Commands: `get_weekly_review(week_start)`, `export_markdown(report_kind, params, path)`.
- Tauri dialog plugin with only the save-dialog permission.

## Acceptance criteria
- [x] With demo data, the report is readable without edits and matches an `insta` snapshot. *(Two `insta` snapshots in core: a week with something in every section, and an empty week; both read cleanly without edits. Demo data itself is spec 24; the fixture is hand-built.)*
- [x] Export writes only to the path the user chose. *(`export_markdown` only accepts an absolute path ending in `.md`/`.markdown` in an existing folder, builds the whole report before touching the disk, writes that one file and nothing else; tests check the folder holds exactly the chosen file, and that refused paths write nothing.)*

## Decisions
- **Report template: user-editable** (your answer). The template is Markdown with `{{placeholders}}` stored in Settings (`settings.report_template`, key `report_template`); Settings → Status report has the editor, a placeholder list, Save and "Reset to default". Placeholders: `title`, `week_start`, `week_end`, `generated_on`, plus one per section: `summary`, `objectives`, `risks`, `done`, `slipped`, `blocked`, `decisions`, `capacity`, `waiting`. A section placeholder expands to the section's body **without a heading**, so the template decides headings, order and which sections appear. A template is checked on save (not blank, at most 20,000 characters, every placeholder known, every `{{` closed); a refused template changes nothing and keeps what was typed. An empty save restores the built-in one. The shared list is `minimap_types::REPORT_PLACEHOLDERS`, the default text `DEFAULT_REPORT_TEMPLATE`.
- **Audience: board / executive summary** (your answer). The built-in report is short and plain: headline numbers, an objectives table (health word, target, the two worst reasons), the top risks, then what got done, slipped, is blocked, decisions made, who is over capacity and what is stale. Health reads as *On track / At risk / Off track* (green / amber / red). No ids or internal jargon. Long lists are capped (15 completed tasks, then "…and N more"). Team-level detail stays in the app (Weekly review steps, This week, Capacity).
- **What each step shows** (weeks are Monday to Sunday; any date in a week picks it; ‹ › move between weeks):
  - *Slipped*: tasks whose due date was **moved later** during the week (activity log: first date of the week to last, counted in working days; moves earlier or first-time dates are not slips; finished tasks are skipped), open tasks that **came due this week and are still open** (before today), and projects/objectives whose **target date was moved later**. Biggest first, projects and objectives ahead of tasks of equal size.
  - *Blocked*: every task with status Blocked now, with what blocks it; the ones that **became** blocked this week (activity) are marked new and listed first.
  - *Overloaded*: the Overview's list: over capacity this week or next, or over the open-task limit (state right now, whatever week is shown).
  - *Waiting on*: stale open waiting-ons (right now) plus those **resolved** during the week.
  - *Decisions*: decisions **decided** (or since replaced) with a decision date inside the week; proposals are not decisions made.
  - *Done*: tasks completed in the week (`completed_at`) and projects/objectives marked done in the week (activity), most important first.
- **Inline fixes**: Slipped and Blocked tasks get Tomorrow / Next week / a typed date (`fri`, `+3d`, as on This week) and a reassign dropdown; Blocked also has **Unblock** (back to To do); Waiting on has Resolve and Snooze 3 days; Overloaded has "Open Capacity" to see the tasks behind the load. Every row opens in the detail pane, and the review refreshes after each fix.
- **Save and copy**: the Report step shows the exact Markdown in a read-only box. **Save as Markdown…** opens the OS save dialog (Tauri dialog plugin, **only `dialog:allow-save` is granted**, no open dialog and no file-system permissions for the webview), then `export_markdown(report_kind, params, path)` writes the file. **Copy to clipboard** uses the browser clipboard API; if the webview refuses it the text is selected and a toast says to press Ctrl+C. The suggested name is `status-report-<Monday>.md`.
- **Why the path is checked in the backend**: the webview could hand any string to `export_markdown`, so the command accepts only an absolute `.md`/`.markdown` path in an existing folder (not a folder itself); the dialog already asks before overwriting.
- **Extra command**: `render_report(report_kind, params)` returns the Markdown text (the Report step's preview and Copy) so the screen and the file are the same text. `report_kind` is `weekly_status` (the only kind for now).

## Implementation notes
- Types: `minimap-types::review` (`WeeklyReview`, `ReviewSlip`/`SlipKind`, `ReviewBlocked`, `ReviewDone`, `ReviewFinished`, `ReportKind`, `ReportParams`, `ExportResult`, `REPORT_PLACEHOLDERS`, `DEFAULT_REPORT_TEMPLATE`); `Settings.report_template`, `UpdateSettings.report_template`.
- Core: `weekly_review::build(ReviewInput)` (pure, from tasks, blockers, a week of activity, decisions, waiting-ons, projects, objectives and the overview), `week_of`; `report::{render, validate_template, escape, TITLE}` (Markdown escaping of user text; two `insta` snapshots in `crates/minimap-core/src/snapshots/`).
- Store: `activity::list_between(from, to)` (inclusive UTC days, oldest first); `settings` validates the template with core and deletes the key on reset.
- Commands (`commands/review.rs`): `get_weekly_review(week_start)`, `render_report`, `export_markdown`; ADR-0009 records the dialog plugin and the export path rule.
- UI: `pages/weekly_review.rs` (route `/weekly-review`, sidebar Review group, `g r`), `TaskActions` from This week reused with an "always visible" option, Settings → Status report; `api::save_dialog` and `api::copy_text` wrap `window.__TAURI__.dialog.save` and `navigator.clipboard.writeText`.

## Not yet verified by hand
- Weekly review opens from the sidebar (or `g r`); `n`/`p` and the step buttons move between the seven steps; the counts on the steps match the rows
- change a task's due date to a later day, then Slipped shows it with the working days; "Tomorrow", a typed date and the reassign dropdown work and the row updates
- mark a task Blocked: it appears under Blocked as "new"; Unblock puts it back to To do
- Resolve and Snooze work on stale waiting-ons; a waiting-on resolved today shows under "Resolved this week"
- decide a decision today: it appears under Decisions; complete a task: it appears under Done
- Report step: the text reads well; **Save as Markdown…** shows the OS save dialog, the file lands where chosen and opens as Markdown; cancelling the dialog does nothing; **Copy to clipboard** puts the text on the clipboard
- Settings → Status report: edit the template (drop a section, add your own heading), Save, and the Report step follows; an unknown `{{placeholder}}` is refused with a message; Reset to default restores the original
