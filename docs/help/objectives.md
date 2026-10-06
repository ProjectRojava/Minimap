# Objectives

An **objective** is an outcome you want, such as *Launch in the EU* or *Cut platform costs by 20%*. Projects and tasks **contribute** to objectives, which is how Minimap rolls health up from the work to the goal.

## The screen

Objectives are listed by priority (1 is highest), then target date, then title. A toggle groups them by **calendar quarter** of their target date, with a final *No date* group. Each row shows how many projects and tasks contribute.

## The detail pane

- **Your assessment**: status (*on track, at risk, off track, done*) and priority. This is your judgement. Minimap shows its own **computed health** next to it, so you can see when they disagree.
- **Target date** and a description.
- **Contributions**: the projects and tasks that contribute, each with a **weight** from 0 to 1. Add or remove them here. (You can also set it from the project's own pane.)
- **Health**: the computed health and why.

## How computed health works

The objective's health is a **weighted average** of what contributes to it, with one safeguard: *one red contributor caps the objective at amber*, so one disaster can't hide behind several fine projects. The objective's own target date is judged like a project's. Details are in [Overview](help:overview).

## Tips

- Link every project to an objective; projects with none are listed separately on the Overview.
- Use weights when contributions are not equal: a project that is 70% of the objective gets 0.7.
