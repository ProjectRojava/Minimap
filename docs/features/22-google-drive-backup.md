# 22 — Google Drive backup

Status: Draft · Milestone: M4 · Priority: Must
Depends on: 20, 21

## Goal
Off-device backups to the user's Google Drive. See ADR-0002.

## Scope
**In**
- Connect Google account from Settings: desktop OAuth with loopback redirect + PKCE, scope `drive.file` only.
- Refresh token stored in the OS keychain; disconnect revokes it and deletes it locally.
- Upload: backup (20) is encrypted (21 or a separate backup key) and uploaded to an app folder "Minimap Backups".
- Auto-upload after each automatic backup (setting), and "Back up to Drive now".
- List Drive backups (date, size); restore one (download → validate → same flow as 20).
- Retention: keep last N on Drive (default 14).
- Status in Settings: connected account, last upload, last error.
- All HTTP in Rust (`reqwest` + `oauth2` or equivalent); the webview CSP stays closed.

## Rules
- Uploads are resumable or retried; a failed upload never deletes older backups.
- Offline: queue silently, show "last upload N days ago" warning after 3 days.

## Prerequisites (you)
- Google Cloud project, OAuth client ID (Desktop app type).
- Consent-screen verification before public distribution.

## Acceptance criteria
- [ ] Connect, upload, list, restore and disconnect all work end to end.
- [ ] Uploaded file is not readable without the key.
- [ ] Retention deletes only app-created backups beyond N.

## Open questions
- Encryption key for Drive backups: same as DB key, or a separate backup passphrase (better if the device is lost)?
- Should the encryption feature (21) be Must since this depends on it?
