//! `app_install_resource`: pick a compatible version, resolve required dependencies and download
//! into the right folder of an instance.

use super::error::{from_resource_error, AppError, AppResult};
use super::instance::ResolvedInstance;
use super::AppCtx;
use crate::download::engine::FileSpec;
use crate::instances::detect::{validate_name, LoaderKind};
use crate::resources::types::{DependencyKind, ModLoader, Platform, ResourceFile, ResourceKind};
use crate::resources::{curseforge, modrinth};
use serde::Serialize;
use serde_json::json;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const MAX_DEPTH: usize = 6;
const MAX_FILES: usize = 60;

/// The remote side, abstracted so selection and dependency logic is testable offline.
#[async_trait::async_trait]
pub trait ResourceApi: Send + Sync {
    async fn versions(&self, platform: Platform, project_id: &str, mc: Option<&str>, loader: Option<ModLoader>) -> Result<Vec<ResourceFile>, String>;
    /// One concrete version (Modrinth version id / CurseForge file id).
    async fn version(&self, platform: Platform, id: &str) -> Result<ResourceFile, String>;
    /// A downloadable URL for files whose listing carries none.
    async fn download_url(&self, file: &ResourceFile) -> Result<String, String>;
    /// CurseForge files by id (deleted ones are omitted).
    async fn curseforge_files(&self, ids: &[u64]) -> Result<Vec<ResourceFile>, String>;
}

pub struct LiveApi;

#[async_trait::async_trait]
impl ResourceApi for LiveApi {
    async fn versions(&self, platform: Platform, project_id: &str, mc: Option<&str>, loader: Option<ModLoader>) -> Result<Vec<ResourceFile>, String> {
        match platform {
            Platform::Modrinth => modrinth::versions(project_id, mc, loader).await,
            Platform::CurseForge => curseforge::files(project_id, mc, loader).await,
        }
    }

    async fn version(&self, platform: Platform, id: &str) -> Result<ResourceFile, String> {
        match platform {
            Platform::Modrinth => modrinth::version_by_id(id).await,
            Platform::CurseForge => {
                let fid: u64 = id.parse().map_err(|_| format!("invalid CurseForge file id {id}"))?;
                curseforge::files_by_ids(&[fid]).await?.into_iter().next().ok_or_else(|| format!("CurseForge file {id} not found"))
            }
        }
    }

    async fn curseforge_files(&self, ids: &[u64]) -> Result<Vec<ResourceFile>, String> {
        let mut out = Vec::new();
        for chunk in ids.chunks(200) {
            out.extend(curseforge::files_by_ids(chunk).await?);
        }
        Ok(out)
    }

    async fn download_url(&self, file: &ResourceFile) -> Result<String, String> {
        match file.platform {
            Platform::Modrinth => file.download_urls.first().cloned().ok_or_else(|| "no download url".to_string()),
            Platform::CurseForge => {
                let fid: u64 = file.id.parse().map_err(|_| "invalid file id".to_string())?;
                curseforge::resolve_download_url(&file.project_id, fid, &file.file_name).await
            }
        }
    }
}

/// Subfolder of the game dir for a resource kind. Data packs live inside a world.
pub fn target_dir(game_dir: &Path, kind: ResourceKind, world: Option<&str>) -> AppResult<PathBuf> {
    Ok(match kind {
        ResourceKind::Mod => game_dir.join("mods"),
        ResourceKind::ResourcePack => game_dir.join("resourcepacks"),
        ResourceKind::Shader => game_dir.join("shaderpacks"),
        ResourceKind::DataPack => {
            let w = world.map(str::trim).filter(|w| !w.is_empty()).ok_or_else(|| AppError::invalid("安装数据包需要指定世界名称（worldName）"))?;
            validate_name(w).map_err(AppError::invalid)?;
            let dir = game_dir.join("saves").join(w);
            if !dir.is_dir() {
                return Err(AppError::new("WORLD_NOT_FOUND", format!("找不到存档：{w}")).with(json!({ "worldName": w })));
            }
            dir.join("datapacks")
        }
        ResourceKind::Modpack | ResourceKind::World => return Err(AppError::invalid("该资源类型请使用 app_install_modpack 或手动导入")),
    })
}

/// Loader to filter mod versions by, from the instance's detected loaders.
pub fn instance_mod_loader(inst: &ResolvedInstance) -> Option<ModLoader> {
    inst.info.loaders.iter().find_map(|l| match l.kind {
        LoaderKind::Fabric => Some(ModLoader::Fabric),
        LoaderKind::Quilt => Some(ModLoader::Quilt),
        LoaderKind::Forge => Some(ModLoader::Forge),
        LoaderKind::NeoForge => Some(ModLoader::NeoForge),
        _ => None,
    })
}

/// A file name from a remote API must stay a plain file name.
pub fn safe_file_name(name: &str) -> AppResult<String> {
    let n = name.trim();
    let bad = n.is_empty() || n == "." || n == ".." || n.contains(['/', '\\', '\0']) || n.chars().any(|c| c.is_control());
    if bad {
        return Err(AppError::new("NETWORK", format!("资源平台返回了不安全的文件名：{name:?}")));
    }
    Ok(n.to_string())
}

fn release_rank(t: &str) -> u8 {
    match t {
        "release" => 0,
        "beta" => 1,
        _ => 2,
    }
}

pub fn compatible(f: &ResourceFile, mc: &str, loader: Option<ModLoader>) -> bool {
    (f.game_versions.is_empty() || f.game_versions.iter().any(|v| v == mc)) && loader.is_none_or(|l| f.loaders.is_empty() || f.loaders.contains(&l))
}

/// Best version: compatible first, then release over beta over alpha, then newest.
pub fn pick_best(files: &[ResourceFile], mc: &str, loader: Option<ModLoader>) -> Option<ResourceFile> {
    files
        .iter()
        .filter(|f| compatible(f, mc, loader))
        .min_by(|a, b| {
            release_rank(&a.release_type)
                .cmp(&release_rank(&b.release_type))
                .then_with(|| b.date_published.cmp(&a.date_published))
        })
        .cloned()
}

/// Newest release (any Minecraft version), used for modpack projects.
pub fn pick_latest(files: &[ResourceFile]) -> Option<ResourceFile> {
    files.iter().min_by(|a, b| release_rank(&a.release_type).cmp(&release_rank(&b.release_type)).then_with(|| b.date_published.cmp(&a.date_published))).cloned()
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlannedFile {
    pub project_id: String,
    pub file_id: String,
    pub file_name: String,
    pub path: String,
    #[serde(skip)]
    pub urls: Vec<String>,
    pub sha1: Option<String>,
    pub size: u64,
    pub is_dependency: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SkippedDependency {
    pub project_id: String,
    pub reason: String,
}

#[derive(Debug, Default)]
pub struct Plan {
    pub files: Vec<PlannedFile>,
    pub skipped: Vec<SkippedDependency>,
}

pub struct PlanInput<'a> {
    pub platform: Platform,
    pub project_id: &'a str,
    pub version_id: Option<&'a str>,
    pub kind: ResourceKind,
    pub mc: &'a str,
    pub loader: Option<ModLoader>,
    pub dest_dir: &'a Path,
}

async fn to_planned(api: &dyn ResourceApi, f: &ResourceFile, dir: &Path, dep: bool) -> AppResult<PlannedFile> {
    let name = safe_file_name(&f.file_name)?;
    let mut urls = f.download_urls.clone();
    if urls.is_empty() {
        urls.push(api.download_url(f).await.map_err(|e| {
            AppError::new("MANUAL_DOWNLOAD_REQUIRED", format!("{name} 不允许第三方下载，请在平台页面手动下载：{e}"))
                .with(json!({ "projectId": f.project_id, "fileId": f.id, "fileName": name }))
        })?);
    }
    Ok(PlannedFile {
        project_id: f.project_id.clone(),
        file_id: f.id.clone(),
        path: dir.join(&name).to_string_lossy().into_owned(),
        file_name: name,
        urls,
        sha1: f.sha1.clone().filter(|s| !s.is_empty()),
        size: f.size,
        is_dependency: dep,
    })
}

/// Candidate versions for a project; Quilt instances also accept Fabric builds.
async fn candidates(api: &dyn ResourceApi, platform: Platform, project: &str, mc: &str, loader: Option<ModLoader>) -> AppResult<Vec<ResourceFile>> {
    let mut all = api.versions(platform, project, Some(mc), loader).await.map_err(from_resource_error)?;
    if loader == Some(ModLoader::Quilt) && pick_best(&all, mc, loader).is_none() {
        all.extend(api.versions(platform, project, Some(mc), Some(ModLoader::Fabric)).await.map_err(from_resource_error)?);
        for f in all.iter_mut() {
            if f.loaders.contains(&ModLoader::Fabric) && !f.loaders.contains(&ModLoader::Quilt) {
                f.loaders.push(ModLoader::Quilt);
            }
        }
    }
    Ok(all)
}

pub async fn plan_install(api: &dyn ResourceApi, inp: &PlanInput<'_>) -> AppResult<Plan> {
    let mod_loader = (inp.kind == ResourceKind::Mod).then_some(inp.loader).flatten();
    let no_version = |what: &str| {
        AppError::new("NO_COMPATIBLE_VERSION", format!("{what} 没有适用于 Minecraft {} 的版本", inp.mc))
            .with(json!({ "projectId": inp.project_id, "mcVersion": inp.mc, "loader": mod_loader }))
    };
    let main = match inp.version_id {
        Some(v) => {
            let f = api.version(inp.platform, v).await.map_err(from_resource_error)?;
            if f.project_id != inp.project_id && !f.project_id.is_empty() {
                return Err(AppError::invalid("versionId 不属于该项目"));
            }
            f
        }
        None => {
            let all = candidates(api, inp.platform, inp.project_id, inp.mc, mod_loader).await?;
            pick_best(&all, inp.mc, mod_loader).ok_or_else(|| no_version("该资源"))?
        }
    };
    let mut plan = Plan::default();
    let mut seen: HashSet<String> = HashSet::from([inp.project_id.to_string(), main.project_id.clone()]);
    plan.files.push(to_planned(api, &main, inp.dest_dir, false).await?);
    if inp.kind != ResourceKind::Mod {
        return Ok(plan);
    }
    // Breadth-first over required dependencies.
    let mut queue: Vec<(ResourceFile, usize)> = vec![(main, 0)];
    while let Some((file, depth)) = queue.pop() {
        if depth >= MAX_DEPTH {
            continue;
        }
        for dep in file.dependencies.iter().filter(|d| d.kind == DependencyKind::Required) {
            let pid = dep.project_id.clone();
            let label = pid.clone().or_else(|| dep.file_id.clone()).unwrap_or_default();
            let resolved: AppResult<ResourceFile> = async {
                // An exact version the author pinned wins when it fits this instance.
                if let Some(fid) = &dep.file_id
                    && let Ok(f) = api.version(file.platform, fid).await
                    && compatible(&f, inp.mc, mod_loader)
                {
                    return Ok(f);
                }
                let pid = pid.as_deref().ok_or_else(|| no_version("依赖项"))?;
                let all = candidates(api, file.platform, pid, inp.mc, mod_loader).await?;
                pick_best(&all, inp.mc, mod_loader).ok_or_else(|| no_version(&format!("依赖项 {pid}")))
            }
            .await;
            match resolved {
                Ok(f) => {
                    if !seen.insert(f.project_id.clone()) {
                        continue;
                    }
                    if plan.files.len() >= MAX_FILES {
                        plan.skipped.push(SkippedDependency { project_id: label, reason: "依赖数量过多".into() });
                        continue;
                    }
                    match to_planned(api, &f, inp.dest_dir, true).await {
                        Ok(p) => {
                            plan.files.push(p);
                            queue.push((f, depth + 1));
                        }
                        Err(e) => plan.skipped.push(SkippedDependency { project_id: label, reason: e.message }),
                    }
                }
                Err(e) => plan.skipped.push(SkippedDependency { project_id: label, reason: e.message }),
            }
        }
    }
    Ok(plan)
}

pub struct ResourceRequest {
    pub platform: Platform,
    pub project_id: String,
    pub version_id: Option<String>,
    pub kind: ResourceKind,
    pub instance_id: String,
    pub world_name: Option<String>,
}

fn validate_id(what: &str, s: &str) -> AppResult<()> {
    let ok = !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    ok.then_some(()).ok_or_else(|| AppError::invalid(format!("{what} 不合法")))
}

/// Resolve everything up front (so version/dependency problems come back as typed errors), then
/// download as a manager task.
pub async fn install_resource(ctx: &AppCtx, api: &dyn ResourceApi, req: ResourceRequest) -> AppResult<String> {
    validate_id("projectId", &req.project_id)?;
    if let Some(v) = &req.version_id {
        validate_id("versionId", v)?;
    }
    let g = ctx.settings.get();
    let inst = super::instance::resolve(&g, &req.instance_id)?;
    let dest_dir = target_dir(&inst.game_dir, req.kind, req.world_name.as_deref())?;
    let loader = instance_mod_loader(&inst);
    if req.kind == ResourceKind::Mod && loader.is_none() {
        return Err(AppError::new("INSTANCE_NOT_MODDABLE", "该实例没有安装 Forge / Fabric / Quilt / NeoForge，无法安装 Mod"));
    }
    let mc = inst.info.mc_version.clone().ok_or_else(|| AppError::new("INSTANCE_INVALID", "无法确定实例的 Minecraft 版本"))?;
    let plan = plan_install(api, &PlanInput {
        platform: req.platform,
        project_id: &req.project_id,
        version_id: req.version_id.as_deref(),
        kind: req.kind,
        mc: &mc,
        loader,
        dest_dir: &dest_dir,
    })
    .await?;
    tracing::info!(files = plan.files.len(), skipped = plan.skipped.len(), dest = %dest_dir.display(), "resource install planned");
    let label = match plan.files.as_slice() {
        [one] => one.file_name.clone(),
        [first, rest @ ..] => format!("{} 等 {} 个文件", first.file_name, rest.len() + 1),
        [] => "资源".into(),
    };
    let emit = ctx.emit.clone();
    let id = ctx.mgr.spawn("resource", &label, move |t| async move {
        let specs: Vec<FileSpec> = plan
            .files
            .iter()
            .map(|f| {
                let mut s = FileSpec::new(f.urls[0].clone(), &f.path).with_sha1(f.sha1.clone()).with_size(Some(f.size));
                s.urls = f.urls.clone();
                s
            })
            .collect();
        if let Some(dir) = specs.first().and_then(|s| s.dest.parent()) {
            tokio::fs::create_dir_all(dir).await?;
        }
        let bytes = specs.iter().filter_map(|s| s.size).sum();
        t.tracker.begin_stage("download", 0.0, 1.0, specs.len() as u64, bytes);
        t.batch(specs).await?;
        emit("app://resource-report", json!({ "taskId": t.tracker.task_id, "installed": plan.files, "skippedDependencies": plan.skipped }));
        Ok(())
    });
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::types::Dependency;
    use std::collections::HashMap;
    use std::sync::Mutex;

    fn rf(platform: Platform, id: &str, project: &str, name: &str) -> ResourceFile {
        ResourceFile {
            platform, id: id.into(), project_id: project.into(), name: name.into(), file_name: name.into(), version_number: None,
            release_type: "release".into(), game_versions: vec!["1.20.1".into()], loaders: vec![ModLoader::Fabric], sha1: Some(format!("sha-{id}")),
            size: 10, download_urls: vec![format!("https://cdn.example/{name}")], date_published: "2024-01-01T00:00:00Z".into(), dependencies: vec![], fingerprint: None,
        }
    }

    #[derive(Default)]
    struct Fake {
        by_project: HashMap<String, Vec<ResourceFile>>,
        by_id: HashMap<String, ResourceFile>,
        calls: Mutex<Vec<String>>,
    }

    #[async_trait::async_trait]
    impl ResourceApi for Fake {
        async fn versions(&self, _p: Platform, project: &str, mc: Option<&str>, loader: Option<ModLoader>) -> Result<Vec<ResourceFile>, String> {
            self.calls.lock().unwrap().push(format!("versions:{project}:{}:{:?}", mc.unwrap_or("-"), loader));
            Ok(self.by_project.get(project).cloned().unwrap_or_default())
        }
        async fn version(&self, _p: Platform, id: &str) -> Result<ResourceFile, String> {
            self.by_id.get(id).cloned().ok_or_else(|| "not found".into())
        }
        async fn download_url(&self, f: &ResourceFile) -> Result<String, String> {
            Err(format!("no cdn for {}", f.file_name))
        }
        async fn curseforge_files(&self, _: &[u64]) -> Result<Vec<ResourceFile>, String> {
            Ok(vec![])
        }
    }

    fn input<'a>(project: &'a str, kind: ResourceKind, loader: Option<ModLoader>, dest: &'a Path) -> PlanInput<'a> {
        PlanInput { platform: Platform::Modrinth, project_id: project, version_id: None, kind, mc: "1.20.1", loader, dest_dir: dest }
    }

    #[test]
    fn picks_release_then_newest_and_checks_compat() {
        let mut beta = rf(Platform::Modrinth, "b", "p", "beta.jar");
        beta.release_type = "beta".into();
        beta.date_published = "2025-01-01T00:00:00Z".into();
        let mut old = rf(Platform::Modrinth, "o", "p", "old.jar");
        old.date_published = "2023-01-01T00:00:00Z".into();
        let new = rf(Platform::Modrinth, "n", "p", "new.jar");
        let mut wrong_mc = rf(Platform::Modrinth, "w", "p", "w.jar");
        wrong_mc.game_versions = vec!["1.19.4".into()];
        wrong_mc.date_published = "2030-01-01T00:00:00Z".into();
        let mut forge = rf(Platform::Modrinth, "f", "p", "f.jar");
        forge.loaders = vec![ModLoader::Forge];
        forge.date_published = "2031-01-01T00:00:00Z".into();
        let all = vec![beta, old, new, wrong_mc, forge];
        assert_eq!(pick_best(&all, "1.20.1", Some(ModLoader::Fabric)).unwrap().id, "n");
        assert_eq!(pick_best(&all, "1.20.1", Some(ModLoader::Forge)).unwrap().id, "f");
        assert!(pick_best(&all, "1.12.2", None).is_none());
        assert_eq!(pick_latest(&all).unwrap().id, "f");
    }

    #[test]
    fn target_dirs() {
        let d = tempfile::tempdir().unwrap();
        let g = d.path();
        assert_eq!(target_dir(g, ResourceKind::Mod, None).unwrap(), g.join("mods"));
        assert_eq!(target_dir(g, ResourceKind::ResourcePack, None).unwrap(), g.join("resourcepacks"));
        assert_eq!(target_dir(g, ResourceKind::Shader, None).unwrap(), g.join("shaderpacks"));
        assert_eq!(target_dir(g, ResourceKind::DataPack, None).unwrap_err().code, "INVALID_ARGUMENT");
        assert_eq!(target_dir(g, ResourceKind::DataPack, Some("W")).unwrap_err().code, "WORLD_NOT_FOUND");
        assert_eq!(target_dir(g, ResourceKind::DataPack, Some("../x")).unwrap_err().code, "INVALID_ARGUMENT");
        std::fs::create_dir_all(g.join("saves/My World")).unwrap();
        assert_eq!(target_dir(g, ResourceKind::DataPack, Some("My World")).unwrap(), g.join("saves/My World/datapacks"));
        assert!(target_dir(g, ResourceKind::Modpack, None).is_err());
    }

    #[test]
    fn file_names_cannot_escape() {
        assert_eq!(safe_file_name("a.jar").unwrap(), "a.jar");
        for bad in ["", "..", "../a.jar", "a/b.jar", "a\\b.jar", "a\0.jar"] {
            assert!(safe_file_name(bad).is_err(), "{bad:?}");
        }
    }

    #[tokio::test]
    async fn resolves_required_dependencies_once() {
        let mut main = rf(Platform::Modrinth, "m1", "main", "main.jar");
        main.dependencies = vec![
            Dependency { project_id: Some("lib".into()), file_id: None, kind: DependencyKind::Required },
            Dependency { project_id: Some("opt".into()), file_id: None, kind: DependencyKind::Optional },
            Dependency { project_id: Some("main".into()), file_id: None, kind: DependencyKind::Required },
        ];
        let mut lib = rf(Platform::Modrinth, "l1", "lib", "lib.jar");
        lib.dependencies = vec![Dependency { project_id: Some("main".into()), file_id: None, kind: DependencyKind::Required }, Dependency { project_id: Some("gone".into()), file_id: None, kind: DependencyKind::Required }];
        let mut api = Fake::default();
        api.by_project.insert("main".into(), vec![main]);
        api.by_project.insert("lib".into(), vec![lib]);
        let dest = Path::new("/g/mods");
        let plan = plan_install(&api, &input("main", ResourceKind::Mod, Some(ModLoader::Fabric), dest)).await.unwrap();
        let names: Vec<_> = plan.files.iter().map(|f| f.file_name.as_str()).collect();
        assert_eq!(names, vec!["main.jar", "lib.jar"]);
        assert!(!plan.files[0].is_dependency && plan.files[1].is_dependency);
        assert_eq!(plan.files[1].path, "/g/mods/lib.jar");
        assert_eq!(plan.skipped.len(), 1);
        assert_eq!(plan.skipped[0].project_id, "gone");
        assert!(!api.calls.lock().unwrap().iter().any(|c| c.contains("opt")), "optional deps are not fetched");
    }

    #[tokio::test]
    async fn non_mod_kinds_skip_dependencies_and_loader_filter() {
        let mut p = rf(Platform::Modrinth, "s1", "shader", "shader.zip");
        p.loaders = vec![];
        p.dependencies = vec![Dependency { project_id: Some("iris".into()), file_id: None, kind: DependencyKind::Required }];
        let mut api = Fake::default();
        api.by_project.insert("shader".into(), vec![p]);
        let plan = plan_install(&api, &input("shader", ResourceKind::Shader, Some(ModLoader::Fabric), Path::new("/g/shaderpacks"))).await.unwrap();
        assert_eq!(plan.files.len(), 1);
        assert!(api.calls.lock().unwrap()[0].ends_with(":None"), "loader is not sent for shaders: {:?}", api.calls.lock().unwrap());
    }

    #[tokio::test]
    async fn typed_errors() {
        let mut api = Fake::default();
        api.by_project.insert("p".into(), vec![]);
        let e = plan_install(&api, &input("p", ResourceKind::Mod, Some(ModLoader::Fabric), Path::new("/m"))).await.unwrap_err();
        assert_eq!(e.code, "NO_COMPATIBLE_VERSION");
        // pinned version of another project is rejected
        api.by_id.insert("v".into(), rf(Platform::Modrinth, "v", "other", "x.jar"));
        let mut inp = input("p", ResourceKind::Mod, Some(ModLoader::Fabric), Path::new("/m"));
        inp.version_id = Some("v");
        assert_eq!(plan_install(&api, &inp).await.unwrap_err().code, "INVALID_ARGUMENT");
        // a file without any URL that the platform cannot resolve needs a manual download
        let mut nourl = rf(Platform::CurseForge, "9", "cf", "nourl.jar");
        nourl.download_urls.clear();
        api.by_project.insert("cf".into(), vec![nourl]);
        let mut inp = input("cf", ResourceKind::Mod, Some(ModLoader::Fabric), Path::new("/m"));
        inp.platform = Platform::CurseForge;
        assert_eq!(plan_install(&api, &inp).await.unwrap_err().code, "MANUAL_DOWNLOAD_REQUIRED");
    }

    #[tokio::test]
    async fn quilt_accepts_fabric_builds() {
        let fabric_only = rf(Platform::Modrinth, "f", "p", "fab.jar");
        struct Q(ResourceFile);
        #[async_trait::async_trait]
        impl ResourceApi for Q {
            async fn versions(&self, _: Platform, _: &str, _: Option<&str>, l: Option<ModLoader>) -> Result<Vec<ResourceFile>, String> {
                Ok(if l == Some(ModLoader::Fabric) { vec![self.0.clone()] } else { vec![] })
            }
            async fn version(&self, _: Platform, _: &str) -> Result<ResourceFile, String> { Err("x".into()) }
            async fn download_url(&self, _: &ResourceFile) -> Result<String, String> { Err("x".into()) }
            async fn curseforge_files(&self, _: &[u64]) -> Result<Vec<ResourceFile>, String> { Ok(vec![]) }
        }
        let plan = plan_install(&Q(fabric_only), &input("p", ResourceKind::Mod, Some(ModLoader::Quilt), Path::new("/m"))).await.unwrap();
        assert_eq!(plan.files[0].file_name, "fab.jar");
    }
}
