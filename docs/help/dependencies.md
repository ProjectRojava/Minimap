# Dependency graph

The **Dependencies** screen (`g l`) draws what blocks what as a graph, left to right. (For the same links on a calendar, see [Gantt](help:gantt).) It is a place to **untangle what is holding what up**, to **check whether a project will finish on time**, and to **fix a link without leaving it**.

## Two levels

A toggle at the top chooses:

- **Tasks**: one box per task, with arrows for *blocks* links (from the task that must finish to the one that waits for it). *Part of* links (sub-tasks) are not drawn: they don't change the order of the work.
- **Projects**: one box per project, with arrows for *depends on* links (from the project that is needed to the one that depends on it).

## What a box says

- **Line 1**: the title. **Line 2**: the status, the owner or assignee, and when it is projected to finish (for a project, how many of its tasks are done). **Line 3**: what the lens is about (below).
- At the right: the owner's **initials** in their colour (the same colour as on the Tasks board) and a **P1** (solid amber) or **P2** (tinted) pill for urgent work. A thin **stripe** on the left is the colour of the objective the work serves, and a `≋ 19d` chip says a task has gone quiet (see [Fog](help:fog)).
- On the **Projects** level a box also says the **safe zone** (*10 days of room*) and the project's **target**, and the health shows in the Risk lens.
- Zoom out and the boxes say less: below 50% only the title, below 30% only the shape.

## Lenses: one question at a time

The **Lens** buttons choose what the graph adds to the picture, so colours don't pile up. **What is wrong with the work is marked in every lens**: **late** work has a red outline, **blocked** work (status *blocked*) an amber dashed one, and work that is **held up** (it waits, directly or further down the chain, on a blocked task) a red tint. The lens adds its own, and the legend at the bottom follows it.

| Lens | Question | Adds | Line 3 says |
|---|---|---|---|
| **Route** (the default) | Will it finish on time? | The **critical path** in the accent colour | *held up · slack 2d · est 3d*, *critical · no slack*, *1.5d over*; for a project, its room and target |
| **Risk** | What is stuck? | Arrows leaving blocked or late work are red | *late 2d · blocked · held up*, or *no problems*; for a project *paused*, *health red* |
| **People** | Who holds it up? | Each box takes its **owner's colour** (unless it is late, blocked or held up, which keep their marks) | *holds up 5 · waits on 2* |

Every mark has words on the box, so colour is never the only signal.

## Trace a chain

**Click a box.** The graph **traces** it: everything it waits on and everything it holds up stays lit, and the rest dims, and the box **opens in place** (below). Click the background or press `Esc` to clear the trace. **Double-click** a box to open it in the detail pane on the right. A hint along the bottom of the graph, and the tooltip of every box, say so.

Under the filters, a row of **insights** says what stands out (*Biggest bottleneck: Provision EU network and clusters holds up 9 · 4 links cross projects · 3 blocked · 2 late*); click one that names a box to trace it.

## Open a box for its details

**Click anywhere on a box** (when you are zoomed in enough to read it) and it **grows in place**; the rest of the graph moves aside to make room, and the view keeps the box where it was. Click an empty part of the open box, or its **⌃**, or press `Esc`, to close it again. Only **one box is open at a time**: opening another closes the first. A double-click opens the item in the detail pane instead of expanding it (the click waits a moment to see whether a second one follows).

An open task says:

- its full title, its status, priority, type, objective colours and owner;
- in words: *Holds up 5 tasks in 2 projects. Waits on nothing. Finishes Oct 16, due Oct 14: 2 working days late. On the critical path: any slip moves the finish.*, and why it is marked (late, blocked, held up);
- what it **waits on** and what it **holds up**, by name with their status: click a name to go to that box (it opens instead), or the **✕** beside it to remove that link;
- the start of its description;
- quick actions: a **status** (*To do*, *In progress*, *Blocked*, *Done*; finishing a decision task asks for its decision first, like anywhere else), **Add blocker…** and **Add task it blocks…** (then click the other box; `Esc` cancels), **Open**, **What if it slips?** (starts a [what-if](help:what-if) with it already in) and **Still accurate** (see [Fog](help:fog)).

An open project (on the Projects level) is simpler: its health, safe zone and progress in words, what it **needs** and what it is **needed by**, and the buttons **Needs…** and **Needed by…**, **Open** and **What if it slips?**.

## Arrows

A *+2d* on an arrow is the **lag** of a task link. Arrows that **cross projects** are dashed. On the Projects level an arrow says the **room** between the two projects (*3d room*, or *no room*): the working days between the end of the project that is needed and the start of the one that depends on it, which is also how far a slip in the first can go before it reaches the second. Point at an arrow to read what it means; click it to select it and a small card appears at its middle.

## Edit links here

- **Draw a link.** Point at a box: a round **handle** appears on its right edge. Drag it onto another box and let go. On the Tasks level the box you dragged from now **blocks** the other; on the Projects level the project you dragged from is **needed by** the other (it *depends on* the first). A link that would form a loop is refused with the path of the loop, like anywhere else.
- **Change a lag.** Click an arrow; in its card type the **lag** in working days (0 removes it).
- **Remove a link.** Select it and press **Delete**, or press **Remove link** in its card; or press the **✕** beside a name in an open box.

All of these are ordinary link changes, so they are logged, exported, synced and **undone with `Ctrl/Cmd+Z`** like the same change made in a detail pane.

## Unlinked work

Work with **no links** isn't drawn (it would bury the graph). A small **Unlinked (N)** button at the top right of the graph opens the list **on the right**; **✕** (or the button's return) hides it again.

The list shows each task or project **as a box**, like the ones in the graph: its title, status, owner, finish, priority, a stripe in its objective's colour and the line the lens says. To find one, **search** (every word must be in the name, the project, the owner or the status) and narrow the list with **status**, **project** and **owner**; the header says *Unlinked (3 of 16)* while it is narrowed. **Drag one onto the graph.** Drop it on empty space and it waits there as a dashed box under the graph, marked *Not linked yet* (the view moves to show it). Then **drag a second one beside it**: dropped to the **right** of the waiting box, the second comes **after** it (the waiting one blocks it); dropped to the **left**, it comes **before** (it blocks the waiting one). It is the same left-to-right order as everywhere else in the graph, and a line over the graph says so while you drag. The two are now linked and join the graph, and the next item you drop starts a new pair. Only one item waits at a time. Take it off again with the **✕** on its box or `Esc`; it also goes when you switch between *Tasks* and *Projects* or leave the screen.

To link something to a box that is already in the graph, **drag it onto that box** (it lights up, and the item blocks the box, or on the Projects level the box needs it), or drag from the box's handle to the waiting one, or click an item and then the box. A link that would make a loop is refused with the path, as always. **Double-click** an item to open it in the detail pane. `Esc` cancels a link you started. A new link is saved like any other (it is in the task's *Links* and shows on its card on the Tasks board, and `Ctrl/Cmd+Z` undoes it), and the item leaves the unlinked list at once and stays out of it, on this screen or after you come back to it. *Draw unlinked* draws them all in the graph instead, and the list goes away. *Show finished* adds finished work.

## Filters

In **My view** (`v`) the graph shows your tasks, what blocks them (up to four links back) and what they block, plus the usual dimmed context; the Projects level shows your projects the same way. See [Portfolio view and My view](help:views).

Narrow the graph by **project**, **team** (including its sub-teams) or **objective**; filters combine. Items outside the filter that are directly linked to what is shown appear **dimmed** as context, so you can see what a filtered group depends on or holds up. A graph is limited to 400 boxes; if the filter is too wide, Minimap asks you to narrow it.

## Moving around

**Drag** the background to pan; the **mouse wheel**, the **+ / −** buttons or the `+` and `-` keys zoom about the pointer; **Fit** shows everything.

Zoom out past the smallest size on the Tasks level and the graph becomes the **Projects** level, with the project of the tasks you were looking at centred and picked; zoom in past the largest size on a project and you are back on the Tasks level, filtered to it. See [Zoom](help:zoom).

## If it says there is a loop

*Blocks* and *depends on* links can't form a loop; if old data somehow does, the screen shows a message naming the problem rather than a picture. See [Links between items](help:links).

Related: [Schedule and critical path](help:schedule), [What if](help:what-if), [Approaching](help:approaching) (the same links, as a warning where you already are), [Projects](help:projects) (the safe zone).
