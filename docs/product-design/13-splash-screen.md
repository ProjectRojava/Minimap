# 13 — Splash screen: what the window shows while Minimap starts

Status: Implemented as spec 55 — awaiting manual check · Milestone: — · Priority: Could · Size: S
Depends on: spec 54 (the mark and the voice) · Read first: [README](README.md), [11 — Brand and voice](11-brand-and-voice.md)
Expected ADR: none

## Goal
The window used to open as an empty dark rectangle until the app had been downloaded and started (several seconds in a development build, a moment in an installed one). Show the mark instead, in the product's calm voice, and get out of the way as soon as the app is there. No game art, no progress bars that lie, no sounds (README principle 5).

## Its own small window (not the whole screen)
The main window opens maximized, so a splash painted inside it covered the whole screen. The splash is now **a small window of its own** (440 × 340, centred, no frame, always on top, off the taskbar: `ui/splash.html`, opened first thing in `setup` by `src-tauri/src/commands/splash.rs`). The main window is configured `"visible": false` and stays hidden while the app downloads and starts. When the app has mounted it calls `app_ready`; the shell then shows the main window and closes the splash, but not before the splash has been up five seconds (`MIN_SHOW`, held equal to the page's `MIN_SHOW_MS` by a test). A failsafe shows the main window after thirty seconds whether or not the app said it was ready, so a start that fails shows its own error rather than a splash that never goes. If the splash window can't be opened the main window is shown at once. The radar window is small already and keeps the overlay in `index.html` (0.7 s); the main window has that overlay too, but behind its hidden window, so it is removed at once. The two pages (`index.html`, `splash.html`) carry the same mark, words and version, and tests check both.

## Two looks: dark and light
The splash window follows **the app's theme**, with a separate design for each kind of theme. The theme lives in the database, which isn't open when the splash must appear, so the app tells the shell which kind it is drawing every time it applies a theme (`remember_splash_scheme`: `dark` or `light` by the theme's kind, `system` when the app follows the system), and the shell keeps that in a one-word file, `splash-scheme`, in the app's data folder. At the next start the shell reads it before anything else and opens `splash.html#force-dark` or `#force-light` (CSS `:target` picks the look, so the page still has no script); `splash.html` alone, for the first start or for the *System* theme, follows the system's colour scheme. The window's own background colour is set the same way, so nothing flashes. Changing the theme in Settings therefore shows in the splash of the next start.
- **Dark, the night map**: navy gradient (`#16293F` to `#0B1524`), a fine grid, a teal glow behind the mark, a small network of teal nodes and links down each side (they breathe softly), the four corners of a viewport in teal with its faint dashed outline, and a fog vignette at the edges, like the logo. The mark is `minimap-logo-dark.svg`, with a teal glow.
- **Light, the paper map**: cream gradient (`#FBF8F0` to `#E9E3D4`), a navy-ink grid, the same network and viewport corners in deep teal (`#17877A`), a soft teal wash, no fog (the light logo has none). The mark is `minimap-logo-light.svg`, with a soft shadow. Words in navy.
The words and the version are the same in both. Under reduced motion nothing moves in either.

## What it is
- **Plain HTML and CSS in `ui/index.html`**: no script, so it paints the instant the page does, and the content security policy (no inline script) is respected. The mark is inline SVG: the dark logo (option A1) at up to 112 px (never more than 42% of the window's short side, so it fits the 260 px radar window too).
- **Always the dark default.** It uses the theme tokens with the default dark values as fallbacks (`#1e1f21`), which is also the window's own background colour, so nothing flashes. The chosen theme is not known until the app has started.
- **Motion, once and short**: the viewport box draws itself in (0.7 s), then the "you" node pulses twice. The line under the name fades in after 0.7 s: *Starting up*. If the start takes longer than 9 s it becomes *Still starting. This can take a moment after an update.* Under `prefers-reduced-motion` nothing moves and the lines just appear.
- **Words** follow [`voice.md`](../voice.md): true first, then what to do, calm, no exclamation marks.
- **The version and who made it** sit at the bottom: *Version 0.4.0* and *Uditt Lamba · uditt.lamba@pm.me*, small and quiet. They are typed in `ui/index.html` (the page has no script to fill them in while the app is still loading), and a test fails when they differ from the About page's `VERSION`, `DEVELOPER` and `EMAIL`, so a release bump has to update both.
- **It stays five seconds, or until the app is ready, whichever is later**: `ui/src/splash.rs` (`dismiss`, called after `mount_to_body`, which is when the app is ready) waits until the page has been open for at least 5 s (`MIN_SHOW_MS`), so a fast start still shows the mark, the version and the author for the full five; a slower start leaves as soon as the app is there. Then it fades out (0.22 s) and removes itself.
- **The radar is the exception**: its splash only holds for 0.7 s (`RADAR_MIN_SHOW_MS`), because the radar is a window you glance at and five seconds would make it useless. It still shows the mark until it is ready.

## Not in it
A progress bar (the start has no measurable steps), a tagline, tips, or anything that needs the database (it has not been opened yet and may be locked).

## Tests
`ui/src/splash.rs`: the page has the element and the fade rule the module relies on; the markup contains no script or inline handlers; reduced motion is handled; the words are the agreed ones; the minimum time is applied.

## Help wiki
`faq.md`: *Minimap shows its start screen for a long time*.

## Not yet verified by hand
- Start the installed app: the mark appears at once, then the Overview/This week without a blank frame in between.
- Start `cargo tauri dev` (slow): *Still starting* shows after 9 s and the splash leaves when the app is ready.
- The radar window shows the splash small and tidy, then the radar.
- With the system set to reduce motion: no drawing, no pulse.
