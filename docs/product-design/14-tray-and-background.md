# 14 — Tray and running in the background

Status: Implemented as spec 56 — awaiting manual check · Milestone: — · Priority: Should · Size: M
Depends on: spec 53 (the radar, which should outlive the main window), spec 55 (the splash, which a tray start skips), encryption (spec 21) and Drive sync (spec 22), which keep working while the window is hidden
Read first: [README](README.md), [10 — Radar window](10-radar-window.md), [13 — Splash screen](13-splash-screen.md)
Expected ADR: none

## Goal
Closing the window used to end the app: no sync, no reminders, no radar. Let Minimap keep running in the system tray when the person wants that, tell them from the icon what needs them, remind them of meetings, start with the computer, and lock itself when they have been away. Every one of these is a choice for **one computer**, with a calm default (ask first; nothing starts or locks by itself).

## Device settings, not synced settings
The choices live in `device-settings.json` in the data folder, read by the shell before the database opens (a start in the tray must not wait for it, and a locked database can't be read). They are never synced: a laptop and a desktop want different answers. `minimap_types::DeviceSettings` is the shape; `shell::read_settings` falls back to the defaults for a missing, damaged or out-of-range file. A second file, `device-state.json`, keeps what the shell remembers for itself: the user's UTC offset, the day the pings note was last sent, and whether the first-hide note was shown.

| Setting | Default |
|---|---|
| When I close the window | Ask me |
| Minimising hides the window to the tray | off |
| Start Minimap when this computer starts | off |
| …and stay in the tray | on |
| Remind me before a meeting, minutes before | on, 10 (1 to 120) |
| Tell me when pings are waiting | on |
| Lock after hidden for | 0 minutes = never (0 to 1440) |

## Phase 1: the tray
- **Close** (`shell::close_action`, `tray::on_window_event`): *Ask* shows the window and asks (`minimap://close-asked`; `ui/src/components/close_prompt.rs`; the answer comes back through `answer_close`, with *Remember my choice*); *Tray* hides the window and, the first time, says so in a notification; *Quit* closes. No tray, or Quit already chosen, always quits. While the page has not loaded there is no dialog, so *Ask* quits.
- **Tray icon** (`tray.rs`): left click opens the window; right click gives *Open Minimap*, *Quick add…* (shows the window and opens the palette: `minimap://quick-add`), *Show or hide the radar*, *Sync now* and *Quit Minimap*. Quit sets a flag and asks for exit, so the flush to Drive in `main.rs` runs. An exit that comes from the last window closing (the splash, say) is held back while the tray is up (`prevent_exit`).
- **The radar outlives the main window** when it is hidden: the radar's close-with-main rule only runs when the main window really closes.
- **Single instance** (`tauri-plugin-single-instance`, registered first): a second launch brings the first forward instead of opening the same database twice.
- **Minimise** goes through `minimize_main` so the shell can send it to the tray.

## Phase 2: working while hidden
- **Start with the computer** (`tauri-plugin-autostart`): registers `--autostart`. A start with that argument, with *stay in the tray* on and a tray present, opens no splash and no window (`shell::starts_hidden`; `app_ready` does not reveal; a tray click does). Only release builds touch the start-up list; the setting is applied again at every start so a moved program is the one that starts.
- **The watcher** (`watch.rs`, one thread, every 30 s): reads the database only when `radar::revision_of` moves or the day turns, then
  - sets the tray tooltip (*Minimap · 3 overdue · 2 pings waiting*) and a red dot on the tray icon and (Windows) the task bar button (`tray::show_attention`; the numbers are This week's overdue list and the due pings);
  - sends **meeting reminders** (`watch::meeting_reminders`): a To do meeting today whose start is within the lead time, once per meeting and start time (a moved meeting is announced again);
  - sends **one pings note a day** (`watch::ping_digest`), not while the main window is in view;
  - **locks** (below).
- **Time zone**: the backend can't know it, so `advance_meetings` (called by the window every 30 s) records the clock's UTC offset (`state::ClockNote`) and the watcher keeps the last one in `device-state.json`. Without one, meeting reminders wait.
- **Lock when hidden** (`security::lock_impl`): only for a database opened with a **passphrase**; the keychain would open it again with nothing typed. After the window has been hidden for the set time the watcher waits (10 s at most) for unsaved changes to reach Drive, drops the connection and key, forgets undo, closes the radar and sends `minimap://locked`; the page reloads into the unlock screen.

## Not in it
Global keyboard shortcut, notification actions (snooze, open the item), per-meeting reminder times, a "do not disturb" window, macOS menu-bar specifics.

## Tests
`shell.rs`: the close decision in every combination, hidden start, lock timing, the settings and notes files. `tray.rs`: badge painting, tooltip. `watch.rs`: time zone arithmetic, reminders (lead time, once, moved, past, other day), pings note. `security.rs`: only a passphrase vault locks and the passphrase opens it again. `minimap-types`: defaults, ranges, tolerant reading. UI: option values, minutes parsing, event names equal the shell's.

## Help wiki
`tray.md`; `settings.md` mentions the card.

## Not yet verified by hand
- Close the window with *Ask me*: the box appears; *Keep running in the tray* hides it and a notification says where it went; the tray icon opens it again.
- With *Remember my choice*, closing hides without asking; with *Quit Minimap* it closes and Drive gets the last changes.
- Start Minimap twice: the second brings the first forward.
- Tray menu: Quick add opens the palette; the radar item toggles the radar; Sync now syncs; Quit closes everything.
- A meeting 10 minutes away: one notification, not repeated; move it and it is announced again.
- An overdue task: the red dot and the tooltip.
- Start at login (installed build): the computer starts it in the tray with no splash.
- Passphrase database, lock after 1 minute, hide the window: it locks and asks for the passphrase on opening.
