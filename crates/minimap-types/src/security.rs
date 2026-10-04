//! Encryption at rest (spec 21).

use serde::{Deserialize, Serialize};

/// A secret typed or shown once (passphrase, recovery key). Its `Debug` never prints the text,
/// so it cannot leak into a log by accident.
#[derive(Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Secret(pub String);

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

/// Shortest accepted passphrase, in characters.
pub const MIN_PASSPHRASE_CHARS: u32 = 12;

/// Where the key to an encrypted database lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyMethod {
    /// A random key in the operating system's keychain; Minimap opens without asking.
    Keychain,
    /// Typed at every start; stored nowhere.
    Passphrase,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityStatus {
    /// An encrypted database is waiting for its key; nothing else works until it is given.
    pub locked: bool,
    /// The database file is encrypted.
    pub encrypted: bool,
    /// How it is unlocked. `None` while locked, when not encrypted, or when it was unlocked with
    /// a recovery key and no keychain entry exists.
    pub method: Option<KeyMethod>,
    pub keychain_available: bool,
    /// Why the keychain can't be used, in words.
    pub keychain_problem: Option<String>,
    /// Backups in the backup folder that are not encrypted (counted only while the database is).
    pub unencrypted_backups: u32,
    pub min_passphrase_chars: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EncryptionChoice {
    #[default]
    Off,
    Keychain,
    Passphrase,
}

/// A request to turn encryption on, change how the key is kept, or turn it off.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SetEncryption {
    pub choice: EncryptionChoice,
    /// The new passphrase (for `Passphrase`).
    pub passphrase: Option<Secret>,
    /// The current passphrase, when the database is in passphrase mode and encryption is being
    /// turned off.
    pub current_passphrase: Option<Secret>,
    /// The user confirmed turning encryption off.
    pub confirmed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptionResult {
    pub status: SecurityStatus,
    /// The new key, shown once, when the keychain holds it (store it somewhere safe: it opens the
    /// database if the keychain is ever lost). `None` otherwise.
    pub recovery_key: Option<Secret>,
}
