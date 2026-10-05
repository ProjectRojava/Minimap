//! Google Drive storage, multi-device sync and attachments (spec 22).

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{NodeRef, NodeType, Secret};

/// Largest attachment accepted, in bytes (250 MB).
pub const MAX_ATTACHMENT_BYTES: u64 = 250 * 1024 * 1024;
/// Default size of the local cache of attachment files, in bytes (2 GB).
pub const DEFAULT_MEDIA_CACHE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// Hourly checkpoints kept on Drive.
pub const HOURLY_CHECKPOINTS_KEPT: u32 = 24;
/// Daily checkpoints kept on Drive (beyond the hourly ones).
pub const DAILY_CHECKPOINTS_KEPT: u32 = 30;

// ------------------------------------------------------------------ attachments

/// Every file extension that can be attached (the file picker offers exactly these; a test in
/// `minimap-core` keeps this list and the kind table in step).
pub const ATTACHMENT_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "avif", "heic", "tif", "tiff", "svg", "md",
    "markdown", "txt", "csv", "tsv", "json", "pdf", "doc", "docx", "dot", "dotx", "odt", "rtf",
    "xls", "xlsx", "xlsm", "ods", "ppt", "pptx", "odp", "key",
];

/// What an attached file is, by its extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentKind {
    /// png, jpg, gif, webp, bmp, heic, avif, tiff (shown as a thumbnail).
    Image,
    /// svg: shown as an image, never run.
    Svg,
    /// md, markdown, txt, csv, tsv, json.
    Text,
    Pdf,
    /// doc, docx, dot, dotx, odt, rtf.
    Word,
    /// xls, xlsx, xlsm, ods.
    Excel,
    /// ppt, pptx, odp, key.
    PowerPoint,
}

impl AttachmentKind {
    pub fn label(self) -> &'static str {
        match self {
            AttachmentKind::Image => "Image",
            AttachmentKind::Svg => "SVG",
            AttachmentKind::Text => "Text",
            AttachmentKind::Pdf => "PDF",
            AttachmentKind::Word => "Word",
            AttachmentKind::Excel => "Excel",
            AttachmentKind::PowerPoint => "PowerPoint",
        }
    }

    /// Shown as a picture (thumbnail, inline in notes).
    pub fn is_picture(self) -> bool {
        matches!(self, AttachmentKind::Image | AttachmentKind::Svg)
    }
}

/// Where the bytes of an attachment are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentState {
    /// Stored on this device only (Drive isn't connected).
    LocalOnly,
    /// On this device, waiting to be uploaded to Drive.
    Waiting,
    /// On Drive and on this device.
    OnDrive,
    /// On Drive but not on this device; fetched when opened.
    NotDownloaded,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attachment {
    pub id: Uuid,
    pub node_type: NodeType,
    pub node_id: Uuid,
    pub file_name: String,
    pub mime_type: String,
    pub kind: AttachmentKind,
    pub size_bytes: u64,
    /// Hex SHA-256 of the file's content (what Drive stores it under).
    pub sha256: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub archived_at: Option<OffsetDateTime>,
    pub state: AttachmentState,
    /// Text to put in a note to show it: `![name](attachment:<id>)` for pictures, a link otherwise.
    pub markdown: String,
}

/// Attach the file at `path` (picked in the file dialog) to a node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddAttachment {
    pub node: NodeRef,
    pub path: String,
}

// ------------------------------------------------------------------ sync status

/// The headline state shown in the status bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncState {
    /// Drive isn't connected: the data exists on this device only.
    LocalOnly,
    /// Connected and everything is on Drive.
    Saved,
    /// Changes are being uploaded.
    Saving,
    /// Another device's changes are being merged.
    Syncing,
    /// Connected, but Drive can't be reached; changes are waiting.
    Offline,
    /// Drive refused or something failed; see `last_error`.
    Error,
    /// Waiting for the user (sign-in expired, recovery key needed).
    NeedsAttention,
}

/// What one merge of another device's snapshot did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeSummary {
    /// The device it came from.
    pub from_device: String,
    /// "2027-03-03 15:30 UTC".
    pub at: String,
    /// Items (rows) that were new here.
    pub added: u32,
    /// Items replaced by a newer version.
    pub updated: u32,
    /// Items removed because the other device deleted them.
    pub deleted: u32,
    /// Things the merge had to fix so the rules still hold, each as a sentence.
    pub repairs: Vec<String>,
}

impl MergeSummary {
    pub fn changed_anything(&self) -> bool {
        self.added + self.updated + self.deleted > 0 || !self.repairs.is_empty()
    }
}

/// Another device that saves to the same Drive folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub device_id: String,
    pub name: String,
    /// "2027-03-03 15:30 UTC".
    pub last_saved: String,
    pub is_this_device: bool,
}

/// A saved copy of the data on Drive that can be recovered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointInfo {
    pub name: String,
    pub device_name: String,
    /// "2027-03-03 15:30 UTC".
    pub created: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncStatus {
    pub state: SyncState,
    /// One line for the status bar ("Saved to Drive · 12 s ago").
    pub summary: String,
    pub connected: bool,
    /// The Google account, once connected.
    pub account: Option<String>,
    /// An OAuth client ID and secret are available (built in or entered in Settings).
    pub client_configured: bool,
    /// The client came with the build rather than from Settings.
    pub client_built_in: bool,
    pub device_id: String,
    pub device_name: String,
    /// Changes made here that are not on Drive yet.
    pub unsaved_changes: bool,
    /// "2027-03-03 15:30 UTC".
    pub last_saved: Option<String>,
    pub last_saved_seconds_ago: Option<u64>,
    pub last_merge: Option<MergeSummary>,
    pub last_error: Option<String>,
    /// A warning that should stay on screen (offline for days, a clock that is off, ...).
    pub warning: Option<String>,
    /// All devices that save to this Drive folder (this one included).
    pub devices: Vec<DeviceInfo>,
    pub checkpoints: Vec<CheckpointInfo>,
    /// Bytes Minimap uses on Drive (snapshots, checkpoints, media).
    pub drive_bytes: Option<u64>,
    /// Attachment files not on Drive yet.
    pub media_waiting: u32,
    /// Bumped every time a merge changed the data on this device; screens reload when it moves.
    pub data_revision: u64,
    /// Until when the "local only" banner stays hidden ("2027-03-10"), if dismissed.
    pub banner_hidden_until: Option<String>,
    pub media_cache_bytes: u64,
    pub media_cache_limit_bytes: u64,
}

/// Settings for the Drive connection (all device-local; none are synced).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateSyncSettings {
    /// A Google OAuth client ID (Desktop type). Empty clears it.
    pub client_id: Option<String>,
    /// Its secret. Empty clears it.
    pub client_secret: Option<Secret>,
    /// How this device is called in the device list.
    pub device_name: Option<String>,
    /// Hide the "local only" banner for 7 days.
    pub hide_banner: bool,
}

/// The outcome of signing in to Google.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ConnectOutcome {
    /// Connected; Drive had no Minimap data, so this device started it. Show the recovery key
    /// once and ask the user to save it.
    Started { recovery_key: Secret },
    /// Connected; Drive already holds Minimap data from other devices, which were merged in
    /// (or adopted when this device had none of its own).
    Joined { adopted: bool, devices: u32 },
    /// Signed in, but Drive holds Minimap data that can only be opened with the recovery key.
    NeedsRecoveryKey { account: String, devices: u32 },
}

/// The recovery key to open data that is already on Drive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinishConnect {
    pub recovery_key: Secret,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoverCheckpoint {
    pub name: String,
}

/// What a recovery brought back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoverResult {
    pub restored: u32,
    pub saved_current_as: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_outcomes_are_tagged() {
        let json = serde_json::to_string(&ConnectOutcome::Joined {
            adopted: true,
            devices: 2,
        })
        .unwrap();
        assert_eq!(json, r#"{"outcome":"joined","adopted":true,"devices":2}"#);
    }

    #[test]
    fn pictures_are_the_image_kinds() {
        assert!(AttachmentKind::Svg.is_picture());
        assert!(AttachmentKind::Image.is_picture());
        assert!(!AttachmentKind::Pdf.is_picture());
    }
}
