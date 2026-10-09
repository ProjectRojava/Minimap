# 36 — Latest notes on This week

Status: Implemented — awaiting manual check · Milestone: — · Priority: Should
Depends on: 09, 16, 34

## Goal
See what you last wrote about a task right where you decide what to do about it.

## Scope
**In** (built)
- Every task in *Needs attention* and *Priorities this week* shows, under its row, its three newest notes: date, a short read of the text (first drawn as plain text, now rendered as the note's Markdown, about four lines; see below), a *1:1* or *meeting* tag where it applies, and a *+N earlier* line when there are more. Clicking a note opens it in the pane; clicking the task's own line still opens the task.
- **Which notes**: every active note that mentions the task (the notes under *Notes and findings*, and any note that `@`-mentions it), newest first by note date then creation. Looking back at a past day leaves out notes dated after it.
- **Where it is computed**: core (`this_week::recent_notes`) from the notes already loaded for the 1:1s, so there is no new command or query; `ThisWeek.task_notes` carries them. `TaskNote.snippet` is `core::notes::snippet` (non-empty lines joined with " · ", markup stripped, the closing "About @task" line left out; used for nothing on screen now but kept for tests and tooltips). **Rendered text**: `TaskNote.body` is `core::notes::preview_body(body, PREVIEW_CHARS = 600)`: the Markdown without the closing line, cut at a line end (or between words, never inside a `@[Name](node:…)` mention) with a closing `…` line; the screen shows it with `MarkdownView` (so `[text](url)` is an underlined link that opens in the browser, mentions open their item) inside a box about four lines high; clicks on a link or mention are handled there, other clicks open the note (`markdown_box::clicked_a_link`). The row is a `div role=button`, not a `<button>`, because it holds links.
- `NodeRow` gains a `below` slot so a row can carry content under its line (aligned with the title).

**Out**
- Notes under other sections (waiting on, 1:1s, the weekly review); a setting for how many notes; rendering the notes as formatted Markdown (they are plain text snippets); demo data (no schema change).

## Acceptance criteria
- [x] Each listed task gets its newest three notes in order, with the true total; a task that is not listed, or a note dated in the future, is not included (`core::this_week` tests).
- [x] A look back to a past day leaves out later notes (`core::this_week` test).
- [x] Through a real database: notes made with the item's closing mention reach the screen (`commands/this_week.rs` test).
- [x] The snippet joins lines and drops the closing line (`core::notes` test); the page's text helpers (`ui::pages::this_week` tests); Help describes it.
- [ ] Click-through (see below).

## Not yet verified by hand
- On a task in *Needs attention*, add two notes under *Notes and findings*: they appear under its row on This week (newest first) without leaving the pane open
- Click a note: the note opens in the pane; click the task title: the task opens
- Add a fourth note: *+1 earlier* shows
- Click a past day in the strip: notes dated after it are gone
