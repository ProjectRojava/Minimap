# Backup and restore

Backups are copies of your whole database that you can go back to. They protect you from mistakes and from losing the computer. (For off-site safety also consider [Google Drive](help:google-drive).) Find them under *Settings → Data & backup → Backup*.

## What is backed up, and when

| Kind | When it is made |
|---|---|
| **Manual** | When you press *Back up now* |
| **Daily** | Automatically: when the app starts and the last backup is over a day old, then daily while it runs. Only the **newest 14** automatic ones are kept |
| **Before upgrade** | Before the app changes the database to a newer version |
| **Before restore** | Before a restore replaces your data |
| **Safety copies** | Before turning encryption on or off, before a Drive sync that would delete a lot, and before recovering a Drive checkpoint |

Manual backups and safety copies are never deleted for you. The automatic backup can be switched off with its checkbox. Backups are made with SQLite's online backup, so they are consistent even while you work.

## Where they go

By default a `backups` folder inside the app's data folder. **Change…** picks another folder (an external drive, or a folder that Dropbox, OneDrive or Google Drive syncs, which gives you an off-site copy). **Use default** goes back. Keep the live database itself out of synced folders; only the backup folder should be there.

## The list

The card lists the backups in the folder with their kind, age and size. Only files Minimap made appear; nothing else in the folder is touched.

## Restoring

Press **Restore…** on a backup, or **Restore from file…** to pick one from elsewhere (for example from another computer).

1. The file is **checked**: it opens, passes SQLite's integrity check, is a Minimap database, and is not from a *newer* version of Minimap. If not, you are told why and nothing happens.
2. A panel shows **what it holds** (how many tasks, people, notes...) and says that your current data will be saved first.
3. On **Restore**: your current data is saved as a *before restore* backup, then the backup replaces your data (all or nothing), an older backup is upgraded to the current version, and the app reloads.

Restored the wrong one? The *before restore* backup is in the list: restore it to come back.

Your **backup folder and automatic-backup setting stay as they are** (otherwise a restore could quietly change where backups go).

## Encrypted databases

When your database is [encrypted](help:encryption), its backups are encrypted too (a plain copy of encrypted data is never written). A backup made under a **different** key (an older passphrase, say) asks for that passphrase or recovery key when you restore it. Backups made *before* you turned encryption on are plain; they are marked **unencrypted**, and *Delete them…* removes them so no unprotected copy of your data stays behind.

## Undo isn't backup

[Undo](help:undo) takes back recent changes in this session. Backups bring back anything, from any day.
