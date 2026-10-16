# Tray, reminders and starting with your computer

Minimap can keep running when you close its window. It then lives in the **system tray** (the row of small icons by the clock, or the menu bar), where it keeps syncing, reminds you of meetings and shows on its icon when something needs you. Everything on this page is about **this computer only**: it is kept in a file next to the database and never synced, so your laptop and your desktop can behave differently.

## Closing the window

The first time you press the window's close button, Minimap asks:

- **Keep running in the tray** hides the window. Minimap is still running.
- **Quit Minimap** closes it for good.
- **Remember my choice** makes the close button do that from now on without asking.

Change your mind under *Settings → General → Window and tray → When I close the window*: *Ask me*, *Keep running in the tray* or *Quit Minimap*. The first time the window goes to the tray, a notification says where it went.

If the system has no tray Minimap can use, closing the window always closes Minimap, because there would be nothing to click to bring it back. (On Linux a tray needs an AppIndicator extension on some desktops.)

## The tray icon

- **Click** the icon to open the window.
- **Right-click** for the menu: *Open Minimap*, *Quick add…* (opens the window on the quick-add box), *Show or hide the radar*, *Sync now* and **Quit Minimap**.
- **Quit** is the way to close Minimap for good when closing the window hides it. Unsaved changes are sent to Google Drive on the way out.
- **Starting Minimap a second time** does not open a second copy; it brings the first one forward.

The radar stays on screen when the main window is hidden, which is what it is for. Quitting closes it too.

## What the icon tells you

The tooltip says what is waiting (*Minimap · 3 overdue · 2 pings waiting*), and a red dot appears on the icon (and, on Windows, on the task bar button) while anything is overdue or a ping has come due. It follows the same numbers as *This week*.

## Reminders

Two kinds of notification, both on by default and both about this computer:

- **Meetings**: a notification a few minutes before a meeting starts (10 by default; *Minutes before* sets 1 to 120). Each meeting is announced once; a meeting you move is announced again for its new time. Only meetings that have not started and are in your To do column count.
- **Pings**: one notification a day, the first time a ping has come due while the window is hidden or minimised. If you are looking at Minimap, it does not interrupt you.

Reminders need Minimap to be running, which is the point of the tray. They use the time zone your window last reported, so open Minimap once after you travel.

## Starting with your computer

*Start Minimap when this computer starts* adds Minimap to the start-up list. *…and stay in the tray* (on by default) then shows only the tray icon, with no start screen and no window, until you click it. Started by hand, Minimap opens its window as always. Only an installed Minimap registers itself; a development build leaves your start-up list alone.

## Minimising

*Minimising hides the window to the tray* (off by default) makes the minimise button hide the window to the tray instead of the task bar.

## Locking when you are away

If your database is encrypted with a **passphrase** (see [Encryption](help:encryption)), *Lock after hidden for* locks it again when the window has been hidden in the tray for that many minutes. Unsaved changes are sent to Drive first, the radar closes, and the next time you open the window it asks for your passphrase. 0 means never. With the keychain this option is off, since Minimap would open again without asking, so locking would protect nothing.
