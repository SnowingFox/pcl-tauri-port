//! Tauri commands for global and per-instance settings.

use super::instance_settings::InstanceSettings;
use super::model::GlobalSettings;
use super::store::SettingsStore;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::OnceLock;

fn store() -> &'static SettingsStore {
    static STORE: OnceLock<SettingsStore> = OnceLock::new();
    STORE.get_or_init(|| SettingsStore::load(SettingsStore::default_path()))
}

/// Shared handle for other modules (launch, download) that need to read settings.
pub fn global() -> &'static SettingsStore {
    store()
}

#[tauri::command]
pub fn settings_get() -> GlobalSettings {
    store().get()
}

#[tauri::command]
pub fn settings_set(settings: GlobalSettings) -> Result<GlobalSettings, String> {
    store().set(settings).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn settings_patch(patch: Value) -> Result<GlobalSettings, String> {
    store().patch(patch).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn settings_reset() -> Result<GlobalSettings, String> {
    store().reset().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn settings_path() -> String {
    store().path().to_string_lossy().into_owned()
}

#[tauri::command]
pub fn instance_settings_get(instance_dir: String) -> InstanceSettings {
    InstanceSettings::load(&PathBuf::from(instance_dir))
}

#[tauri::command]
pub fn instance_settings_set(instance_dir: String, settings: InstanceSettings) -> Result<(), String> {
    settings.save(&PathBuf::from(instance_dir)).map_err(|e| e.to_string())
}
