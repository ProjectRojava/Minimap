# ADR-0002: Allow network access; add Google Drive backup

## Context
The original spec forbade network access by default. Users need off-device backups of their work, and Google Drive is the requested destination.

## Decision
- The "no network access by default" constraint is removed.
- Telemetry and analytics remain forbidden. User data only leaves the device to a destination the user has explicitly connected.
- Google Drive backup is added (M4):
  - Desktop OAuth with loopback redirect and PKCE; `drive.file` scope only, so the app sees only files it created.
  - Refresh token stored in the OS keychain via `keyring`, never in the database or logs.
  - Backups are encrypted before upload, so encryption must land before or with this feature.
  - All HTTP happens in the Rust backend; the webview CSP stays closed to the network.
- Local folder backup stays. A folder synced by Google Drive for Desktop also works with no extra code. The live `minimap.db` must never be placed in a synced folder.

## Consequences
- New dependencies: an HTTP/TLS client and an OAuth client.
- A Google Cloud project and OAuth client ID are required. Public distribution needs Google's consent-screen verification.
- Settings gains connect/disconnect, backup now, list and restore for Drive.
