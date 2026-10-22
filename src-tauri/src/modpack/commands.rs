//! Tauri commands for modpacks. See docs/api/resources.md.
use super::*;
use crate::settings::guard;
use std::path::PathBuf;

/// Archives are opened read-only as zip files; they must at least look like modpacks.
fn check_archive(archive: &str) -> Result<PathBuf, String> {
    let p = guard::clean_abs(Path::new(archive))?;
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if !matches!(ext.as_str(), "zip" | "mrpack") {
        return Err("仅支持 .zip 与 .mrpack 整合包".into());
    }
    if !p.is_file() {
        return Err(format!("文件不存在：{archive}"));
    }
    Ok(p)
}

/// Parse a modpack archive into an install plan (CurseForge files remain unresolved).
#[tauri::command]
pub async fn modpack_parse(archive: String) -> Result<ModpackInstallPlan, String> {
    let archive = check_archive(&archive)?;
    tauri::async_runtime::spawn_blocking(move || parse_pack(&archive))
        .await
        .map_err(|e| e.to_string())?
}

/// Resolve CurseForge file references (needs PCL_CURSEFORGE_KEY). Returns the updated plan and
/// the references that no longer exist upstream.
#[tauri::command]
pub async fn modpack_resolve(mut plan: ModpackInstallPlan) -> Result<(ModpackInstallPlan, Vec<CurseForgeRef>), String> {
    plan.validate()?;
    let missing = curseforge_pack::resolve(&mut plan).await?;
    Ok((plan, missing))
}

/// Copy override directories from the archive into the instance directory.
#[tauri::command]
pub async fn modpack_extract_overrides(archive: String, plan: ModpackInstallPlan, instance_dir: String) -> Result<usize, String> {
    let archive = check_archive(&archive)?;
    plan.validate()?;
    // Only `<registered root>/versions/<name>` may receive files.
    let instance_dir = guard::confine_instance_dir(&guard::registered_roots(), Path::new(&instance_dir))?;
    tauri::async_runtime::spawn_blocking(move || extract_overrides(&archive, &plan, &instance_dir))
        .await
        .map_err(|e| e.to_string())?
}
