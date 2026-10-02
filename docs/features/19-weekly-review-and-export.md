# 19 — Weekly review and Markdown export

Status: Draft · Milestone: M4 · Priority: Should
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
- [ ] With demo data, the report is readable without edits and matches an `insta` snapshot.
- [ ] Export writes only to the path the user chose.

## Open questions
- Report template: fixed, or user-editable?
- Report audience: board/exec summary vs team-level detail?
