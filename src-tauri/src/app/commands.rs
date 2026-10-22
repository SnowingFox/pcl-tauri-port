//! `#[tauri::command]` surface of the orchestration layer. See `docs/api/app.md`.

use super::error::{AppError, AppResult};
use super::install::LoaderChoice;
use super::modpack::ModpackSource;
use super::resource::{LiveApi, ResourceRequest};
use super::{instance, saves, AppCtx};
use crate::resources::local_mods::{self, LocalMod};
use crate::resources::types::{Platform, ResourceKind};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

pub type Ctx<'a> = State<'a, Arc<AppCtx>>;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchStarted {
    pub launch_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskStarted {
    pub task_id: String,
}

/// Launch `instance_id` with `account_id`. Resolves when the game process has been spawned; progress
/// arrives on `launch://progress` meanwhile. Pass `launch_id` to learn the id before the command
/// returns (it must be unique; a generated one is used otherwise).
#[tauri::command]
pub async fn app_launch(ctx: Ctx<'_>, instance_id: String, account_id: String, launch_id: Option<String>) -> AppResult<LaunchStarted> {
    let id = super::launch::launch(&ctx, &instance_id, &account_id, launch_id).await?;
    Ok(LaunchStarted { launch_id: id })
}

#[tauri::command]
pub fn app_launch_kill(launch_id: String) -> AppResult<()> {
    crate::launch::kill_by_id(&launch_id).map_err(AppError::from)
}

#[tauri::command]
pub async fn app_install_game(ctx: Ctx<'_>, mc_version: String, loader: Option<LoaderChoice>, instance_name: Option<String>) -> AppResult<TaskStarted> {
    let task_id = super::install::install_game(&ctx, mc_version, loader, instance_name).await?;
    Ok(TaskStarted { task_id })
}

#[tauri::command]
pub async fn app_install_resource(
    ctx: Ctx<'_>,
    platform: Platform,
    project_id: String,
    version_id: Option<String>,
    kind: ResourceKind,
    instance_id: String,
    world_name: Option<String>,
) -> AppResult<TaskStarted> {
    let req = ResourceRequest { platform, project_id, version_id, kind, instance_id, world_name };
    let task_id = super::resource::install_resource(&ctx, &LiveApi, req).await?;
    Ok(TaskStarted { task_id })
}

#[tauri::command]
pub async fn app_install_modpack(ctx: Ctx<'_>, source: ModpackSource, instance_name: String) -> AppResult<TaskStarted> {
    let task_id = super::modpack::install_modpack(&ctx, Arc::new(LiveApi), source, instance_name).await?;
    Ok(TaskStarted { task_id })
}

/// Report (missing manual downloads, warnings) of a finished modpack task, if any.
#[tauri::command]
pub fn app_modpack_report(ctx: Ctx<'_>, task_id: String) -> Option<super::modpack::ModpackReport> {
    ctx.reports.lock().unwrap().get(&task_id).cloned()
}

#[tauri::command]
pub async fn app_instance_mods(ctx: Ctx<'_>, instance_id: String) -> AppResult<Vec<LocalMod>> {
    let inst = instance::resolve(&ctx.settings.get(), &instance_id)?;
    let dir = inst.game_dir.join("mods");
    tokio::task::spawn_blocking(move || local_mods::scan_mods_dir(&dir))
        .await
        .map_err(AppError::internal)?
        .map_err(|e| AppError::new("IO", e))
}

#[tauri::command]
pub async fn app_instance_saves(ctx: Ctx<'_>, instance_id: String) -> AppResult<Vec<saves::SaveEntry>> {
    let inst = instance::resolve(&ctx.settings.get(), &instance_id)?;
    tokio::task::spawn_blocking(move || saves::list_saves(&inst.game_dir)).await.map_err(AppError::internal)
}

/// Only plain http(s) URLs are handed to the system.
pub fn validate_url(raw: &str) -> AppResult<url::Url> {
    let u = url::Url::parse(raw.trim()).map_err(|_| AppError::invalid("链接格式不正确"))?;
    if !matches!(u.scheme(), "http" | "https") || u.host_str().is_none() {
        return Err(AppError::invalid("只允许打开 http / https 链接"));
    }
    Ok(u)
}

#[tauri::command]
pub fn app_open_url(app: AppHandle, url: String) -> AppResult<()> {
    let u = validate_url(&url)?;
    app.opener().open_url(u.as_str(), None::<&str>).map_err(|e| AppError::new("IO", format!("无法打开链接：{e}")))
}

fn canon(p: &Path) -> Option<PathBuf> {
    p.canonicalize().ok()
}

/// Directories `app_reveal` may point at: registered Minecraft folders, the app data dir, and the
/// game dirs of their instances (always below a registered folder).
pub fn reveal_roots(g: &crate::settings::GlobalSettings) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = g.game.folders.iter().filter_map(|f| canon(Path::new(&f.path))).collect();
    v.extend(canon(&crate::settings::store::data_dir()));
    v
}

/// Canonical `path` if it exists and lies inside one of `roots`.
pub fn check_reveal(roots: &[PathBuf], path: &Path) -> AppResult<PathBuf> {
    if !path.is_absolute() {
        return Err(AppError::invalid("需要绝对路径"));
    }
    let real = path.canonicalize().map_err(|_| AppError::new("NOT_FOUND", format!("路径不存在：{}", path.display())))?;
    if roots.iter().any(|r| real.starts_with(r)) {
        Ok(real)
    } else {
        Err(AppError::forbidden(path))
    }
}

#[tauri::command]
pub fn app_reveal(app: AppHandle, path: String) -> AppResult<()> {
    let real = check_reveal(&reveal_roots(&crate::settings::commands::global().get()), Path::new(&path))?;
    app.opener().reveal_item_in_dir(&real).map_err(|e| AppError::new("IO", format!("无法在 Finder 中显示：{e}")))
}

#[derive(Serialize)]
pub struct DataDirs {
    pub data: String,
    pub logs: String,
}

#[tauri::command]
pub fn app_data_dirs() -> DataDirs {
    DataDirs {
        data: crate::settings::store::data_dir().to_string_lossy().into_owned(),
        logs: super::paths::logs_dir(&crate::settings::store::data_dir()).to_string_lossy().into_owned(),
    }
}
