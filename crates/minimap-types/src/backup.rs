//! Local backup and restore (spec 20).

use serde::{Deserialize, Serialize};

/// Why a backup was made; part of its file name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupKind {
    /// "Back up now": `minimap-YYYYMMDD-HHMMSS.db`. Never deleted by Minimap.
    Manual,
    /// The daily one: `minimap-auto-YYYYMMDD-HHMMSS.db`. Only the newest 14 are kept.
    Auto,
    /// Taken before a schema upgrade: `minimap-pre-migration-vN-YYYYMMDD-HHMMSS.db`.
    PreMigration,
    /// The live data as it was just before a restore: `minimap-pre-restore-YYYYMMDD-HHMMSS.db`.
    PreRestore,
    /// Taken before encryption is turned on, off or re-keyed, and removed once that succeeded:
    /// `minimap-pre-encryption-YYYYMMDD-HHMMSS.db`.
    PreEncryption,
    /// Taken before another device's data is first merged in, or before a merge that removes
    /// many items (spec 22): `minimap-pre-sync-YYYYMMDD-HHMMSS.db`. Never deleted by Minimap.
    PreSync,
    /// Taken before items are recovered from a Drive checkpoint: `minimap-pre-recover-…`.
    PreRecover,
}

/// How many automatic backups are kept.
pub const AUTO_BACKUPS_KEPT: u32 = 14;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupEntry {
    pub file_name: String,
    /// Full path, to hand back to `preview_restore`.
    pub path: String,
    pub kind: BackupKind,
    /// "2027-03-03 15:30 UTC".
    pub created: String,
    /// Minutes since it was made (never negative).
    pub age_minutes: u64,
    pub bytes: u64,
    /// The file is encrypted (it does not start with the plain SQLite header).
    pub encrypted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupStatus {
    /// Where backups go now: the chosen folder, or the default one inside the app's data folder.
    pub folder: String,
    pub is_default_folder: bool,
    pub default_folder: String,
    pub auto_backup: bool,
    /// The live database is encrypted (so backups without encryption stand out).
    pub database_encrypted: bool,
    pub keep_auto: u32,
    /// The newest manual or automatic backup.
    pub last_backup: Option<BackupEntry>,
    /// Everything in the folder that Minimap made, newest first.
    pub backups: Vec<BackupEntry>,
}

/// One line of what a backup holds ("Tasks", 41).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupCount {
    pub label: String,
    pub count: u32,
}

/// What restoring a file would do, shown before the user confirms.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestorePreview {
    pub path: String,
    pub file_name: String,
    pub bytes: u64,
    pub schema_version: u32,
    pub current_schema_version: u32,
    /// The backup is encrypted.
    pub encrypted: bool,
    /// The file is from an older version and is upgraded after restoring.
    pub will_upgrade: bool,
    /// Active items in the backup, per kind.
    pub counts: Vec<BackupCount>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestoreResult {
    /// The copy of the data that was live before the restore.
    pub saved_current_as: BackupEntry,
    pub restored_from: String,
}
