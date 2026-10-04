# 14 — Impact analysis ("What if this slips?")

Status: Implemented — awaiting manual check · Milestone: M2 · Priority: Must
Depends on: 13

## Goal
Show the downstream effect of a slip before it happens. The product's headline feature.

## Scope
**In**
- Core: input node (task or project) + N working days; propagate along `blocks` and `depends_on`, consuming slack per step.
- Output: affected tasks (old/new finish, slip absorbed, remaining slip), affected projects (new projected finish vs target), objectives (via `contributes_to`), people (via `assigned_to`).
- Command: `run_impact_analysis(node_id, slip_days)`.
- Screen: pick node + slip days; results grouped by tasks / projects / objectives / people with before→after dates; "late vs target" highlighted.
- "What if this slips?" button on every task and project detail.
- Read-only: nothing is saved.

## Acceptance criteria
- [ ] Demo data: a 5-day slip on a chosen task matches an `insta` snapshot. *(deferred: demo data is spec 24 and `insta` isn't available offline; hand-built tests assert exact numbers, dates and the absorbed/passed split instead)*
- [x] Proptest: impact never moves any task earlier; absorbed + passed-on = slip at every step. *(also: no task moves more than the total slip, unreachable tasks are never reported, more slip never means less delay, and applying the plan reproduces the scenario)*

## Decisions
- **Multiple simultaneous slips: yes.** A scenario is a list of slips (tasks or projects); `run_impact_analysis(slips)` takes the list (one slip is a list of one). The same task slipped twice slips by the sum; two slips feeding one task push it by the later of the two.
- **"Apply this slip": yes, as an explicit, previewed, all-or-nothing action; what-if itself stays read-only.** "Apply to plan…" lists exactly which dates would change, then asks. Recording a slip means: each slipped task gets a **start date** (its baseline start plus the slip) and its **due date moves by the same working days**. Targets are never changed (a missed target should show as late). Tasks that are merely pushed along by `blocks` links are not touched, because the schedule moves them by itself and pinning them would freeze them. Tasks held back by a `depends_on` knock-on are pinned too, because the plain schedule can't derive that. One transaction, one activity entry per task, and the scenario is cleared afterwards so it isn't applied twice. The command re-derives the changes from the plan as it is at that moment instead of trusting the screen.
- **What "slip" means**: a task slips by starting that many working days later (so it finishes that much later); a project slips by holding **all its open tasks** back. A finished or cancelled task can't slip.
- **Analysis = re-run the schedule (13) with the slip as a "not before" and diff**, so it can't disagree with the Schedule section: slack, lag, start dates, targets and cross-project links all count the same way. `absorbed` is how much of what arrived was soaked up by slack on the way in; `delay` is how far the task's own finish moves; `incoming = absorbed + delay` for every task. Tasks that absorb everything are listed with delay 0: that is where the slip stops.
- **`depends_on` between projects is finish-to-start**: if the project it needs finishes `d` days later and the plan left `r` days of room between them, the dependent project's work starts `max(0, d - r)` days later, repeated along chains of dependencies. (The plain schedule ignores `depends_on`; this is why applying pins those tasks.)
- **Objectives** are affected when a contributing project's projected finish or a contributing task's finish moves; they show the latest contributor finish before/after against the objective's target. **People** are listed for assigned tasks that finish later (largest delay first). **Late** is shown for tasks (due date), projects and objectives (target), both before and after, with "(new)" when it was on time before.
- **Entry points**: a "What if this slips?" button in every task and project panel (starts a 5-working-day scenario), the sidebar screen "What if" (`g f`), and the palette action "What if this slips?" (the one spec 12 left out).

## Implementation notes
- Core `impact::analyze(world, slips)`, `impact::apply_plan(world, slips)`, `late_working_days`; `schedule::compute_with(.., not_before)` is the hook. Types in `minimap-types::impact`. Commands `run_impact_analysis(slips)`, `preview_apply_slips(slips)`, `apply_slips(slips)`; store `tasks::update_many` (+ `update_in_tx`).
- UI: `pages/what_if.rs` (scenario editor with a search picker and per-slip days, summary line, Tasks / Projects / Objectives / People with before -> after, Apply bar), `components/what_if_button.rs`, `Scenario` context.

## Not yet verified by hand
- open a task with successors: "What if this slips?" opens the screen with the task at 5 days; the Tasks list shows it as "slipped" and its successors with before -> after finishes
- change the days; add a second slip with the search box; remove one with ✕
- a successor with slack shows "absorbed N days: stops here" instead of a delay
- a project with a target / an objective with a target turn red with "late by N working days (new)" when pushed past it
- People lists the assignees of the tasks that move
- "Apply to plan…" lists the exact start/due changes; Cancel does nothing; Apply updates the task (start date set, due moved), toasts, clears the scenario, and the project's Schedule section now shows the later finish; the task's activity shows the change
- a finished task can't be picked (the error says so)
