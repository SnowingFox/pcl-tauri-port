//! Per-instance settings, stored at `<versions>/<name>/PCL/config.json`
//! (same location PCL uses for its `JsonConfigProvider`).

use super::store::{SettingsError, atomic_write_json, read_json};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const INSTANCE_SCHEMA_VERSION: u32 = 1;

/// Tri-state override: `None` means "use the global setting".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InstanceSettings {
    pub schema_version: u32,
    /// Force version isolation on/off. `None` = decide automatically.
    pub isolated: Option<bool>,
    /// `None` = use global memory setting.
    pub memory_mb: Option<u32>,
    /// `None` = use global Java.
    pub java_path: Option<String>,
    pub jvm_args: String,
    pub game_args: String,
    pub pre_launch_command: String,
    pub wait_for_pre_launch: bool,
    pub server_address: String,
    pub custom_title: String,
    pub custom_info: String,
    pub starred: bool,
    /// 0 = auto, 1 = hidden, other values map to `CardCategory`.
    pub display_type: i32,
    pub logo: String,
    pub logo_custom: bool,
    pub disable_mod_update_check: bool,
}

impl Default for InstanceSettings {
    fn default() -> Self {
        Self {
            schema_version: INSTANCE_SCHEMA_VERSION,
            isolated: None,
            memory_mb: None,
            java_path: None,
            jvm_args: String::new(),
            game_args: String::new(),
            pre_launch_command: String::new(),
            wait_for_pre_launch: true,
            server_address: String::new(),
            custom_title: String::new(),
            custom_info: String::new(),
            starred: false,
            display_type: 0,
            logo: String::new(),
            logo_custom: false,
            disable_mod_update_check: false,
        }
    }
}

pub fn config_path(instance_dir: &Path) -> PathBuf {
    instance_dir.join("PCL").join("config.json")
}

impl InstanceSettings {
    /// Missing or unreadable file yields defaults.
    pub fn load(instance_dir: &Path) -> Self {
        read_json::<InstanceSettings>(&config_path(instance_dir)).ok().flatten().unwrap_or_default()
    }

    pub fn save(&self, instance_dir: &Path) -> Result<(), SettingsError> {
        if !instance_dir.is_dir() {
            return Err(SettingsError::Invalid(format!("instance dir not found: {}", instance_dir.display())));
        }
        let mut s = self.clone();
        s.schema_version = INSTANCE_SCHEMA_VERSION;
        if let Some(mb) = s.memory_mb {
            s.memory_mb = Some(mb.clamp(512, 1 << 20));
        }
        atomic_write_json(&config_path(instance_dir), &s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_defaults() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(InstanceSettings::load(d.path()), InstanceSettings::default());
        let s = InstanceSettings {
            isolated: Some(true),
            memory_mb: Some(1),
            starred: true,
            ..Default::default()
        };
        s.save(d.path()).unwrap();
        let back = InstanceSettings::load(d.path());
        assert_eq!(back.isolated, Some(true));
        assert_eq!(back.memory_mb, Some(512));
        assert!(back.starred);
    }

    #[test]
    fn save_rejects_missing_dir() {
        let d = tempfile::tempdir().unwrap();
        assert!(InstanceSettings::default().save(&d.path().join("nope")).is_err());
    }
}
