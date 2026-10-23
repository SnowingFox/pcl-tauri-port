//! Tauri commands for resource platforms and local mods. See docs/api/resources.md.
use super::local_mods::{self, LocalMod, ModUpdate};
use super::types::*;
use super::{Result, curseforge, modrinth};
use crate::settings::guard;
use std::path::{Path, PathBuf};

/// `<registered root>/...` only. The mods folder comes from the webview, so it must not
/// be able to point the scanner (which reads and hashes every jar) at arbitrary folders.
fn checked(dir: &str) -> Result<PathBuf> {
    guard::confine_to_registered(Path::new(dir))
}

#[tauri::command]
pub async fn resource_search(platform: Platform, kind: ResourceKind, query: SearchQuery) -> Result<SearchResult> {
    match platform {
        Platform::Modrinth => modrinth::search(kind, &query).await,
        Platform::CurseForge => curseforge::search(kind, &query).await,
    }
}

#[tauri::command]
pub async fn resource_project(platform: Platform, id: String) -> Result<ProjectSummary> {
    match platform {
        Platform::Modrinth => modrinth::project(&id).await,
        Platform::CurseForge => curseforge::project(&id).await,
    }
}

#[tauri::command]
pub async fn resource_versions(
    platform: Platform,
    project_id: String,
    mc_version: Option<String>,
    loader: Option<ModLoader>,
) -> Result<Vec<ResourceFile>> {
    match platform {
        Platform::Modrinth => modrinth::versions(&project_id, mc_version.as_deref(), loader).await,
        Platform::CurseForge => curseforge::files(&project_id, mc_version.as_deref(), loader).await,
    }
}

#[tauri::command]
pub async fn curseforge_download_url(mod_id: String, file_id: u64, file_name: String) -> Result<String> {
    curseforge::resolve_download_url(&mod_id, file_id, &file_name).await
}

#[tauri::command]
pub async fn local_mods_scan(mods_dir: String) -> Result<Vec<LocalMod>> {
    let dir = checked(&mods_dir)?;
    tauri::async_runtime::spawn_blocking(move || local_mods::scan_mods_dir(&dir)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn local_mod_set_enabled(path: String, enabled: bool) -> Result<String> {
    local_mods::set_mod_enabled(&checked(&path)?, enabled).map(|p| p.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn local_mods_check_updates(mods: Vec<LocalMod>, mc_version: String, loader: ModLoader) -> Result<Vec<ModUpdate>> {
    local_mods::check_updates(&mods, &mc_version, loader).await
}
