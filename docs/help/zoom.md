# Zoom: from the whole portfolio to one task

A game's minimap scrolls out to the world map and back in to where you stand, and you never lose your place. Minimap's work is arranged the same way, in four **levels**, and two keys move you between them.

| Level | What you see | Where it is |
|---|---|---|
| **World** | Everything | [Overview](help:overview) |
| **Objective** | One objective and the projects that serve it | [Objectives](help:objectives), the objective open in the detail pane |
| **Project** | One project and its tasks | [Tasks](help:tasks) filtered to the project, the project open in the detail pane |
| **Item** | One task, note, decision, person… | Its detail pane, on the screen it is on |

## The keys

- `Alt+↑` **zooms out** one level from what you are looking at: from a task to its project, from the project to its objective, from the objective to the Overview.
- `Alt+↓` **zooms back in** to exactly where you came from: the same screen, the same open item and the same highlighted row. Press it as many times as you pressed `Alt+↑`.

Both also work from the command palette (*Zoom out*, *Zoom in*). In a text box the arrow keys keep their usual meaning and nothing zooms. On a row of the [Order](help:order) screen they move the task instead.

An example. You have *Gateway auth* open on the Tasks screen. `Alt+↑`: the board is filtered to *EU Region* and the project opens in the pane. `Alt+↑` again: Objectives, with *Launch in the EU* open. Again: the Overview. Now `Alt+↓` three times takes you back to *Gateway auth*.

If nothing is waiting to be returned to, `Alt+↓` zooms into the **highlighted row** instead: on the Projects screen, into that project's tasks; on the task board, into that task.

Going anywhere by another route (the sidebar, a `g` shortcut, a link) forgets the way back: `Alt+↓` then only zooms into the highlighted row.

## The path above you: the breadcrumb

Under the title of the Overview, of Objectives (when an objective is open), of Tasks (when it is filtered to a project) and at the top of every detail pane, a line shows where you are: **World › Launch in the EU › EU Region › Gateway auth**. Click any step to zoom there; `Alt+↓` then brings you back.

Some items skip a level. A task in the Inbox has no project, so it zooms out straight to its objective (if you linked it to one) or to the Overview. A project that serves no objective goes straight to the Overview. Notes, decisions and waiting-ons sit below the first objective, project or task they mention, affect or are about; people and teams sit directly under World.

### A project with several objectives

A project can serve more than one objective. Zooming out takes you to **one** of them: the one with the highest priority (1 first), then the heaviest *weight* on the link, then the earliest target date, then the title. A small **▾** after that objective in the breadcrumb lists the others; choose one to zoom there.

Archived objectives and projects are ignored.

## On the Dependencies screen

The wheel, `+` and `−` still zoom the picture. Past the ends of that zoom they change level:

- On the **Tasks** level, zoom out past the smallest size and the graph becomes the **Projects** level with the project of the tasks you were looking at centred and highlighted. A short note says so the first time.
- On the **Projects** level, zoom in past the largest size on a project (with the wheel over it, or `+` with it picked) and you are on the **Tasks** level filtered to that project.

See [Dependencies](help:dependencies).

## Good to know

- Zoom only moves your view. It never changes, hides or archives anything, and nothing is stored.
- The keys are listed with all the others in [Keyboard shortcuts](help:keyboard).
