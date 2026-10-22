# Backup and restore

Backups are copies of your whole database that you can go back to: **save points**. They protect you from mistakes and from losing the computer. (For off-site safety also consider [Google Drive](help:google-drive).) Find them all under *Settings → Data & backup → Save points*; where they go and the daily switch are under *Backup* just below it.

## What is backed up, and when

| Kind | When it is made |
|---|---|
| **Named** (or **Manual**) | When you make a save point with `Ctrl/Cmd+S` (or press *Back up now*), with a name if you give one |
| **Daily** | Automatically: when the app starts and the last backup is over a day old, then daily while it runs. Only the **newest 14** automatic ones are kept |
| **Before upgrade** | Before the app changes the database to a newer version |
| **Before restore** | Before a restore replaces your data |
| **Safety copies** | Before turning encryption on or off, before a Drive sync that would delete a lot, and before recovering a Drive checkpoint |

Named and manual save points and safety copies are never deleted for you. The automatic backup can be switched off with its checkbox. Backups are made with SQLite's online backup, so they are consistent even while you work.

## Where they go

By default a `backups` folder inside the app's data folder. **Change…** picks another folder (an external drive, or a folder that Dropbox, OneDrive or Google Drive syncs, which gives you an off-site copy). **Use default** goes back. Keep the live database itself out of synced folders; only the backup folder should be there.

## Save points

The **Save points** card lists every copy you can go back to on **one timeline**, newest first and grouped by day on your own clock: your named and manual save points, the daily backups, the safety copies, and, with [Google Drive](help:google-drive) connected, its hourly and daily **checkpoints** (marked *Drive hourly* or *Drive daily* with the computer that made them). Each row has its time, what kind it is, its name if it has one, its size and **Restore…**. A daily backup that will be deleted soon (only the newest 14 are kept) says *will be removed soon*; so do Drive checkpoints that are about to age out. If Google Drive can't be reached in a few seconds, the list shows this computer's save points and says so.

**Make one with `Ctrl/Cmd+S`** (from any screen, even inside a text box), *Make a save point…* here or in the command palette, or *Back up now*. A small box asks for a name (optional, up to 80 characters), for example *before the Q2 replan*. Press `Enter`: a toast says *Save point made: before the Q2 replan.* The name is kept in the file's name as plain lower-case words (`minimap-20270303-140200--before-the-q2-replan.db`); a name with no English letters or digits is saved as *save*. Only files Minimap made appear; nothing else in the folder is touched.

**The status bar** along the bottom shows your newest save point as a small disk inside a ring. The ring is full when the save point is new and empties over two days; hover the disk to read *Last save point 2 h ago*. If there has been none for over a day, the disk turns amber and words appear next to it that say why: *Automatic backups are off*, *Automatic backups haven't run in a day*, or *Google Drive hasn't saved a checkpoint in a day*. The same words show when there has never been a save point (*No save point yet*). Click the disk for a small panel with the details, **Make a save point…** and **See save points**, which opens the timeline. To see the words all the time, tick *Words in the status bar* in [Settings](help:settings), Appearance.

## Restoring

Press **Restore…** on a save point, or **Restore from file…** to pick one from elsewhere (for example from another computer).

1. The file is **checked**: it opens, passes SQLite's integrity check, is a Minimap database, and is not from a *newer* version of Minimap. If not, you are told why and nothing happens.
2. A panel shows **what it holds** (how many tasks, people, notes...) and says that your current data will be saved first.
3. On **Restore**: your current data is saved as a *before restore* backup, then the backup replaces your data (all or nothing), an older backup is upgraded to the current version, and the app reloads.

Restored the wrong one? The *before restore* save point is in the list: restore it to come back. A **Drive checkpoint** is recovered differently, as described in [Google Drive](help:google-drive): items that differ come back as new edits and newer work stays.

Your **backup folder and automatic-backup setting stay as they are** (otherwise a restore could quietly change where backups go).

## Encrypted databases

When your database is [encrypted](help:encryption), its backups are encrypted too (a plain copy of encrypted data is never written). A backup made under a **different** key (an older passphrase, say) asks for that passphrase or recovery key when you restore it. Backups made *before* you turned encryption on are plain; they are marked **unencrypted**, and *Delete them…* removes them so no unprotected copy of your data stays behind.

## Undo isn't a save point

[Undo](help:undo) takes back recent changes in this session, one change at a time. Save points bring back everything as it was at a moment, from any day, even after you close the app.
