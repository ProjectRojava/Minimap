# Google Drive: backup, sync and attachments

Connecting Google Drive is **optional**. Without it Minimap works fully, but your data exists only on this computer, and the app says so (a *Local only* label in the status bar, a banner on [This week](help:this-week)). With it, **every change is saved to your own Drive automatically** and your other computers stay in step.

## Connect

Open *Settings → Data & backup → Google Drive* and press **Sign in with Google**. Your browser opens, you approve, and you come back.

- **First computer** (nothing on Drive yet): Minimap sets up the Drive folder and shows a **recovery key**. **Save it** in a password manager or on paper. It is what lets a new computer open your Drive data.
- **Another computer** (Drive already has data): see *Add another computer* below.

### Add another computer

Two computers share one Drive vault. To join a second one:

1. **On the computer that is already connected**, open *Settings → Data & backup → Google Drive*, find **Add another computer** and press **Show recovery key**, then **Copy**. (*Hide* puts it away again.) The key is only available on a computer that is connected.
2. **On the new computer**, install the same version of Minimap, open the same card and press **Sign in with Google**. Use **the same Google account**. If you use your own Google client under *Advanced: the Google OAuth client*, enter **the same client ID and secret** on both computers: Minimap can only see files created by its own client, so a different one won't find your data.
3. Minimap finds the data on Drive and asks for the **recovery key**. Paste it and press **Connect**. A wrong key is refused after a short wait and nothing changes.
4. A new computer with no data **adopts what is on Drive** (the screen reloads). One that already has data **merges** it, after making a safety backup.

If, instead of asking for a key, Minimap shows you a **new** recovery key, it found an empty Drive, so you signed in with a different account or client. Cancel and check both before saving anything.

## What is private about it

- Minimap asks Google for access to **only the files it creates** (the `drive.file` permission): it can't see the rest of your Drive.
- **Everything uploaded is encrypted** on your computer with a key only your computers hold (the recovery key is a copy of it), so Google can't read it.
- Sign-in uses your browser; Minimap never sees your Google password. The access token is kept in your system keychain.
- Nothing goes anywhere else: no accounts, no analytics.

## How it keeps computers in step

- Each computer keeps a **full local copy**, so it is fast and works offline.
- Each computer saves an encrypted **snapshot of its own** to Drive: about **5 seconds after you stop editing**, and never more than 30 seconds after the first change.
- Every **15 seconds** it looks for other computers' newer snapshots and **merges them automatically**, item by item: for each item the **newest edit wins**; something you deleted for good stays deleted. There are no conflict dialogs and no locks.
- After a merge the rules are re-checked and any repair is **reported** (for example two projects that ended up with the same handle get different ones, or a loop made by edits on two computers is broken). The *last merge* line in Settings says what was added, changed, deleted and repaired.
- If a computer is offline it keeps working and catches up when it is back; the status says how long it has been since the last save, and warns after a few days.

## The Settings card when connected

Status and *last saved*; **Sync now**; the list of **computers** using this Drive; the last merge; how much space Minimap uses; **checkpoints**; **Rename this computer**; **Disconnect…**. The status bar along the bottom shows the same state at a glance; click it to open the card.

## Checkpoints and recovery

Besides each computer's latest snapshot, Minimap keeps **hourly checkpoints** (the last 24) and **daily ones** (the last 30 days). **Recover…** on a checkpoint brings back whatever differs from that old copy **as new edits** (after saving a backup first); newer work stays. Use it if something went badly wrong on every computer.

## Attachments

You can attach files to any item: paste or drop them on a note, or use *Attach file…* in the detail pane. Allowed: **pictures** (including SVG), **text** (Markdown, TXT, CSV, JSON), **PDF**, **Word**, **Excel** and **PowerPoint**, up to **250 MB** each. Executables and scripts are refused.

Files are stored **encrypted**, once per distinct content, on Drive and in a local cache (2 GB by default; the least recently used are dropped, never one that isn't on Drive yet). A file attached on another computer is downloaded the first time you open it. Pictures show inline in notes. *Open* uses the program your system uses for that file type.

## Disconnecting

*Disconnect…* revokes Minimap's access and stops syncing. **Your data stays on this computer.** The files on Drive stay too (delete the *Minimap* folder in Drive if you want them gone).

## If something needs you

The card and a red banner say when Drive needs attention: you have to sign in again, the recovery key doesn't match, or Drive holds data from a newer version of Minimap (update the app). See [Questions and troubleshooting](help:faq).
