//! One error type for everything the sync crate does. Messages are sentences for the user and
//! never contain keys, tokens, file names or note text.

use minimap_store::StoreError;

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    /// Google Drive can't be reached (no network, DNS, a timeout). The work is retried later.
    #[error("Can't reach Google Drive: {0}")]
    Offline(String),
    /// Google no longer accepts the sign-in (revoked, expired, wrong client). The user has to
    /// connect again.
    #[error("Google sign-in problem: {0}")]
    Auth(String),
    /// Drive answered, but refused or failed.
    #[error("Google Drive said: {0}")]
    Drive(String),
    /// The vault key (recovery key) doesn't open what is on Drive.
    #[error("That recovery key doesn't open the data on Google Drive")]
    WrongKey,
    /// A file is damaged or has been changed (authentication failed, checksum differs).
    #[error("A file is damaged or has been tampered with: {0}")]
    Corrupt(String),
    /// Another device saved data with a newer version of Minimap.
    #[error("{device} uses a newer version of Minimap (data format {found}; this one understands {supported}). Update Minimap on this device to sync with it")]
    NewerData {
        device: String,
        found: u32,
        supported: u32,
    },
    /// The encrypted database is waiting for its key.
    #[error("The database is locked")]
    Locked,
    #[error("Google Drive is not connected")]
    NotConnected,
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Io(String),
    #[error("cancelled")]
    Cancelled,
    #[error(transparent)]
    Store(#[from] StoreError),
}

pub type Result<T> = std::result::Result<T, SyncError>;

impl SyncError {
    pub fn io(context: &str, e: std::io::Error) -> Self {
        SyncError::Io(format!("{context}: {e}"))
    }

    /// Worth trying again later without bothering the user.
    pub fn is_transient(&self) -> bool {
        matches!(self, SyncError::Offline(_))
            || matches!(self, SyncError::Drive(m) if m.contains("try again"))
    }

    /// The user has to do something before syncing can continue.
    pub fn needs_attention(&self) -> bool {
        matches!(self, SyncError::Auth(_) | SyncError::WrongKey)
    }
}
