//! Local backup and restore (spec 20).

use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};

/// Why a backup was made; part of its file name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupKind {
    /// "Back up now" or a named save point: `minimap-YYYYMMDD-HHMMSS.db`, or
    /// `minimap-YYYYMMDD-HHMMSS--before-q2-replan.db` with a name. Never deleted by Minimap.
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
    /// The name a manual backup was given, as the slug in its file name (`before-q2-replan`;
    /// the screen shows the dashes as spaces). `None` for every other backup.
    #[serde(default)]
    pub label: Option<String>,
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

// ------------------------------------------------------------------ save points (spec 51)

/// How often a Drive checkpoint was made: the newest 24 of a device are the hourly ones, the
/// older ones are the one-a-day copies kept after that.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointCadence {
    Hourly,
    Daily,
}

/// Where a save point is kept and what it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum SaveSource {
    /// A file in the backup folder on this computer.
    Local {
        path: String,
        file_name: String,
        kind: BackupKind,
        /// The name it was given (the slug), for a named save point.
        label: Option<String>,
        encrypted: bool,
    },
    /// An encrypted copy on Google Drive (spec 22).
    Drive {
        /// The checkpoint's file name on Drive, what `recover_checkpoint` takes.
        name: String,
        device_name: String,
        /// Made by this computer.
        this_device: bool,
        cadence: CheckpointCadence,
    },
}

/// One restorable copy of the data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavePoint {
    pub source: SaveSource,
    #[serde(with = "time::serde::rfc3339")]
    pub at: OffsetDateTime,
    pub bytes: u64,
    /// Minimap will delete it soon: an automatic backup beyond the newest `AUTO_BACKUPS_KEPT`, or
    /// a Drive checkpoint past the ones it keeps. Named, manual and safety copies never are.
    pub will_be_pruned: bool,
}

/// The save points of one calendar day (the user's own), newest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveDay {
    pub date: Date,
    pub points: Vec<SavePoint>,
}

/// Why there has been no save point for over a day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StaleReason {
    /// Automatic backups are off and Google Drive is not connected.
    AutoOff,
    /// Automatic backups are on, yet none was made in the last day.
    AutoFailing,
    /// Google Drive is connected, but there is no checkpoint and no local copy from the last day.
    DriveOffline,
}

/// Every restorable copy, newest first, grouped by day.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveTimeline {
    pub days: Vec<SaveDay>,
    /// The newest save point of any source.
    pub last: Option<SavePoint>,
    /// Set when the newest is over 24 hours old (or there is none).
    pub stale: Option<StaleReason>,
    /// Said when Google Drive's checkpoints could not be listed: only this computer's are shown.
    pub drive_note: Option<String>,
}

/// What the status bar needs: the newest save point and whether it is too old.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveSummary {
    pub last: Option<SavePoint>,
    /// Minutes since `last` (never negative).
    pub age_minutes: Option<u64>,
    pub stale: Option<StaleReason>,
}
