# The radar

A game's minimap is the small map in the corner of the screen: you don't play on it, you glance at it for half a second and look back at the game. Minimap's **radar** is the same idea. It is a small window, about 260 by 260 pixels, that can stay on top of your other apps, and it shows your world **centred on you**. Glance at it during a call, hover to see what a mark is, click to open the item.

The radar only **reads**. Nothing you do in it changes any of your data.

## Open it

- Press the **Radar** button in the title bar.
- Press `Ctrl/Cmd+Shift+M` (from anywhere, even a text box). Press it again to close it.
- In the command palette, *Open radar* and *Close radar*.
- In *Settings → General → Radar*, **Open with Minimap** shows it whenever Minimap starts.

The radar opens where you left it last time, at the size you left it, on a screen that is still there (if that monitor has been unplugged it opens in the corner of the main one). The first time it opens in the bottom-right corner. It has no title bar: **drag** its top bar, or any empty part of the picture, to move it, and drag its edges to resize it. Closing Minimap's main window closes the radar too.

## How to read it

The middle is **you**, drawn as a small triangle. The further out something is, the further off it is.

| What you see | What it means |
|---|---|
| **Rings** | Time, counted in working days from today: the inner ring is this week (**week**), the middle one the next two weeks (**2 wks**), the outer one this quarter (**qtr**). Hover a mark for its real date |
| **Overdue work** | Close to the middle, and closer the later it is (ten days late or more sits in the centre) |
| **Wedges** | One per objective that is still open, in that objective's colour on the outer edge (the same colour as everywhere else in Minimap), ordered by priority and then name. A busier wedge is wider. An **Other** wedge, with no colour, holds what serves no objective |
| **Blips** | The items that matter, placed by how soon they are due and which objective they serve. A blip that serves two objectives is drawn once, in the higher-priority one |
| **Edge marks** | Trouble approaching your work, on the rim (below) |

### The blips

The shape says how the item is doing, so you do not need to tell the colours apart.

| Shape | Means |
|---|---|
| Filled circle (red) | Overdue |
| Half-filled circle (amber) | Blocked, or a project at risk |
| Empty circle (green) | On track |
| A dash | No health to judge (for example a project with nothing planned) |
| Small hollow square | A meeting today or tomorrow |
| A pin | A [ping](help:pings) that has come due on something that is not otherwise on the radar |
| A thin ring round a blip | It is a project, not a task |
| A small dot on the corner | The item has a ping |
| Faded, with a dashed outline | Nobody has touched it for a while: [fog](help:fog) |

A bigger blip means a higher priority: priority 1 is large, 2 is medium, the rest are small.

What gets a blip:

- open tasks (not meetings) that are overdue, due within the outer ring, blocked, or [in focus](help:tasks);
- active projects whose target is within the outer ring, or whose [safe zone](help:projects) is narrow or already past the target;
- today's and tomorrow's [meetings](help:meetings);
- [pings](help:pings) that are due.

At most 60 blips are drawn. If there are more, the most urgent stay (overdue first, then by date, then priority) and the bottom line says how many are not shown. A blip stays where it is between refreshes: its angle comes from the item, not from chance.

### Edge marks

A short coloured arc on the rim with a chevron pointing out means something outside your work is on its way to it, as in [Approaching](help:approaching). It sits in the wedge of the objective it will reach. **Red** means the work it reaches is already late and this is what holds it up; **amber** means that work has little room left. A mark pulses twice when it first appears and then stays still (not at all if your computer is set to reduce motion). Hover it for the sentence, like *Gateway auth (EU Region) is blocked; reaches Pen test in 3 days*. Click it to open the thing in trouble. Items that only deserve a look (the "also watching" ones) are not drawn.

## Hover, click, keys

- **Hover** a blip or mark for what it is: *Gateway auth · EU Region · blocked · due Fri*. Aim within a few pixels; the radar forgives a near miss.
- **Click** one and the main Minimap window comes forward (even if it was minimised) with that item open in the detail pane.
- `Tab` and `Shift+Tab` walk the blips clockwise from 12 o'clock; `Enter` opens the one you are on; `v` switches the view; `Esc` closes the radar.

## The top bar and the bottom line

The top bar has three buttons:

- **Portfolio or My view**: the arrow is [Portfolio view](help:views) (everything), the marker is My view (what touches you). Click it, or press `v`. It is the same choice as the button in the main window, so a switch in one is followed by the other. In My view only the objectives, projects and tasks that touch you are shown. An item whose objective is not part of your view goes in **Other**. Without a "you" in [People](help:people-teams), the radar shows the Portfolio view.
- **The pin**: keeps the radar on top of other windows, or lets other windows cover it. The setting is remembered.
- **✕**: closes the radar.

The line at the bottom is a summary, for example *2 overdue · 1 ping today · visibility 64%*: tasks past their due date, tasks due today, pings that are due, items approaching your work, and your [fog](help:fog) visibility. When there is nothing to say it reads *Nothing in range this week*. If Minimap is locked (an [encrypted](help:encryption) database waiting for its passphrase) the line says so; unlock it in the main window.

## Keeping up to date

The radar asks Minimap every few seconds whether anything changed, and redraws within about five seconds of a change you make in the main window (a new due date, a task finished, a ping cleared). It also redraws once a minute whatever happens, because dates move at midnight and meetings come round.

## Settings

*Settings → General → Radar*, for this computer only:

- **Keep on top of other windows** (on by default).
- **Open with Minimap** (off by default).
- **Rings** (default 5, 10 and 60 working days): how far out each ring is. Each must be farther out than the one before, from 1 to 260. A change shows within a minute.

## Good to know

- The radar is for glancing. To change anything, open the item in the main window.
- Where the window sits and its size are remembered on this computer only; they are not part of your data and are never synced or exported.
- Dates and rings use your [working days](help:settings). On a day off, "today" counts as the next working day.
