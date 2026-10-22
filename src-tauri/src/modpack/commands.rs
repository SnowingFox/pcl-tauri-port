//! Tauri commands for modpacks. See docs/api/resources.md.
use super::*;
use std::path::PathBuf;

/// Parse a modpack archive into an install plan (CurseForge files remain unresolved).
#[tauri::command]
pub async fn modpack_parse(archive: String) -> Result<ModpackInstallPlan, String> {
    tauri::async_runtime::spawn_blocking(move || parse_pack(&PathBuf::from(archive)))
        .await
        .map_err(|e| e.to_string())?
}

/// Resolve CurseForge file references (needs PCL_CURSEFORGE_KEY). Returns the updated plan and
/// the references that no longer exist upstream.
#[tauri::command]
pub async fn modpack_resolve(mut plan: ModpackInstallPlan) -> Result<(ModpackInstallPlan, Vec<CurseForgeRef>), String> {
    let missing = curseforge_pack::resolve(&mut plan).await?;
    Ok((plan, missing))
}

/// Copy override directories from the archive into the instance directory.
#[tauri::command]
pub async fn modpack_extract_overrides(archive: String, plan: ModpackInstallPlan, instance_dir: String) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || extract_overrides(&PathBuf::from(archive), &plan, &PathBuf::from(instance_dir)))
        .await
        .map_err(|e| e.to_string())?
}
