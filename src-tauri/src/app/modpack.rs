//! `app_install_modpack`: the real [`ModpackInstaller`] on top of the download and install engines.
//!
//! Flow: parse archive (or download it from the platform) -> resolve CurseForge references ->
//! create `versions/<name>` with game + loader (via `install::*`) -> extract overrides -> download
//! files with SHA-1 verification -> register the instance. Files the platform refuses to serve
//! are reported (`app://modpack-report`) instead of failing the install.

use super::AppCtx;
use super::error::{AppError, AppResult, from_resource_error, into_task_error};
use super::install::{self, LoaderChoice};
use super::resource::{self, ResourceApi};
use crate::download::engine::FileSpec;
use crate::download::error::{DlError, DlResult};
use crate::download::{Manager, SpawnOpts, TaskCtx};
use crate::instances::detect::validate_name;
use crate::modpack::{self as mp, LoaderSpec, ModpackInstallPlan, ModpackInstaller, PlanFile};
use crate::resources::types::{ModLoader, Platform};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ModpackSource {
    File {
        path: String,
    },
    Project {
        platform: Platform,
        #[serde(rename = "projectId")]
        project_id: String,
        #[serde(rename = "versionId")]
        version_id: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MissingFile {
    pub path: String,
    pub project_id: u64,
    pub file_id: u64,
    pub page_url: String,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ModpackReport {
    pub instance_id: String,
    pub missing_files: Vec<MissingFile>,
    pub warnings: Vec<String>,
}

pub fn loader_choice(l: &LoaderSpec) -> LoaderChoice {
    LoaderChoice {
        kind: match l.loader {
            ModLoader::Fabric => "fabric",
            ModLoader::Quilt => "quilt",
            ModLoader::Forge => "forge",
            ModLoader::NeoForge => "neoforge",
        }
        .into(),
        version: l.version.clone(),
    }
}

/// Engine-backed installer. `install_game` installs into `<root>/versions/<instance_name>` and
/// records the produced version id; `download_files` verifies hashes through the download engine.
pub struct EngineInstaller {
    pub ctx: TaskCtx,
    pub mgr: Arc<Manager>,
    pub root: PathBuf,
    pub instance_name: String,
    pub java: Option<String>,
    pub unresolved: Mutex<Vec<PlanFile>>,
}

#[async_trait::async_trait]
impl ModpackInstaller for EngineInstaller {
    async fn download_files(&self, instance_dir: &Path, files: &[PlanFile]) -> Result<(), String> {
        let mut specs = Vec::new();
        for f in files {
            if f.urls.is_empty() {
                self.unresolved.lock().unwrap().push(f.clone());
                continue;
            }
            // Never trust a path from the pack: it must stay inside the instance directory.
            let dest = mp::safe_join(instance_dir, &f.path)?;
            let mut s = FileSpec::new(f.urls[0].clone(), dest).with_sha1(f.sha1.clone()).with_size(f.size);
            s.urls = f.urls.clone();
            specs.push(s);
        }
        let bytes = specs.iter().filter_map(|s| s.size).sum();
        self.ctx.tracker.begin_stage("pack-files", 0.0, 1.0, specs.len() as u64, bytes);
        self.ctx.batch(specs).await.map_err(|e| e.to_string())
    }

    async fn install_game(&self, _instance_dir: &Path, mc_version: &str, loaders: &[LoaderSpec]) -> Result<(), String> {
        let choice = loaders.first().map(loader_choice);
        if loaders.len() > 1 {
            tracing::warn!("modpack lists {} loaders, installing only the first", loaders.len());
        }
        let pre_existed = crate::install::vanilla::version_json_path(&self.root, mc_version).is_file();
        self.ctx.tracker.set_scale(0.0, 0.5);
        let produced = install::install_version(&self.ctx, &self.mgr, mc_version, choice.as_ref(), self.java.clone())
            .await
            .map_err(|e| e.to_string())?;
        self.ctx.tracker.set_scale(0.5, 0.5);
        let (root, name) = (self.root.clone(), self.instance_name.clone());
        tokio::task::spawn_blocking(move || install::finalize_name(&root, &produced, &name, pre_existed))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())
    }
}

/// Where an instance named `name` keeps its files (isolation is forced on: a pack owns its mods).
pub fn instance_dir(root: &Path, name: &str) -> PathBuf {
    root.join("versions").join(name)
}

fn force_isolation(dir: &Path) {
    let mut s = crate::settings::InstanceSettings::load(dir);
    s.isolated = Some(true);
    if let Err(e) = s.save(dir) {
        tracing::warn!("could not mark instance as isolated: {e}");
    }
}

pub fn page_url(r: &mp::CurseForgeRef) -> String {
    format!("https://www.curseforge.com/projects/{}", r.project_id)
}

/// A CurseForge file whose listing carries no download URL (the author disabled third-party
/// distribution). Downloading it is attempted on a best-effort basis; failures are reported.
#[derive(Debug, Clone)]
pub struct SoftFile {
    pub plan: PlanFile,
    pub remote: crate::resources::types::ResourceFile,
}

#[derive(Debug, Default)]
pub struct Resolved {
    /// Deleted upstream: dropped from the plan.
    pub gone: Vec<MissingFile>,
    /// Removed from `plan.files`; to be fetched after the regular files.
    pub soft: Vec<SoftFile>,
}

/// Resolve CurseForge references through the API.
pub async fn resolve_plan(api: &dyn ResourceApi, plan: &mut ModpackInstallPlan) -> AppResult<Resolved> {
    let ids: Vec<u64> = plan.files.iter().filter_map(|f| f.curseforge.as_ref().map(|c| c.file_id)).collect();
    if ids.is_empty() {
        return Ok(Resolved::default());
    }
    let found = api.curseforge_files(&ids).await.map_err(from_resource_error)?;
    let no_url: std::collections::HashMap<u64, crate::resources::types::ResourceFile> =
        found.iter().filter(|f| f.download_urls.is_empty()).filter_map(|f| Some((f.id.parse().ok()?, f.clone()))).collect();
    let gone = mp::curseforge_pack::apply_resolved(plan, found);
    let mut soft = Vec::new();
    plan.files.retain(|f| match f.curseforge.as_ref().and_then(|c| no_url.get(&c.file_id)) {
        Some(remote) => {
            soft.push(SoftFile { plan: f.clone(), remote: remote.clone() });
            false
        }
        None => true,
    });
    let gone = gone
        .iter()
        .map(|r| MissingFile { path: String::new(), project_id: r.project_id, file_id: r.file_id, page_url: page_url(r) })
        .collect();
    Ok(Resolved { gone, soft })
}

/// Try the files without an API URL one by one; whatever cannot be fetched is returned.
async fn fetch_soft(t: &TaskCtx, api: &dyn ResourceApi, dir: &Path, soft: Vec<SoftFile>) -> DlResult<Vec<MissingFile>> {
    let mut missing = Vec::new();
    t.tracker.begin_stage("pack-restricted", 0.0, 1.0, soft.len() as u64, 0);
    for s in soft {
        let c = s.plan.curseforge.clone().expect("soft files come from CurseForge refs");
        let miss = MissingFile { path: s.plan.path.clone(), project_id: c.project_id, file_id: c.file_id, page_url: page_url(&c) };
        let Ok(dest) = mp::safe_join(dir, &s.plan.path) else {
            missing.push(miss);
            continue;
        };
        let Ok(url) = api.download_url(&s.remote).await else {
            missing.push(miss);
            continue;
        };
        let spec = FileSpec::new(url, dest).with_sha1(s.plan.sha1.clone()).with_size(s.plan.size);
        match t.file(spec).await {
            Ok(()) => {}
            Err(DlError::Cancelled) => return Err(DlError::Cancelled),
            Err(e) => {
                tracing::warn!(file = %s.plan.path, "restricted file could not be downloaded: {e}");
                missing.push(miss);
            }
        }
    }
    Ok(missing)
}

/// Download the pack archive for a platform project into `dest`.
async fn fetch_pack(
    t: &TaskCtx,
    api: &dyn ResourceApi,
    platform: Platform,
    project: &str,
    version: Option<&str>,
    dest: &Path,
) -> AppResult<()> {
    let file = match version {
        Some(v) => api.version(platform, v).await.map_err(from_resource_error)?,
        None => {
            let all = api.versions(platform, project, None, None).await.map_err(from_resource_error)?;
            resource::pick_latest(&all).ok_or_else(|| AppError::new("NO_COMPATIBLE_VERSION", "该整合包没有可下载的版本"))?
        }
    };
    let name = resource::safe_file_name(&file.file_name)?;
    let url = match file.download_urls.first() {
        Some(u) => u.clone(),
        None => {
            api.download_url(&file).await.map_err(|e| AppError::new("MANUAL_DOWNLOAD_REQUIRED", format!("{name} 不允许第三方下载：{e}")))?
        }
    };
    t.tracker.begin_stage("pack-archive", 0.0, 1.0, 1, file.size);
    let mut spec = FileSpec::new(url, dest).with_sha1(file.sha1.clone()).with_size(Some(file.size).filter(|s| *s > 0));
    spec.urls.dedup();
    t.file(spec).await.map_err(AppError::from)
}

pub async fn install_modpack(ctx: &AppCtx, api: Arc<dyn ResourceApi>, source: ModpackSource, instance_name: String) -> AppResult<String> {
    let name = instance_name.trim().to_string();
    validate_name(&name).map_err(AppError::invalid)?;
    if let ModpackSource::Project { project_id, version_id, .. } = &source {
        for (what, s) in [("projectId", Some(project_id)), ("versionId", version_id.as_ref())] {
            if let Some(s) = s {
                let ok = !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
                if !ok {
                    return Err(AppError::invalid(format!("{what} 不合法")));
                }
            }
        }
    }
    let g = ctx.settings.get();
    let root = super::config::selected_folder(&g);
    if install::instance_exists(&root, &name) || instance_dir(&root, &name).exists() {
        return Err(AppError::new("INSTANCE_EXISTS", format!("已存在同名实例：{name}")).with(json!({ "name": name })));
    }
    // A local archive is parsed up front so format errors reach the caller immediately.
    let local = match &source {
        ModpackSource::File { path } => {
            let p = PathBuf::from(path);
            if !p.is_file() {
                return Err(AppError::invalid(format!("整合包文件不存在：{path}")));
            }
            let parsed = {
                let p = p.clone();
                tokio::task::spawn_blocking(move || mp::parse_pack(&p)).await.map_err(AppError::internal)?
            };
            Some((p, parsed.map_err(|e| AppError::new("UNSUPPORTED", format!("无法识别该整合包：{e}")))?))
        }
        ModpackSource::Project { .. } => None,
    };
    let java = match local.as_ref().map(|(_, plan)| plan) {
        Some(plan) if plan.loaders.iter().any(|l| matches!(l.loader, ModLoader::Forge | ModLoader::NeoForge)) => {
            Some(install::java_for_processors(&ctx.discover, ctx.settings, &plan.mc_version, &root).await?)
        }
        _ => None,
    };
    let cfg = crate::download::DownloadConfig { game_dir: root.clone(), ..ctx.mgr.cfg() };
    let (mgr, settings, emit, reports) = (ctx.mgr.clone(), ctx.settings, ctx.emit.clone(), ctx.reports.clone());
    let discover = ctx.discover.clone();
    let label = format!("整合包：{name}");
    tracing::info!(%name, source = ?source, "install modpack");
    let id = ctx.mgr.spawn_with(SpawnOpts { cfg: Some(cfg), on_progress: None }, "modpack", &label, move |t| async move {
        let to_dl = |e: AppError| into_task_error(e);
        let dir = instance_dir(&root, &name);
        let result = async {
            // 1. archive + plan
            t.tracker.set_scale(0.0, 0.05);
            let (archive, mut plan, temp) = match local {
                Some((p, plan)) => (p, plan, None),
                None => {
                    let ModpackSource::Project { platform, project_id, version_id } = &source else { unreachable!() };
                    let tmp = root.join(".pcl-tmp").join(format!("pack-{}", t.tracker.task_id));
                    tokio::fs::create_dir_all(&tmp).await?;
                    let archive = tmp.join("pack.zip");
                    fetch_pack(&t, api.as_ref(), *platform, project_id, version_id.as_deref(), &archive).await.map_err(to_dl)?;
                    let a = archive.clone();
                    let plan = tokio::task::spawn_blocking(move || mp::parse_pack(&a))
                        .await
                        .map_err(|e| DlError::Other(e.to_string()))?
                        .map_err(|e| to_dl(AppError::new("UNSUPPORTED", format!("无法识别该整合包：{e}"))))?;
                    (archive, plan, Some(tmp))
                }
            };
            t.tracker.begin_stage("resolve", 0.0, 1.0, 0, 0);
            let resolved = resolve_plan(api.as_ref(), &mut plan).await.map_err(to_dl)?;
            let mut missing = resolved.gone;
            // Packs created for another Java edition of Forge etc. need a Java for processors.
            let java = match java {
                Some(j) => Some(j),
                None if plan.loaders.iter().any(|l| matches!(l.loader, ModLoader::Forge | ModLoader::NeoForge)) => {
                    Some(install::java_for_processors(&discover, settings, &plan.mc_version, &root).await.map_err(to_dl)?)
                }
                None => None,
            };
            let optional: Vec<String> = plan.files.iter().filter(|f| f.optional).map(|f| f.path.clone()).collect();
            if !optional.is_empty() {
                plan.warnings.push(format!("已跳过 {} 个可选文件", optional.len()));
            }
            // 2. game, overrides, files
            t.tracker.set_scale(0.05, 0.95);
            let installer = EngineInstaller { ctx: t.clone(), mgr: mgr.clone(), root: root.clone(), instance_name: name.clone(), java, unresolved: Mutex::default() };
            mp::installer::install(&archive, &plan, &dir, &[], &installer).await.map_err(|e| match e.strip_prefix("Other: ") {
                Some(rest) => DlError::Other(rest.to_string()),
                None => DlError::Other(e),
            })?;
            // 3. files the platform does not hand out: best effort, the rest goes into the report
            missing.extend(fetch_soft(&t, api.as_ref(), &dir, resolved.soft).await?);
            force_isolation(&dir);
            install::register_instance(settings, &root, &name);
            if let Some(tmp) = temp {
                let _ = tokio::fs::remove_dir_all(tmp).await;
            }
            let report = ModpackReport { instance_id: dir.to_string_lossy().into_owned(), missing_files: missing, warnings: plan.warnings.clone() };
            emit("app://modpack-report", json!({ "taskId": t.tracker.task_id, "instanceId": report.instance_id, "missingFiles": report.missing_files, "warnings": report.warnings }));
            reports.lock().unwrap().insert(t.tracker.task_id.clone(), report);
            t.tracker.begin_stage("done", 1.0, 0.0, 0, 0);
            Ok::<(), DlError>(())
        }
        .await;
        if result.is_err() {
            // Leave no half-built instance behind (the version folder only exists because of this task).
            let _ = tokio::fs::remove_dir_all(&dir).await;
            let _ = tokio::fs::remove_dir_all(root.join(".pcl-tmp").join(format!("pack-{}", t.tracker.task_id))).await;
        }
        result
    });
    Ok(id)
}
