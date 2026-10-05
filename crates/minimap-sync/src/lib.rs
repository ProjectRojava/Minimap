//! Google Drive storage, multi-device sync and attachments (spec 22, ADR-0011).
//!
//! No SQL lives here: the database side (snapshots, merging, attachment rows) is in
//! `minimap-store`. This crate owns the encryption, the remote (Drive and a folder stand-in for
//! tests), the OAuth sign-in and the engine that decides when to pull, merge and save.

pub mod crypto;
pub mod drive;
pub mod engine;
pub mod error;
pub mod folder;
pub mod media;
pub mod oauth;
pub mod remote;
pub mod vault;

pub use engine::{Engine, Host, Probe};
pub use error::{Result, SyncError};
pub use remote::{CheckpointFile, DeviceFile, DeviceMeta, MediaFile, Remote};
