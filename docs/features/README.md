# Feature specs

One file per feature. Each is a draft for you to edit before handing it to Claude to implement.

**Workflow**
1. Edit a spec: fix scope, answer the *Open questions*, and set `Status: Ready`.
2. Hand it over, e.g. "implement docs/features/03-people-and-teams.md".
3. Claude implements it, ticks the acceptance criteria, sets `Status: Done` and updates `docs/progress.md`.

**Priority**: `Must` = MVP. `Should` = MVP if time allows. `Later` = after MVP.
Numbers give a suggested build order; `Depends on` lists hard prerequisites.

| # | Feature | Milestone | Priority | Depends on |
|---|---|---|---|---|
| 01 | [Data model and activity log](01-data-model-and-activity-log.md) | M1 | Must | — |
| 02 | [App shell and navigation](02-app-shell-and-navigation.md) | M1 | Must | — |
| 03 | [People and teams](03-people-and-teams.md) | M1 | Must | 01, 02 |
| 04 | [Objectives](04-objectives.md) | M1 | Must | 01, 02 |
| 05 | [Projects](05-projects.md) | M1 | Must | 03, 04 |
| 06 | [Tasks and inbox](06-tasks-and-inbox.md) | M1 | Must | 05 |
| 07 | [Edges, linking and cycle detection](07-edges-and-cycle-detection.md) | M1/M2 | Must | 03–06 |
| 08 | [Waiting-on](08-waiting-on.md) | M3 | Must | 03, 07 |
| 09 | [Notes and @mentions](09-notes-and-mentions.md) | M3 | Must | 03, 07 |
| 10 | [Decisions](10-decisions.md) | M3 | Should | 07 |
| 11 | [Search](11-search.md) | M1 | Must | 03–06 |
| 12 | [Command palette and quick-add](12-command-palette-and-quick-add.md) | M3 | Must | 06, 07, 11 |
| 13 | [Schedule and critical path](13-schedule-and-critical-path.md) | M2 | Must | 07 |
| 14 | [Impact analysis](14-impact-analysis.md) | M2 | Must | 13 |
| 15 | [Health scoring and Overview](15-health-and-overview.md) | M3 | Must | 13 |
| 16 | [This week view](16-this-week-view.md) | M3 | Must | 06, 08 |
| 17 | [Capacity](17-capacity.md) | M3 | Should | 13 |
| 18 | [Dependency graph view](18-dependency-graph-view.md) | M2 | Later | 13 |
| 19 | [Weekly review and Markdown export](19-weekly-review-and-export.md) | M4 | Should | 15, 16 |
| 20 | [Local backup and restore](20-local-backup-and-restore.md) | M4 | Must | 01 |
| 21 | [Encryption](21-encryption.md) | M4 | Should | 20 |
| 22 | [Google Drive storage and autosave](22-google-drive-backup.md) | M4 | Must | 20, 21 |
| 23 | [Settings](23-settings.md) | M4 | Must | 02 |
| 24 | [Demo data](24-demo-data.md) | M2 | Must | 03–07 |
| 25 | [Undo](25-undo.md) | M4 | Should | 01 |
| 26 | [Full data export](26-data-export.md) | M4 | Should | 01 |
| 27 | [Recurring items](27-recurring-items.md) | — | Later | 06, 08 |
| 28 | [Reference links on tasks](28-reference-links.md) | — | Should | 06 |
| 29 | [Subtasks](29-subtasks.md) | — | Should | 06, 07 |
| 30 | [Ongoing objectives](30-ongoing-objectives.md) | — | Should | 04, 15, 16, 19 |

`Must`/`Should`/`Later` follow the MVP brainstorm and are suggestions; change them freely. Where a spec disagrees with `CLAUDE.md`, the spec wins once you mark it Ready, and Claude records the deviation as an ADR.
