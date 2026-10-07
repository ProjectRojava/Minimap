# Objectives

An **objective** is an outcome you want, such as *Launch in the EU* or *Cut platform costs by 20%*. Projects and tasks **contribute** to objectives, which is how Minimap rolls health up from the work to the goal.

## The screen

Objectives are listed by priority (1 is highest), then target date, then title. A toggle groups them by **calendar quarter** of their target date, with a final *No date* group. Each row shows how many projects and tasks contribute.

## Colours

Every objective has its own **colour**, shown as a dot and a coloured edge on its row. Minimap picks it for you, in the order the objectives were created, and keeps the colours far apart so two objectives never look alike. The same colour appears on the projects and tasks that serve the objective (see [Projects](help:projects) and [Tasks and the inbox](help:tasks)), so you can see what belongs together at a glance. If you archive an objective, the ones created after it each move up one colour.

## The detail pane

The pane opens with a row of coloured pills: your assessment (green on track, amber at risk, red off track), the priority (P1 and P2 in amber) and the target date (red "overdue" once it has passed while the objective is not done). The dropdowns for assessment and priority are coloured the same way, and the status of each contributing project or task is a coloured pill.

- **Your assessment**: status (*on track, at risk, off track, done*) and priority. This is your judgement. Minimap shows its own **computed health** next to it, so you can see when they disagree.
- **Target date** and a description.
- **Contributions**: the projects and tasks that contribute, each with a **weight** from 0 to 1. Add or remove them here. (You can also set it from the project's own pane.)
- **Health**: the computed health and why.
- **Notes and findings**: write a note about the objective (progress, reviews, decisions) and open the ones that mention it. They are ordinary [notes](help:notes).

## How computed health works

The objective's health is a **weighted average** of what contributes to it, with one safeguard: *one red contributor caps the objective at amber*, so one disaster can't hide behind several fine projects. The objective's own target date is judged like a project's. Details are in [Overview](help:overview).

## Tips

- Link every project to an objective; projects with none are listed separately on the Overview.
- Use weights when contributions are not equal: a project that is 70% of the objective gets 0.7.
