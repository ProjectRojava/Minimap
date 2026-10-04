//! The operating system's keychain, for the random database key (spec 21).
//!
//! Two slots make changing the key crash-safe: the new key is written to `Pending` and read back
//! *before* the database is re-keyed, and only afterwards becomes `Current`. A crash anywhere in
//! between leaves a key that opens the database in one slot or the other, and start-up tries both.

use minimap_store::security::RawKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Slot {
    /// The key the database is encrypted with.
    Current,
    /// A key being switched to.
    Pending,
}

impl Slot {
    fn account(self) -> &'static str {
        match self {
            Slot::Current => "database-key",
            Slot::Pending => "database-key-pending",
        }
    }
}

/// Where the random key is kept. Errors are sentences for the user, never containing key text.
pub trait KeyStore: Send + Sync {
    /// `Ok` when a keychain can be used; otherwise why not.
    fn availability(&self) -> Result<(), String>;
    fn get(&self, slot: Slot) -> Result<Option<RawKey>, String>;
    fn set(&self, slot: Slot, key: &RawKey) -> Result<(), String>;
    /// Removes a slot; a slot that is already empty is fine.
    fn delete(&self, slot: Slot) -> Result<(), String>;
}

const SERVICE: &str = "app.minimap.desktop";

/// macOS Keychain, Windows Credential Manager, or the Secret Service (GNOME Keyring, KWallet)
/// on Linux, through the `keyring` crate.
pub struct OsKeyStore;

fn describe(e: &keyring::Error) -> String {
    format!("The system keychain refused: {e}")
}

fn entry(slot: Slot) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, slot.account()).map_err(|e| describe(&e))
}

impl KeyStore for OsKeyStore {
    fn availability(&self) -> Result<(), String> {
        match keyring::Entry::store_status() {
            Ok(()) => Ok(()),
            Err(e) => Err(format!(
                "No system keychain is available ({e}). On Linux, install and unlock GNOME Keyring \
                 or KWallet (a Secret Service provider); until then use a passphrase"
            )),
        }
    }

    fn get(&self, slot: Slot) -> Result<Option<RawKey>, String> {
        match entry(slot)?.get_password() {
            Ok(text) => RawKey::from_hex(&text)
                .map(Some)
                .ok_or_else(|| "The keychain entry is not a valid database key".to_owned()),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(describe(&e)),
        }
    }

    fn set(&self, slot: Slot, key: &RawKey) -> Result<(), String> {
        let hex = key.to_hex();
        entry(slot)?
            .set_password(hex.as_str())
            .map_err(|e| describe(&e))
    }

    fn delete(&self, slot: Slot) -> Result<(), String> {
        match entry(slot)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(describe(&e)),
        }
    }
}

#[cfg(test)]
pub(crate) mod memory {
    use std::{collections::HashMap, sync::Mutex};

    use super::*;

    /// A keychain in memory, for tests; can be made unavailable or made to refuse writes.
    pub struct MemoryKeyStore {
        pub slots: Mutex<HashMap<Slot, RawKey>>,
        pub available: bool,
        pub refuse_current_writes: bool,
    }

    impl MemoryKeyStore {
        pub fn new() -> Self {
            Self {
                slots: Mutex::new(HashMap::new()),
                available: true,
                refuse_current_writes: false,
            }
        }

        pub fn unavailable() -> Self {
            Self {
                available: false,
                ..Self::new()
            }
        }

        pub fn has(&self, slot: Slot) -> bool {
            self.slots.lock().unwrap().contains_key(&slot)
        }
    }

    impl KeyStore for MemoryKeyStore {
        fn availability(&self) -> Result<(), String> {
            if self.available {
                Ok(())
            } else {
                Err("No system keychain is available".into())
            }
        }

        fn get(&self, slot: Slot) -> Result<Option<RawKey>, String> {
            self.availability()?;
            Ok(self.slots.lock().unwrap().get(&slot).cloned())
        }

        fn set(&self, slot: Slot, key: &RawKey) -> Result<(), String> {
            self.availability()?;
            if slot == Slot::Current && self.refuse_current_writes {
                return Err("The keychain refused the write".into());
            }
            self.slots.lock().unwrap().insert(slot, key.clone());
            Ok(())
        }

        fn delete(&self, slot: Slot) -> Result<(), String> {
            self.availability()?;
            self.slots.lock().unwrap().remove(&slot);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whatever this machine has (a keychain, or none in CI and containers), asking must answer
    /// promptly with a sentence rather than panic or hang.
    #[test]
    fn the_system_keychain_reports_whether_it_can_be_used() {
        match OsKeyStore.availability() {
            Ok(()) => {}
            Err(why) => assert!(!why.is_empty()),
        }
    }
}
