//! Token storage. Refresh/access tokens never go into the JSON file.
use super::{AccountError, Result};
use std::collections::HashMap;
use std::sync::Mutex;

const SERVICE: &str = "pcl-tauri-port";

pub trait SecretStore: Send + Sync {
    fn set(&self, key: &str, value: &str) -> Result<()>;
    fn get(&self, key: &str) -> Result<Option<String>>;
    fn delete(&self, key: &str) -> Result<()>;
}

/// macOS Keychain (generic password items under service `pcl-tauri-port`).
pub struct KeychainStore;

fn entry(key: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, key).map_err(|e| AccountError::Keychain(e.to_string()))
}

impl SecretStore for KeychainStore {
    fn set(&self, key: &str, value: &str) -> Result<()> {
        entry(key)?.set_password(value).map_err(|e| AccountError::Keychain(e.to_string()))
    }
    fn get(&self, key: &str) -> Result<Option<String>> {
        match entry(key)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(AccountError::Keychain(e.to_string())),
        }
    }
    fn delete(&self, key: &str) -> Result<()> {
        match entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(AccountError::Keychain(e.to_string())),
        }
    }
}

/// In-memory store for tests.
#[allow(dead_code)]
#[derive(Default)]
pub struct MemoryStore(Mutex<HashMap<String, String>>);

impl SecretStore for MemoryStore {
    fn set(&self, key: &str, value: &str) -> Result<()> {
        self.0.lock().unwrap().insert(key.into(), value.into());
        Ok(())
    }
    fn get(&self, key: &str) -> Result<Option<String>> {
        Ok(self.0.lock().unwrap().get(key).cloned())
    }
    fn delete(&self, key: &str) -> Result<()> {
        self.0.lock().unwrap().remove(key);
        Ok(())
    }
}

pub fn ms_refresh_key(id: &str) -> String {
    format!("ms:{id}:refresh")
}
pub fn ygg_access_key(id: &str) -> String {
    format!("ygg:{id}:access")
}
pub fn ygg_client_key(id: &str) -> String {
    format!("ygg:{id}:client")
}
