# 40 — About

Status: Implemented — awaiting manual check · Milestone: — · Priority: Could
Depends on: —

## Goal
Say who made Minimap, how to reach them and which version is running.

## Scope
**In** (built)
- A pinned sidebar entry **About** (`nav::NAV`, group `pinned`, icon `about`, chord `g u`) opening `/about` (`pages/about.rs`): a card with *Developer* (Uditt Lamba), *Email* (uditt.lamba@pm.me; a button that opens the mail program through `open_link` with `mailto:`, so it passes the same http/https/mailto check as every link) and *Latest version* (`env!("CARGO_PKG_VERSION")` of the workspace, so a release bump is the only change needed).
- Help: the welcome page's map and the keyboard page.

**Out**
- Checking online for a newer release (the app makes no network calls of its own), a changelog viewer, licence text.

## Acceptance criteria
- [x] The page's name, email, `mailto:` address and a three-number version (`ui::pages::about` tests); the entry is in the sidebar footer with an icon and a unique chord, and the Help lists it (`nav`, `icons` and `help` tests).
- [ ] Click-through (see below).

## Not yet verified by hand
- Sidebar footer: *About* sits between Settings and Help (or `g u`); the page shows the name, the email and the version that matches `Cargo.toml`
- Clicking the email opens the mail program with the address filled in
