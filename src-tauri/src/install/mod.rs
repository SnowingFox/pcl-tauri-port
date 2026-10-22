//! Game and mod-loader installation commands. Each install is a task managed by
//! `download::Manager`; progress arrives on `download://progress`.

pub mod forge;
pub mod loaders;
pub mod vanilla;

use crate::download::error::DlError;
use crate::download::{Manager, Mgr};
use std::path::PathBuf;
use std::sync::Arc;

#[tauri::command]
pub fn install_vanilla(m: Mgr<'_>, mc_version: String) -> Result<String, DlError> {
    crate::settings::guard::check_mc_version(&mc_version).map_err(DlError::Other)?;
    let mgr: Arc<Manager> = m.inner().clone();
    let mc = mc_version.clone();
    Ok(m.spawn("vanilla", &format!("Minecraft {mc_version}"), move |ctx| {
        let mgr = mgr.clone();
        async move { vanilla::install_vanilla(&ctx, &mgr, &mc, 0.0, 1.0).await.map(|_| ()) }
    }))
}

#[tauri::command]
pub fn install_loader(m: Mgr<'_>, loader: String, mc_version: String, loader_version: String, java_path: Option<String>) -> Result<String, DlError> {
    use crate::settings::guard::{check_mc_version, check_version_token};
    check_mc_version(&mc_version).map_err(DlError::Other)?;
    check_version_token("加载器版本", &loader_version).map_err(DlError::Other)?;
    // The forge/neoforge installer runs `java` with processors from the downloaded jar: the
    // executable must be a real Java, never an arbitrary program picked by the UI.
    if let Some(j) = java_path.as_deref().filter(|j| !j.is_empty()) {
        crate::java::check_java_exe(j).map_err(DlError::Other)?;
    }
    let mgr: Arc<Manager> = m.inner().clone();
    let label = format!("{loader} {loader_version} for {mc_version}");
    let kind = loader.clone();
    let (mc, lv) = (mc_version, loader_version);
    let l = loader.clone();
    if !["fabric", "quilt", "forge", "neoforge"].contains(&l.as_str()) {
        return Err(DlError::Other(format!("unknown loader {loader}")));
    }
    Ok(m.spawn(&kind, &label, move |ctx| {
        let mgr = mgr.clone();
        async move {
            match l.as_str() {
                "fabric" => loaders::install_profile_loader(&ctx, &mgr, loaders::ProfileLoader::Fabric, &mc, &lv).await.map(|_| ()),
                "quilt" => loaders::install_profile_loader(&ctx, &mgr, loaders::ProfileLoader::Quilt, &mc, &lv).await.map(|_| ()),
                "forge" => forge::install_forge_like(&ctx, &mgr, forge::Flavor::Forge, &mc, &lv, java_path).await.map(|_| ()),
                _ => forge::install_forge_like(&ctx, &mgr, forge::Flavor::NeoForge, &mc, &lv, java_path).await.map(|_| ()),
            }
        }
    }))
}

#[tauri::command]
pub async fn install_list_loader_versions(m: Mgr<'_>, loader: String, mc_version: String) -> Result<Vec<loaders::LoaderVersion>, DlError> {
    crate::settings::guard::check_mc_version(&mc_version).map_err(DlError::Other)?;
    loaders::list_loader_versions(&m, &loader, &mc_version).await
}

#[tauri::command]
pub async fn install_list_optifine(m: Mgr<'_>, mc_version: String) -> Result<Vec<loaders::OptifineVersion>, DlError> {
    crate::settings::guard::check_mc_version(&mc_version).map_err(DlError::Other)?;
    loaders::list_optifine(&m, &mc_version).await
}

/// Download an OptiFine jar into `mods_dir` (for use with Forge). Standalone OptiFine install is not implemented.
#[tauri::command]
pub fn install_optifine_mod(m: Mgr<'_>, mc_version: String, kind: String, patch: String, mods_dir: PathBuf) -> Result<String, DlError> {
    use crate::settings::guard;
    guard::check_mc_version(&mc_version).map_err(DlError::Other)?;
    guard::check_version_token("OptiFine 类型", &kind).map_err(DlError::Other)?;
    guard::check_version_token("OptiFine 补丁", &patch).map_err(DlError::Other)?;
    let (mods_dir, depth) = guard::confine_content(&guard::registered_roots(), &mods_dir).map_err(DlError::Other)?;
    if depth != 0 || mods_dir.file_name().map(|n| n != "mods").unwrap_or(true) {
        return Err(DlError::Other(format!("OptiFine 只能安装到 mods 文件夹：{}", mods_dir.display())));
    }
    let label = format!("OptiFine {mc_version} {kind} {patch}");
    Ok(m.spawn("optifine", &label, move |ctx| async move {
        loaders::install_optifine_mod(&ctx, &mc_version, &kind, &patch, mods_dir).await.map(|_| ())
    }))
}

/// Which installed version ids exist under the configured game dir.
#[tauri::command]
pub fn install_list_installed(m: Mgr<'_>) -> Vec<String> {
    let dir = m.cfg().game_dir.join("versions");
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .map(|r| r.flatten().filter(|e| e.path().join(format!("{}.json", e.file_name().to_string_lossy())).is_file()).map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    v.sort();
    v
}
