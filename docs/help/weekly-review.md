# Weekly review

The **Weekly review** (`g r`) is a guided look back at the week, in seven short steps, with the fixes at hand, ending in a **status report** you can send. Weeks run Monday to Sunday; the arrows next to the dates move to another week.

## The seven steps

Move with the step buttons (each shows how many items it has), the **Back/Next** buttons, or the `n` and `p` keys. `j`, `k` and `Enter` walk the rows of a step.

| Step | The question | What is listed |
|---|---|---|
| **1. Slipped** | What slipped? | Tasks whose due date was **moved later** this week (counted in working days), tasks that **came due this week and are still open**, and projects and objectives whose **target** was pushed out |
| **2. Blocked** | What is blocked? | Tasks with status *blocked*, those that became blocked this week first |
| **3. Overloaded** | Who has too much on? | People over capacity this week or next, or with too many open tasks |
| **4. Waiting on** | What is stale? | Stale waiting-ons, plus what was resolved this week |
| **5. Decisions** | What was decided? | Decisions made this week (proposals are left out) |
| **6. Done** | What got done? | Tasks finished this week, and projects and objectives marked done |
| **7. Report** | What do I tell people? | The status report |

## Fixing things as you go

Rows have quick fixes so you don't have to leave the review: **Tomorrow**, **Next week** or a typed date on slipped and blocked tasks; an **assignee** picker; **Unblock** on blocked tasks; **Resolve** or **Snooze** on stale waiting-ons; **Open Capacity** on overloaded people.

## The status report

Step 7 shows the exact Markdown report, written for a board or executive audience: a summary (projects at risk, completed, slipped, blocked, decisions, people over capacity, stale waiting-ons), objectives with health, top risks and the sections for the week. Two buttons:

- **Save as Markdown…** writes it to a `.md` file you choose;
- **Copy to clipboard**.

## Changing the report

The layout comes from a **template** in *Settings → Reports* (the *Edit template* button goes there). Write Markdown and put placeholders where the sections should go:

| Placeholder | Fills in |
|---|---|
| `{{title}}` | The report title |
| `{{week_start}}`, `{{week_end}}` | First and last day of the week |
| `{{generated_on}}` | The day the report was made |
| `{{summary}}` | Headline numbers |
| `{{objectives}}` | Objectives with health, as a table |
| `{{risks}}` | The top risks |
| `{{done}}`, `{{slipped}}`, `{{blocked}}` | What got done, slipped, is blocked |
| `{{decisions}}` | Decisions made |
| `{{capacity}}` | People over capacity |
| `{{waiting}}` | Stale or resolved waiting-ons |

Each section fills in *without* a heading, so your template sets the headings, their order and which sections appear. A template with an unknown placeholder or an unclosed `{{` is refused and nothing changes; *Reset to default* brings back the built-in one. User text in the report (titles and names) is escaped so it can't break the Markdown.

## A good rhythm

Do the review at the same time each week: clear *Slipped* and *Blocked*, chase *Waiting on*, rebalance *Overloaded*, then send the report. It is also the best way to keep dates honest.
