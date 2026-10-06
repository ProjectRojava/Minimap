# Encryption

Encryption locks your database file so that nobody can read it without the key, even if they copy it. It is **off by default**. Turn it on in *Settings → Security → Encryption*.

## Two ways to hold the key

| Method | How it works | Good for |
|---|---|---|
| **System keychain** | Minimap makes a random key and keeps it in your computer's keychain. The app opens without asking | Convenience: protects the file if someone copies it, not if they use your logged-in computer |
| **Passphrase** | You choose a passphrase (at least **12 characters**). Minimap asks for it each time it starts and **never stores it** | Strongest: needs your passphrase every time |

If your computer has no usable keychain, only the passphrase method is offered; Minimap never keeps a key in a place that offers no protection.

## The recovery key

With the keychain method, Minimap shows a **recovery key** exactly **once**, when you turn encryption on (or ask for a new one). **Save it somewhere safe** (a password manager, a printout). It opens your data if the keychain entry is ever lost: after reinstalling the system, or on a new computer. The panel won't close until you tick *I saved it*.

> **Warning:** a forgotten passphrase can't be recovered. There is no reset and no back door: that is what encryption means. Keep your passphrase or recovery key safe.

## Turning it on, changing and off

Under *Security → Encryption*, the buttons depend on the current state: **turn on** (with keychain or passphrase), **new recovery key**, **change passphrase**, **switch method**, **turn off**.

Every change is a **verified swap**, never an in-place edit: first a *before encryption* backup is made, then an encrypted (or decrypted) copy is built and checked against the original (every table and row, integrity, version), then it replaces the database. If anything fails, nothing changes and your original is kept. Turning it off asks for confirmation (and the passphrase, if that is your method). Your data is identical afterwards.

## Starting an encrypted database

If the key is in the keychain the app just opens. Otherwise it shows an **unlock screen**: type your **passphrase or your recovery key**. A wrong key makes you wait a moment before trying again, which keeps guessing slow. Nothing else works until it is unlocked.

## What it protects

- **Protected**: the database file at rest, and its [backups](help:backup-restore), and what [Google Drive](help:google-drive) holds (Drive snapshots are always encrypted with their own key, whether or not you turn this on).
- **Not protected**: what you see while the app is open and unlocked, anything you [export](help:export) (plain text), and files you copy out of the app.
- Notes and other content are never written to the log file.

## Good to know

- Old plain backups remain readable by anyone; delete them from the Backup card once encryption is on.
- Encrypting loses nothing, and the app works exactly the same afterwards.
