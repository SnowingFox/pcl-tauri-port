//! App data layout and the one-time migration from the pre-identifier directory.
//!
//! Everything lives under `~/Library/Application Support/<identifier>/` (see
//! `settings::store::data_dir`): `settings.json`, `accounts.json`, `logs/`, `tools/`.

use crate::settings::store::{data_dir, legacy_data_dir};
use std::path::{Path, PathBuf};

pub fn logs_dir(base: &Path) -> PathBuf {
    base.join("logs")
}

pub fn launch_logs_dir(base: &Path) -> PathBuf {
    logs_dir(base).join("launch")
}

pub fn authlib_injector_jar(base: &Path) -> PathBuf {
    base.join("tools").join("authlib-injector.jar")
}

/// Copy files from the legacy directory that do not exist in the new one yet. The legacy
/// directory is never modified or deleted, so the migration is safe to repeat and to roll back.
/// Returns the number of files copied.
pub fn migrate_dir(legacy: &Path, new: &Path) -> std::io::Result<usize> {
    if !legacy.is_dir() || legacy == new {
        return Ok(0);
    }
    let mut copied = 0;
    std::fs::create_dir_all(new)?;
    for entry in std::fs::read_dir(legacy)? {
        let entry = entry?;
        let from = entry.path();
        let to = new.join(entry.file_name());
        let ty = entry.file_type()?;
        if ty.is_dir() {
            // Logs and caches are cheap to regenerate; only carry small config directories.
            let n = entry.file_name();
            if n == "logs" || n == "cache" {
                continue;
            }
            copied += migrate_dir(&from, &to)?;
        } else if ty.is_file() && !to.exists() {
            std::fs::copy(&from, &to)?;
            copied += 1;
        }
    }
    Ok(copied)
}

/// Run before anything reads the settings. Failures are logged by the caller, never fatal.
pub fn migrate_legacy() -> std::io::Result<usize> {
    if std::env::var_os("PCL_DATA_DIR").is_some() {
        return Ok(0);
    }
    migrate_dir(&legacy_data_dir(), &data_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_without_overwriting_or_deleting() {
        let d = tempfile::tempdir().unwrap();
        let (old, new) = (d.path().join("old"), d.path().join("new"));
        std::fs::create_dir_all(old.join("sub")).unwrap();
        std::fs::create_dir_all(old.join("logs")).unwrap();
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(old.join("settings.json"), "old").unwrap();
        std::fs::write(old.join("accounts.json"), "acc").unwrap();
        std::fs::write(old.join("sub/x.json"), "x").unwrap();
        std::fs::write(old.join("logs/a.log"), "l").unwrap();
        std::fs::write(new.join("accounts.json"), "keep").unwrap();
        assert_eq!(migrate_dir(&old, &new).unwrap(), 2);
        assert_eq!(std::fs::read_to_string(new.join("settings.json")).unwrap(), "old");
        assert_eq!(std::fs::read_to_string(new.join("accounts.json")).unwrap(), "keep");
        assert!(new.join("sub/x.json").is_file());
        assert!(!new.join("logs").exists());
        assert!(old.join("settings.json").is_file(), "legacy dir must stay intact");
        assert_eq!(migrate_dir(&old, &new).unwrap(), 0, "idempotent");
        assert_eq!(migrate_dir(&d.path().join("missing"), &new).unwrap(), 0);
    }
}
