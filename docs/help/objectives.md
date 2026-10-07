# Objectives

An **objective** is an outcome you want, such as *Launch in the EU* or *Cut platform costs by 20%*. Projects and tasks **contribute** to objectives, which is how Minimap rolls health up from the work to the goal.

Two kinds: a **goal** has an end (a target date, and it can be *done*); an **ongoing** objective never ends, like *Keep internal systems healthy*, so it has no target date, is never "done" (archive it if it ever stops), and is judged by its work and a regular review. See *Ongoing objectives* below.

## The screen

Objectives are listed by priority (1 is highest), then target date, then title. A toggle groups them by **calendar quarter** of their target date, with a *No date* group for goals without one. Ongoing objectives are always in a group of their own, *Ongoing*, at the end, whichever way you group; the date column shows an *ongoing* pill (or an amber *review due* pill when its review is overdue). Each row shows how many projects and tasks contribute.

## Colours

Every objective has its own **colour**, shown as a dot and a coloured edge on its row. Minimap picks it for you, in the order the objectives were created, and keeps the colours far apart so two objectives never look alike. The same colour appears on the projects and tasks that serve the objective (see [Projects](help:projects) and [Tasks and the inbox](help:tasks)), so you can see what belongs together at a glance. If you archive an objective, the ones created after it each move up one colour.

## The detail pane

The pane opens with a row of coloured pills: your assessment (green on track, amber at risk, red off track), the priority (P1 and P2 in amber) and the target date (red "overdue" once it has passed while the objective is not done). The dropdowns for assessment and priority are coloured the same way, and the status of each contributing project or task is a coloured pill.

- **Your assessment**: status (*on track, at risk, off track, done*) and priority. This is your judgement. Minimap shows its own **computed health** next to it, so you can see when they disagree.
- **Kind**: *Goal: has an end* or *Ongoing: no end*. Switching to ongoing clears the target date (and moves "done" back to on track); switching back lets you set a date again.
- **Target date** and a description (goals only).
- **Contributions**: the projects and tasks that contribute, each with a **weight** from 0 to 1. Add or remove them here. (You can also set it from the project's own pane.)
- **Health**: the computed health and why.
- **Notes and findings**: write a note about the objective (progress, reviews, decisions) and open the ones that mention it. They are ordinary [notes](help:notes).

## Ongoing objectives

Some outcomes never finish: keeping the systems patched, access reviewed, costs in check. Mark such an objective **Ongoing** (the *Ongoing* box when you add one, or *Kind* in its pane).

- **No end date, never done.** There is no target date, so nothing about it is ever "late" and it is not offered the *done* status. When it stops mattering, archive it.
- **A review instead of a deadline.** An ongoing objective is reviewed on a rhythm: *Review* offers never, every week, every 2 weeks, every month (the default), every quarter, every 6 months or every year. *Last reviewed* shows when, and *Mark reviewed* sets it to today. The next review is due one rhythm after the last (or after the objective was created, if it never was).
- **A late review is its "overdue".** When the review date passes, the objective's health drops to **amber** with the reason *review overdue by 12 days (due 2027-02-19, every 30 days)*, a *review overdue* pill shows in its pane, and it is listed on [This week](help:this-week) and in the [Weekly review](help:weekly-review) until you mark it reviewed. Reviews due by Sunday are listed too.
- **The rest is the same.** Projects and tasks contribute with weights as for any objective (recurring tasks suit it well: see [Recurring items](help:recurring)), and the roll-up of their health, with *one red contributor caps it at amber*, applies. With nothing contributing it is *not scored*.

## How computed health works

The objective's health is a **weighted average** of what contributes to it, with one safeguard: *one red contributor caps the objective at amber*, so one disaster can't hide behind several fine projects. A goal's own target date is judged like a project's; an ongoing objective has none, and an overdue review holds it at amber instead. Details are in [Overview](help:overview).

## Tips

- Link every project to an objective; projects with none are listed separately on the Overview.
- Use weights when contributions are not equal: a project that is 70% of the objective gets 0.7.
