# 12 — Command palette and quick-add

Status: Draft · Milestone: M3 · Priority: Must
Depends on: 06, 07, 11

## Goal
`Ctrl/Cmd+K` to go anywhere, run any action and capture work in one line. This is the main way data gets in.

## Scope
**In**
- Palette: navigate to screens, search nodes (11), run actions ("New project", "Resolve waiting-on…", "What if this slips?").
- Quick-add grammar per `CLAUDE.md` §7, documented in `docs/quick-add-grammar.md`:
  ```
  task Fix login timeout @priya #api-launch !2 due:fri est:3d blocks:"Release 1.2"
  project Q1 EU region owner:@me target:2027-03-31 for:"Launch EU"
  wait @raj on "Security review sign-off" by:next-wed
  note 1:1 @priya
  decision "Postgres over Mongo" affects:#api-launch
  ```
- Parser in `minimap-core`: pure, returns a structured preview with resolved/ambiguous/unresolved references.
- Date parser: `today`, `tomorrow`, weekday names (`fri` = next Friday, or today if Friday? decide), `next-wed`, `+3d`, `+2w`, ISO dates.
- Ambiguous `@name` / `#project` → pick from candidates inline. Unresolved → offer create, or send to inbox.
- Commands: `parse_quick_add(text)` → preview; `commit_quick_add(text, resolutions)`.
- Preview always shown before commit; `Enter` commits.

## Acceptance criteria
- [ ] Every example above parses and commits correctly (snapshot tests on the parse output).
- [ ] Ambiguous person asks to pick; `@me` resolves to self.
- [ ] Palette opens in < 50 ms and is fully keyboard driven.

## Open questions
- `fri` on a Friday: today or next week?
- Without a leading keyword (`task`, `project`…), default to task?
