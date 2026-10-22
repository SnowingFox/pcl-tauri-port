//! Account list persistence: `<app data dir>/accounts.json`. Holds no secrets.
use super::{AccountInfo, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AccountKind {
    Offline { name: String, uuid: String },
    Microsoft { name: String, uuid: String },
    Yggdrasil {
        name: String,
        uuid: String,
        server_url: String,
        server_name: String,
        identifier: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredAccount {
    pub id: String,
    #[serde(flatten)]
    pub kind: AccountKind,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    /// Azure app client ID for Microsoft login (overrides env).
    pub ms_client_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StoreData {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub selected: Option<String>,
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub accounts: Vec<StoredAccount>,
}

impl StoredAccount {
    pub fn info(&self) -> AccountInfo {
        let (kind, name, uuid, url, sname, ident) = match &self.kind {
            AccountKind::Offline { name, uuid } => ("offline", name, uuid, None, None, None),
            AccountKind::Microsoft { name, uuid } => ("microsoft", name, uuid, None, None, None),
            AccountKind::Yggdrasil { name, uuid, server_url, server_name, identifier } => (
                "yggdrasil",
                name,
                uuid,
                Some(server_url.clone()),
                Some(server_name.clone()),
                Some(identifier.clone()),
            ),
        };
        AccountInfo {
            id: self.id.clone(),
            kind: kind.into(),
            name: name.clone(),
            uuid: uuid.clone(),
            server_url: url,
            server_name: sname,
            identifier: ident,
        }
    }
}

pub fn file_path(dir: &Path) -> PathBuf {
    dir.join("accounts.json")
}

pub fn load(dir: &Path) -> Result<StoreData> {
    match std::fs::read(file_path(dir)) {
        Ok(b) => Ok(serde_json::from_slice(&b)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(StoreData::default()),
        Err(e) => Err(e.into()),
    }
}

/// Atomic write (temp file + rename).
pub fn save(dir: &Path, data: &StoreData) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join("accounts.json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(data)?)?;
    std::fs::rename(tmp, file_path(dir))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_has_no_tokens() {
        let dir = std::env::temp_dir().join(format!("pcl-acc-{}", uuid::Uuid::new_v4()));
        let mut d = StoreData::default();
        d.accounts.push(StoredAccount {
            id: "1".into(),
            kind: AccountKind::Offline { name: "A".into(), uuid: "u".into() },
        });
        d.accounts.push(StoredAccount {
            id: "2".into(),
            kind: AccountKind::Microsoft { name: "B".into(), uuid: "v".into() },
        });
        save(&dir, &d).unwrap();
        let raw = std::fs::read_to_string(file_path(&dir)).unwrap();
        assert!(!raw.to_lowercase().contains("token"));
        let l = load(&dir).unwrap();
        assert_eq!(l.accounts.len(), 2);
        assert_eq!(l.accounts[1].info().kind, "microsoft");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn missing_file_is_empty() {
        let l = load(Path::new("/nonexistent-pcl-dir")).unwrap();
        assert!(l.accounts.is_empty());
    }
}
