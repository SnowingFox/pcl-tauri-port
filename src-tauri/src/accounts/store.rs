//! Account list persistence: `<app data dir>/accounts.json`. Holds no secrets.
use super::{AccountInfo, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AccountKind {
    Offline { name: String, uuid: String },
    Microsoft { name: String, uuid: String },
    Yggdrasil { name: String, uuid: String, server_url: String, server_name: String, identifier: String },
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
            AccountKind::Yggdrasil { name, uuid, server_url, server_name, identifier } => {
                ("yggdrasil", name, uuid, Some(server_url.clone()), Some(server_name.clone()), Some(identifier.clone()))
            }
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
    let mut data: StoreData = match std::fs::read(file_path(dir)) {
        Ok(b) => serde_json::from_slice(&b)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(StoreData::default()),
        Err(e) => return Err(e.into()),
    };
    // Older builds added a new entry every time the same offline name was entered (a failed login
    // that was retried left several copies). Collapse them once, and write the result back so the
    // cleanup is not repeated and the file on disk matches what the app shows.
    if dedupe_offline(&mut data) > 0 {
        save(dir, &data)?;
    }
    Ok(data)
}

/// Keep the first offline account per UUID (same UUID = same name = same player) and drop the
/// rest. `selected` follows a removed copy to the entry that stays. Microsoft and third-party
/// accounts are never touched. Returns how many entries were removed.
pub fn dedupe_offline(data: &mut StoreData) -> usize {
    let mut keep: std::collections::HashMap<String, String> = Default::default(); // uuid -> kept id
    let mut moved: std::collections::HashMap<String, String> = Default::default(); // removed id -> kept id
    data.accounts.retain(|a| match &a.kind {
        AccountKind::Offline { uuid, .. } => match keep.get(uuid) {
            Some(kept) => {
                moved.insert(a.id.clone(), kept.clone());
                false
            }
            None => {
                keep.insert(uuid.clone(), a.id.clone());
                true
            }
        },
        _ => true,
    });
    if let Some(sel) = data.selected.clone()
        && let Some(kept) = moved.get(&sel)
    {
        data.selected = Some(kept.clone());
    }
    moved.len()
}

/// Atomic write (temp file + rename).
pub fn save(dir: &Path, data: &StoreData) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join("accounts.json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(data)?)?;
    // Owner-only: the file lists account names, UUIDs and server URLs (tokens live in the Keychain).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(tmp, file_path(dir))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offline(id: &str, name: &str, uuid: &str) -> StoredAccount {
        StoredAccount { id: id.into(), kind: AccountKind::Offline { name: name.into(), uuid: uuid.into() } }
    }

    #[test]
    fn duplicate_offline_accounts_collapse_and_selection_follows() {
        let ms = StoredAccount { id: "ms".into(), kind: AccountKind::Microsoft { name: "Same".into(), uuid: "u-same".into() } };
        let mut d = StoreData {
            selected: Some("c3".into()),
            accounts: vec![
                offline("a1", "3mc_g", "u-a"),
                offline("c1", "c3m", "u-c"),
                ms,
                offline("c2", "c3m", "u-c"),
                offline("c3", "c3m", "u-c"),
                offline("b1", "other", "u-b"),
            ],
            ..Default::default()
        };
        assert_eq!(dedupe_offline(&mut d), 2);
        let ids: Vec<_> = d.accounts.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["a1", "c1", "ms", "b1"], "first copy per UUID stays, order kept, other account kinds untouched");
        assert_eq!(d.selected.as_deref(), Some("c1"), "selection moved from the removed copy to the one that stays");
        assert_eq!(dedupe_offline(&mut d), 0, "idempotent");
    }

    #[test]
    fn load_cleans_up_a_file_written_by_an_older_build_once() {
        let dir = std::env::temp_dir().join(format!("pcl-acc-dedupe-{}", uuid::Uuid::new_v4()));
        let mut d = StoreData {
            selected: Some("x2".into()),
            accounts: vec![offline("x1", "c3m", "u"), offline("x2", "c3m", "u")],
            ..Default::default()
        };
        d.version = 0;
        save(&dir, &d).unwrap();
        let loaded = load(&dir).unwrap();
        assert_eq!(loaded.accounts.len(), 1);
        assert_eq!(loaded.selected.as_deref(), Some("x1"));
        let on_disk: StoreData = serde_json::from_slice(&std::fs::read(file_path(&dir)).unwrap()).unwrap();
        assert_eq!(on_disk.accounts.len(), 1, "the cleanup was written back");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn roundtrip_has_no_tokens() {
        let dir = std::env::temp_dir().join(format!("pcl-acc-{}", uuid::Uuid::new_v4()));
        let mut d = StoreData::default();
        d.accounts.push(StoredAccount { id: "1".into(), kind: AccountKind::Offline { name: "A".into(), uuid: "u".into() } });
        d.accounts.push(StoredAccount { id: "2".into(), kind: AccountKind::Microsoft { name: "B".into(), uuid: "v".into() } });
        save(&dir, &d).unwrap();
        let raw = std::fs::read_to_string(file_path(&dir)).unwrap();
        assert!(!raw.to_lowercase().contains("token"));
        let l = load(&dir).unwrap();
        assert_eq!(l.accounts.len(), 2);
        assert_eq!(l.accounts[1].info().kind, "microsoft");
        std::fs::remove_dir_all(dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn accounts_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("pcl-acc-perm-{}", uuid::Uuid::new_v4()));
        save(&dir, &StoreData::default()).unwrap();
        let mode = std::fs::metadata(file_path(&dir)).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn missing_file_is_empty() {
        let l = load(Path::new("/nonexistent-pcl-dir")).unwrap();
        assert!(l.accounts.is_empty());
    }
}
