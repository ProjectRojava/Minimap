# 34 — The Markdown box

Status: Implemented — awaiting manual check · Milestone: — · Priority: Should
Depends on: 06, 09, 22, 28

## Goal
Make writing long text feel like a GitHub comment: read formatted text first, a pencil to edit, a toolbar, Write/Preview, explicit Save/Cancel where a draft should not overwrite good text, and a thread for notes on an item.

## Scope
**In** (built)
- **One shared box** (`components/markdown_box.rs`): `MarkdownBox` (tabs, toolbar, `@` picker, paste/drop attach), `MarkdownView` (formatted text; mentions, attachments and web/mail links open), `MarkdownField` (read first, pencil, Save/Cancel).
- **Toolbar**: heading, bold, italic | quote, code, link | bulleted, numbered, task list | mention, attach. Pure edits in `ui/src/md_edit.rs` as splices applied through the browser's edit command so `Ctrl/Cmd+Z` in the box stays native. Keys: `Ctrl/Cmd+B`, `Ctrl/Cmd+I`, `Ctrl/Cmd+Enter` (submit), `Esc` (cancel), `Enter` continues a list or quote and ends it on an empty item. Not built: strikethrough, tables, Tab to indent, a link shortcut (`Ctrl/Cmd+K` is the palette).
- **Descriptions** of task, project, objective and team, the **notes** of a person and the **context / decision / rationale** of a decision are `MarkdownField`s: formatted text, a pencil (or double-click), *Save* / *Cancel*. `Esc` cancels only an unchanged draft. No schema change: plain text is valid Markdown; the waiting-on text stays a one-line field.
- **Notes and findings** (`item_notes.rs`) is a thread, oldest first, newest ten shown (*Show earlier notes*): each card formatted with its date and kind, pencil = edit in place (Save/Cancel), ⋯ = Open in the pane / Archive (undoable). The "About @[item]" closing line that ties the note to the item is hidden and kept on save; the title follows the first line unless edited by hand. The composer is at the bottom (*Add note*, `Ctrl/Cmd+Enter`); files attach to the item.
- **The note screen's body** uses the same box with its autosave and the pencil / *Done* it already had.
- **Rendering** (`core::notes::render`): a single newline is a line break; a mention shows one `@`; links stay href-less but `MarkdownView` opens a clicked `span.link` through `open_link` (http, https, mailto only).
- Mentions in a description or decision are render-only: no `mentions` link is made (that edge is note-to-anything only).

**Out**
- Mentions in descriptions that make graph links; the search index and the Markdown export still hold the raw `@[Name](node:id)` token of such a mention (search for the word "node" may find them); a draft kept when you leave the pane mid-edit; strikethrough, tables, indent keys; the waiting-on text.

## Acceptance criteria
- [x] Each toolbar edit is right on its own and toggles back; `Enter` continues lists; offsets inside a character are safe (`ui::md_edit` tests).
- [x] A note thread keeps the closing line out of the card and puts it back on save; titles follow the first line (`ui::item_notes` tests).
- [x] Single newlines are line breaks, a mention shows one `@` (`core::notes` tests).
- [x] Descriptions, person notes and decision text no longer have plain boxes; the Help page describes the box (`docs/help/notes.md`, page-truth tests pass).
- [ ] Click-through (see below).

## Not yet verified by hand
- Open a task: Description reads as text with a pencil; click it, type `- a`, press Enter (the next line starts `- `), press Bold with a word selected, then `Ctrl+Z` (undoes the bold only); *Save*; reopen: the text is there; *Cancel* restores the old text
- In the toolbar press @ and pick a task; Preview shows the mention; click it in the saved text and the task opens
- Notes and findings: add two notes (oldest first); edit one in place; ⋯ → Archive, then `Ctrl+Z` brings it back; a pasted screenshot is attached to the item and shows in the note
- Paste a screenshot into a description; click a web link in a saved description and it opens in the browser
