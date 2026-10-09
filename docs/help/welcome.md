# Welcome to Minimap

Minimap is a private command center for running a team or an organisation. It keeps your **objectives, projects, tasks, people, teams, notes, decisions and the things you are waiting on** in one place, linked together, so it can answer the questions you actually have: *what is late, who is overloaded, what will a slip knock over, and what do I need to chase this week?*

## What to know first

- **Your data stays with you.** Everything is in one file on this computer. There is no account, no login and no telemetry. Connecting Google Drive is optional, and then your data goes only to *your* Drive, encrypted. See [Google Drive](help:google-drive).
- **One user.** People in Minimap are records (a name, a role, a weekly capacity), not accounts. Only you use the app.
- **It works things out for you.** Dates, the critical path, project health, workload and the effect of a slip are all computed from your tasks and the links between them. You rarely have to maintain a status by hand.
- **Keyboard first.** Almost everything has a shortcut, and `Ctrl/Cmd+K` opens a command palette that can add things from one line of text. See [Keyboard shortcuts](help:keyboard) and [Quick-add and the palette](help:quick-add).

## Your first ten minutes

1. **Say who you are.** On the first launch Minimap asks for your name. You become the person marked *you* in People.
2. **Add the people you work with** on the [People and teams](help:people-teams) screen (*New person*). Quick-add can also create a person the first time you mention one.
3. **Add a project** on [Projects](help:projects) with *New project*. Give it an owner and a target date if you have one.
4. **Add tasks.** Press `Ctrl/Cmd+K` and type, for example:

```text
task Write the launch plan @me #my-project due:fri est:3d
```

   Press `Enter` to see a preview and `Enter` again to add it. Tasks without a project go to the *Inbox*.
5. **Say what depends on what.** Open a task and, under *Links*, add a *blocks* link to the task that has to wait for it. See [Links between items](help:links).
6. **Look around.** [This week](help:this-week) shows what needs you now. [Overview](help:overview) shows health across everything. [Schedule](help:schedule) shows when things will really finish.
7. **Protect your data.** Turn on [automatic backups](help:backup-restore), consider [encryption](help:encryption), and optionally connect [Google Drive](help:google-drive).

> **Tip:** the quickest way to see everything working is the demo data. In a development build, *Settings → Developer → Add demo data* fills an empty database with a realistic company. It is not offered in the released app. To get rid of it again, see [the FAQ](help:faq).

## The screens at a glance

| Group | Screen | What it is for |
|---|---|---|
| | [This week](help:this-week) | Your landing screen: overdue, due, blocked, the tasks you keep in focus, in progress, waiting, 1:1s |
| | [Overview](help:overview) | Health of objectives and projects, top risks, overloaded people |
| | Inbox | Tasks that belong to no project (the Tasks screen, filtered) |
| Plan | [Objectives](help:objectives) | The outcomes you are after, and what contributes to them |
| Plan | [Projects](help:projects) | Work with an owner, a target date and tasks; list and board |
| Plan | [Tasks](help:tasks) | Every task, as a drag-and-drop board of status columns or a list, with filters |
| Plan | [Dependencies](help:dependencies) | A picture of what blocks what, with the critical path |
| Plan | [What if](help:what-if) | See what a slip of a few days would push back |
| People | [People](help:people-teams) and Teams | Who is who, and the teams and reporting lines |
| People | [Capacity](help:capacity) | A heatmap of who is loaded when |
| Log | [Notes](help:notes) | Meeting notes and 1:1s, with `@` mentions and checklists |
| Log | [Decisions](help:decisions) | What was decided, why, and what replaced it |
| Log | [Waiting on](help:waiting-on) | What you are waiting for from others, and how long |
| Review | [Weekly review](help:weekly-review) | A guided look back, ending in a status report |
| | [Settings](help:settings) | Appearance, working time, thresholds, backups, encryption, Drive |
| | About | Who made Minimap (name and email, click the email to write to them) and the version you are running. In the sidebar footer, or `g u` |

## Using this help

Pick a page on the left, or search all pages at the top. Words in a link like [this one](help:concepts) open another help page. Press `?` anywhere (outside a text box) or `F1` to open the help, and `g h` to go to it from the keyboard.

New to the ideas? Read [How Minimap thinks](help:concepts) next.
