# Quick-add grammar

One line of text creates a task, project, waiting-on, note or decision. Parsing is pure and lives in `minimap-core::quick_add`; the command palette (`Ctrl/Cmd+K`) always shows a preview and asks about anything unclear before `Enter` adds it.

```
task Fix login timeout @priya #api-launch !2 due:fri est:3d blocks:"Release 1.2"
project Q1 EU region owner:@me target:2027-03-31 for:"Launch EU"
wait @raj on "Security review sign-off" by:next-wed
note 1:1 @priya
decision "Postgres over Mongo" affects:#api-launch
```

## Shape of a line
- **Keyword first** (case-insensitive): `task`, `project`, `wait` (or `waiting`), `note`, `decision`. **No keyword means a task**, so `Fix login timeout !1` is a task. A task whose title starts with a keyword needs the keyword: `task note taking policy`.
- The rest is **title words** mixed with markers. Words keep their order and are joined with spaces; markers can go anywhere. Quote text to keep spaces together: `"Release 1.2"`, `due:"..."`, `@"Priya Shah"`.
- **Quoting also switches markers off**: a token that *starts* inside quotes is plain text (`"#123 is a bug"`, `"@home"`). Anything unrecognised (`10:30`, `re:`, `!wow`) is just a word.
- An unclosed quote is reported and nothing is added until it is closed.

## Markers
| marker | meaning | where |
|---|---|---|
| `@name` | a person; `@me` is you | task: assignee (default: you), project: owner, wait: who you wait on (required), note: mentioned |
| `#handle` | a project, by handle (`api-launch`) or name | task: its project (default: inbox), wait: what it is about, note: mentioned, decision: affected |
| `!1`..`!5` | priority, 1 highest | task, project |
| `due:` / `by:` | due date | task; `by:` is also a wait's expected date |
| `start:` | start date | task, project |
| `target:` | target date (`due:`/`by:` also work) | project |
| `est:` | estimate: `3d`, `1.5d`, `4h` (hours use the hours-per-day setting) | task |
| `blocks:` | a task this one blocks (repeatable) | task |
| `for:` | an objective it contributes to (repeatable) | task, project |
| `owner:` | the owner (same as `@name`) | project |
| `about:` | the task or project a wait is about (`#project` also works) | wait |
| `affects:` | a project, task or objective (repeatable; `#x` means a project) | decision |
| `date:` | the note's date / the decision's date | note, decision |
| `kind:` | `1:1`, `meeting`, `general` | note |
| `status:` | `proposed` or `decided` | decision |
| `1:1` | bare word: makes the note a 1:1 | note |

A marker not used by the kind is reported ("`est:` isn't used for a project"), never silently dropped. A second assignee/project/etc. is an error; the same reference twice counts once.

### Per kind
- **task**: needs a title. Missing `@` = assigned to you; missing `#` = inbox.
- **project**: needs a title; the handle is made from it.
- **wait**: needs a description and an `@person`. The linking word `on` right after the person is dropped: `wait @raj on "Sign-off"` and `wait @raj Sign-off` are the same.
- **note**: a 1:1 needs a person and is titled `1:1 with <name>` unless you give a title; `kind:meeting` with no title is titled "Meeting"; a general note needs a title. Mentioned people/projects become `@[Name](node:id)` in the body, so they link automatically.
- **decision**: needs a title; created as proposed unless `status:decided`.

## Dates
| text | means |
|---|---|
| `today`, `tomorrow` | |
| `fri`, `friday`, `mon`... | the **next** such day; **today if it is that day** |
| `next-fri`, `next-wed`... | that weekday **in the next calendar week** (Monday to Sunday) |
| `+3d`, `+2w` | days / weeks from today |
| `2027-03-31` | ISO date |

Examples with today = Wednesday 2027-03-03: `wed` = 2027-03-03, `fri` = 03-05, `mon` = 03-08, `next-wed` = 03-10, `next-mon` = 03-08, `+2w` = 03-17. Anything else is reported with the accepted forms. The preview shows the date with its weekday ("Fri 2027-03-05") so a misunderstanding is visible before you commit.

## Names
`@name`, `#project` and `blocks:`/`for:`/`affects:`/`about:` values are matched against active nodes, in this order, stopping at the first step with a match:
1. the whole name or handle (case-insensitive);
2. every typed word is a whole word of the name (`@priya` -> Priya Shah);
3. every typed word starts a word of the name (`@pri`);
4. a typo away (`@priyaa`): offered as "did you mean", **never taken silently**.

One match at steps 1-3 is used; several ask you to pick. Nothing matched, or a guess, also asks:
- pick one of the candidates,
- **Create** a new person / project / objective / task with the typed name (not offered for `affects:`/`about:` values whose kind is unclear; `#x` there means a project),
- **Skip** it (what happens is shown: "assigned to you instead", "goes to the inbox", "not linked"). Required references (who a wait is from) can't be skipped.

Answers are given in order, from the keyboard (arrows + Enter) or the mouse. The whole line is one transaction: if anything fails, nothing (including newly created people or projects) is written.
