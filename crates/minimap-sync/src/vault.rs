//! `vault.json`: what sits on Drive so a new device can tell, before downloading or replacing
//! anything, whether the recovery key it was given is the right one. It holds the format
//! version and an encrypted known value; no key material.

use base64::{engine::general_purpose::STANDARD, Engine};
use minimap_store::security::RawKey;
use serde::{Deserialize, Serialize};

use crate::{
    crypto::{open, seal},
    error::{Result, SyncError},
};

/// The layout version of the Drive folder this code writes and understands.
pub const FORMAT: u32 = 1;
const PURPOSE: &str = "vault-check";
const KNOWN: &[u8] = b"minimap-vault-ok";

#[derive(Serialize, Deserialize)]
struct VaultFile {
    format: u32,
    check: String,
}

/// The bytes of a new `vault.json` for `key`.
pub fn create(key: &RawKey) -> Result<Vec<u8>> {
    let file = VaultFile {
        format: FORMAT,
        check: STANDARD.encode(seal(key, PURPOSE, KNOWN)?),
    };
    serde_json::to_vec_pretty(&file).map_err(|e| SyncError::Invalid(e.to_string()))
}

/// Checks `key` against a `vault.json`: `WrongKey` when it doesn't match, `Invalid` when the
/// file isn't one or is from a newer layout.
pub fn verify(bytes: &[u8], key: &RawKey) -> Result<()> {
    let file: VaultFile = serde_json::from_slice(bytes)
        .map_err(|_| SyncError::Invalid("The vault file on Google Drive is not valid".into()))?;
    if file.format > FORMAT {
        return Err(SyncError::Invalid(
            "The data on Google Drive was made by a newer version of Minimap. Update Minimap to use it"
                .into(),
        ));
    }
    let sealed = STANDARD
        .decode(file.check.as_bytes())
        .map_err(|_| SyncError::Invalid("The vault file on Google Drive is not valid".into()))?;
    let plain = open(key, PURPOSE, &sealed)?;
    if plain == KNOWN {
        Ok(())
    } else {
        Err(SyncError::WrongKey)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_right_key_passes_and_others_are_refused() {
        let key = RawKey::generate().unwrap();
        let bytes = create(&key).unwrap();
        verify(&bytes, &key).unwrap();
        assert!(matches!(
            verify(&bytes, &RawKey::generate().unwrap()),
            Err(SyncError::WrongKey)
        ));
        let text = String::from_utf8(bytes).unwrap();
        assert!(
            !text.contains(key.to_hex().as_str()),
            "no key material in the file"
        );
    }

    #[test]
    fn damaged_or_newer_files_are_told_apart_from_a_wrong_key() {
        let key = RawKey::generate().unwrap();
        assert!(matches!(
            verify(b"{not json", &key),
            Err(SyncError::Invalid(_))
        ));
        assert!(matches!(
            verify(br#"{"format":1,"check":"!!!"}"#, &key),
            Err(SyncError::Invalid(_))
        ));
        assert!(matches!(
            verify(br#"{"format":99,"check":""}"#, &key),
            Err(SyncError::Invalid(_))
        ));
    }
}
