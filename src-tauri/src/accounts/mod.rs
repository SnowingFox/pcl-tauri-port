//! Account / authentication module.
//!
//! Ported (logic and flow, not line by line) from Plain Craft Launcher 2
//! (https://github.com/Hex-Dragon/PCL2, see its LICENCE; this is a third-party
//! derivative) with the Swift macOS port PCL.Mac used as a flow reference.
//!
//! Layout:
//! - `offline`   : offline names and deterministic `OfflinePlayer:` UUIDs
//! - `microsoft` : device-code OAuth -> Xbox Live -> XSTS -> Minecraft services
//! - `yggdrasil` : third-party (authlib-injector compatible) servers
//! - `skin`      : skin / avatar helpers
//! - `secrets`   : macOS Keychain storage (via `keyring`) for tokens
//! - `store`     : JSON account list persistence (no secrets)
//! - `manager`   : ties everything together
//! - `commands`  : `#[tauri::command]` surface
//!
//! Microsoft sign-in needs an Azure app client ID, see `docs/api/accounts.md`.

pub mod commands;
pub mod manager;
pub mod microsoft;
pub mod offline;
pub mod secrets;
pub mod skin;
pub mod store;
pub mod yggdrasil;

use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum AccountError {
    #[error("network error: {0}")]
    Network(String),
    #[error("unexpected response: {0}")]
    BadResponse(String),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("account not found: {0}")]
    NotFound(String),
    #[error("Microsoft client ID is not configured (set PCL_MS_CLIENT_ID or the accounts setting)")]
    MissingClientId,
    #[error("login was cancelled")]
    Cancelled,
    #[error("the device code expired, please start again")]
    DeviceCodeExpired,
    #[error("authorization was declined")]
    Declined,
    #[error("this account does not own Minecraft: Java Edition")]
    NotOwned,
    #[error("Xbox error: {0}")]
    Xbox(String),
    #[error("login expired, please sign in again")]
    NeedRelogin,
    #[error("server error: {0}")]
    Server(String),
    #[error("keychain error: {0}")]
    Keychain(String),
    #[error("io error: {0}")]
    Io(String),
}

impl From<reqwest::Error> for AccountError {
    fn from(e: reqwest::Error) -> Self {
        AccountError::Network(e.without_url().to_string())
    }
}
impl From<std::io::Error> for AccountError {
    fn from(e: std::io::Error) -> Self {
        AccountError::Io(e.to_string())
    }
}
impl From<serde_json::Error> for AccountError {
    fn from(e: serde_json::Error) -> Self {
        AccountError::BadResponse(e.to_string())
    }
}
impl Serialize for AccountError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, AccountError>;

/// Everything the launcher needs to start the game for one account.
/// `uuid` is the undashed lowercase hex form used on the command line.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuthSession {
    pub player_name: String,
    pub uuid: String,
    pub access_token: String,
    /// `msa`, `mojang` (third-party) or `legacy` (offline), as in `--userType`.
    pub user_type: String,
    /// Account kind: `offline`, `microsoft` or `yggdrasil`.
    pub account_kind: String,
    /// Yggdrasil client token, if any.
    pub client_token: Option<String>,
    /// Set for third-party accounts: needs `-javaagent` with authlib-injector.
    pub authlib_injector: Option<AuthlibInjectorConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuthlibInjectorConfig {
    /// API root, passed as `-javaagent:<jar>=<server_url>`.
    pub server_url: String,
    /// Base64 of the server metadata for `-Dauthlibinjector.yggdrasil.prefetched=`.
    pub prefetched: Option<String>,
}

/// Public, secret-free view of a stored account.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfo {
    pub id: String,
    pub kind: String,
    pub name: String,
    /// Dashed UUID.
    pub uuid: String,
    pub server_url: Option<String>,
    pub server_name: Option<String>,
    pub identifier: Option<String>,
}

/// Parse an undashed or dashed UUID into undashed lowercase hex.
pub fn undash(uuid: &str) -> String {
    uuid.replace('-', "").to_lowercase()
}

/// Turn 32 hex chars into the dashed form; returns input unchanged otherwise.
pub fn dash(uuid: &str) -> String {
    let u = undash(uuid);
    if u.len() != 32 {
        return uuid.to_string();
    }
    format!("{}-{}-{}-{}-{}", &u[..8], &u[8..12], &u[12..16], &u[16..20], &u[20..])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dash_roundtrip() {
        let d = "b50ad385-829d-3141-a216-7e7d7539ba7f";
        assert_eq!(dash(&undash(d)), d);
        assert_eq!(undash(d), "b50ad385829d3141a2167e7d7539ba7f");
    }
}
