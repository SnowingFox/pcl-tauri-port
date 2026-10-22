//! Asset index handling. Downloading is not done here; this reports what is
//! missing so the download module can fetch it.

use super::LaunchError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct AssetIndexFile {
    pub objects: HashMap<String, AssetObject>,
    #[serde(default)]
    pub map_to_resources: bool,
    #[serde(default, rename = "virtual")]
    pub is_virtual: bool,
}

#[derive(Debug, Deserialize)]
pub struct AssetObject {
    pub hash: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MissingAsset {
    pub name: String,
    pub hash: String,
    pub size: u64,
    pub path: String,
}

pub fn index_path(assets_root: &Path, id: &str) -> PathBuf {
    assets_root.join("indexes").join(format!("{id}.json"))
}

pub fn object_path(assets_root: &Path, hash: &str) -> PathBuf {
    assets_root.join("objects").join(hash.get(..2).unwrap_or("00")).join(hash)
}

/// The index file is untrusted (it may come from a shared instance or a tampered download):
/// the id must be one path component and every hash / legacy name must be a safe path.
pub fn load_index(assets_root: &Path, id: &str) -> Result<AssetIndexFile, LaunchError> {
    use crate::settings::guard::{is_hex_hash, is_safe_component, is_safe_rel};
    if !is_safe_component(id) {
        return Err(LaunchError::VersionJson(format!("unsafe asset index id: {id}")));
    }
    let p = index_path(assets_root, id);
    let text = std::fs::read_to_string(&p).map_err(|e| LaunchError::Io(format!("{}: {e}", p.display())))?;
    let idx: AssetIndexFile = serde_json::from_str(&text).map_err(|e| LaunchError::VersionJson(format!("asset index {id}: {e}")))?;
    if let Some((name, o)) = idx.objects.iter().find(|(n, o)| !is_hex_hash(&o.hash) || !is_safe_rel(n)) {
        return Err(LaunchError::VersionJson(format!("asset index {id}: unsafe entry {name:?} / {:?}", o.hash)));
    }
    Ok(idx)
}

pub fn missing_assets(assets_root: &Path, idx: &AssetIndexFile) -> Vec<MissingAsset> {
    idx.objects
        .iter()
        .filter_map(|(name, o)| {
            let p = object_path(assets_root, &o.hash);
            let ok = p.metadata().map(|m| m.len() == o.size).unwrap_or(false);
            (!ok).then(|| MissingAsset { name: name.clone(), hash: o.hash.clone(), size: o.size, path: p.display().to_string() })
        })
        .collect()
}

/// Pre-1.7 (`map_to_resources`) and 1.7 (`virtual`) indexes need the objects
/// copied to a readable tree. Returns the directory to use, if any.
pub fn materialize_legacy(assets_root: &Path, game_dir: &Path, idx: &AssetIndexFile) -> Result<Option<PathBuf>, LaunchError> {
    let dest = if idx.map_to_resources {
        game_dir.join("resources")
    } else if idx.is_virtual {
        assets_root.join("virtual").join("legacy")
    } else {
        return Ok(None);
    };
    for (name, o) in &idx.objects {
        let src = object_path(assets_root, &o.hash);
        let dst = dest.join(name);
        if dst.is_file() || !src.is_file() {
            continue;
        }
        if let Some(p) = dst.parent() {
            std::fs::create_dir_all(p).map_err(|e| LaunchError::Io(e.to_string()))?;
        }
        std::fs::copy(&src, &dst).map_err(|e| LaunchError::Io(e.to_string()))?;
    }
    Ok(Some(dest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostile_index_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("indexes")).unwrap();
        let write = |id: &str, body: &str| std::fs::write(root.join("indexes").join(format!("{id}.json")), body).unwrap();
        write("ok", r#"{"objects":{"minecraft/sounds/a.ogg":{"hash":"abcdef0123","size":3}}}"#);
        assert!(load_index(root, "ok").is_ok());
        write("h1", r#"{"objects":{"a":{"hash":"../../../../.ssh/authorized_keys","size":3}}}"#);
        assert!(load_index(root, "h1").is_err());
        write("h2", r#"{"objects":{"../../../escape":{"hash":"abcdef0123","size":3}},"virtual":true}"#);
        assert!(load_index(root, "h2").is_err());
        write("h3", r#"{"objects":{"/etc/passwd":{"hash":"abcdef0123","size":3}}}"#);
        assert!(load_index(root, "h3").is_err());
        write("h4", r#"{"objects":{"a":{"hash":"é","size":3}}}"#);
        assert!(load_index(root, "h4").is_err());
        // The index id itself is a path component.
        assert!(load_index(root, "../ok").is_err());
        assert!(load_index(root, "a/b").is_err());
        assert!(load_index(root, "").is_err());
        // object_path never panics on short or multi-byte hashes.
        let _ = object_path(root, "é");
        let _ = object_path(root, "");
    }

    #[test]
    fn missing_detection() {
        let idx: AssetIndexFile = serde_json::from_str(r#"{"objects":{"a/b.ogg":{"hash":"abcdef","size":3}},"virtual":true}"#).unwrap();
        assert!(idx.is_virtual);
        let tmp = std::env::temp_dir().join(format!("pcl-assets-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        assert_eq!(missing_assets(&tmp, &idx).len(), 1);
        let op = object_path(&tmp, "abcdef");
        std::fs::create_dir_all(op.parent().unwrap()).unwrap();
        std::fs::write(&op, b"123").unwrap();
        assert!(missing_assets(&tmp, &idx).is_empty());
        let d = materialize_legacy(&tmp, &tmp, &idx).unwrap().unwrap();
        assert!(d.join("a/b.ogg").is_file());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
