//! `app_install_game`: vanilla / Fabric / Quilt / Forge / NeoForge into the selected game folder,
//! optionally under a custom instance name.

use super::AppCtx;
use super::error::{AppError, AppResult, into_task_error};
use super::tuning;
use crate::download::error::{DlError, DlResult};
use crate::download::{DownloadConfig, Manager, SpawnOpts, TaskCtx};
use crate::install::{forge, loaders, vanilla};
use crate::instances::detect::validate_name;
use crate::settings::store::SettingsStore;
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct LoaderChoice {
    /// `fabric` | `quilt` | `forge` | `neoforge`
    pub kind: String,
    pub version: String,
}

pub const LOADER_KINDS: [&str; 4] = ["fabric", "quilt", "forge", "neoforge"];

/// Loader versions end up in URLs and file names.
pub fn validate_token(what: &str, s: &str) -> AppResult<()> {
    let ok = !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-'));
    ok.then_some(()).ok_or_else(|| AppError::invalid(format!("{what} 不合法：{s}")))
}

pub fn validate_loader(l: &LoaderChoice) -> AppResult<()> {
    if !LOADER_KINDS.contains(&l.kind.as_str()) {
        return Err(AppError::invalid(format!("不支持的加载器：{}", l.kind)));
    }
    validate_token("加载器版本", &l.version)
}

pub fn instance_exists(root: &Path, name: &str) -> bool {
    vanilla::version_json_path(root, name).is_file()
}

/// Install `mc` (+ loader) under `ctx.cfg.game_dir`; returns the id of the version that was produced.
pub async fn install_version(
    t: &TaskCtx,
    mgr: &Arc<Manager>,
    mc: &str,
    loader: Option<&LoaderChoice>,
    java: Option<String>,
) -> DlResult<String> {
    let Some(l) = loader else {
        vanilla::install_vanilla(t, mgr, mc, 0.0, 1.0).await?;
        return Ok(mc.to_string());
    };
    match l.kind.as_str() {
        "fabric" => loaders::install_profile_loader(t, mgr, loaders::ProfileLoader::Fabric, mc, &l.version).await,
        "quilt" => loaders::install_profile_loader(t, mgr, loaders::ProfileLoader::Quilt, mc, &l.version).await,
        "forge" => forge::install_forge_like(t, mgr, forge::Flavor::Forge, mc, &l.version, java).await,
        "neoforge" => forge::install_forge_like(t, mgr, forge::Flavor::NeoForge, mc, &l.version, java).await,
        other => Err(DlError::Other(format!("INVALID_ARGUMENT: 不支持的加载器：{other}"))),
    }
}

/// Rewrite the `id` of a version json in place. A version that does not inherit from another one
/// (vanilla) loses its Minecraft version when renamed, so it is recorded as `clientVersion`
/// (the key the instances module and HMCL/PCL use for custom names).
fn rewrite_id(json_path: &Path, new_id: &str, old_id: &str) -> std::io::Result<Value> {
    let mut v: Value = serde_json::from_slice(&std::fs::read(json_path)?).map_err(std::io::Error::other)?;
    if v["inheritsFrom"].as_str().is_none_or(str::is_empty) && v.get("clientVersion").is_none() {
        v["clientVersion"] = json!(old_id);
    }
    v["id"] = json!(new_id);
    std::fs::write(json_path, serde_json::to_vec_pretty(&v).map_err(std::io::Error::other)?)?;
    Ok(v)
}

/// Make the installed version `produced` available as `name`.
///
/// * Loader versions (they `inheritsFrom` the vanilla one): the version folder is moved, files
///   named `<produced>.*` are renamed, the json id is rewritten.
/// * Vanilla: json + jar are copied to the new name. The original folder is removed again when
///   this install created it (`vanilla_pre_existed == false`), so no stray extra version remains.
pub fn finalize_name(root: &Path, produced: &str, name: &str, vanilla_pre_existed: bool) -> std::io::Result<()> {
    if produced == name {
        return Ok(());
    }
    let from = root.join("versions").join(produced);
    let to = root.join("versions").join(name);
    if vanilla::version_json_path(root, name).is_file() {
        return Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, format!("实例已存在：{name}")));
    }
    std::fs::create_dir_all(&to)?;
    let inherits = {
        let v: Value =
            serde_json::from_slice(&std::fs::read(vanilla::version_json_path(root, produced))?).map_err(std::io::Error::other)?;
        v["inheritsFrom"].as_str().is_some_and(|s| !s.is_empty())
    };
    let move_files = inherits || !vanilla_pre_existed;
    for e in std::fs::read_dir(&from)? {
        let e = e?;
        let fname = e.file_name().to_string_lossy().into_owned();
        let Some(rest) = fname.strip_prefix(produced) else { continue };
        let dest = to.join(format!("{name}{rest}"));
        if e.file_type()?.is_file() {
            if move_files {
                std::fs::rename(e.path(), &dest)?;
            } else {
                std::fs::copy(e.path(), &dest)?;
            }
        }
    }
    rewrite_id(&vanilla::version_json_path(root, name), name, produced)?;
    if move_files {
        // Remove the (now mostly empty) source folder; keep it if the user put other files there.
        let _ = std::fs::remove_dir(&from);
    }
    Ok(())
}

/// Make the instance the selected one so the instances list shows it first.
pub fn register_instance(settings: &SettingsStore, root: &Path, name: &str) {
    let root_s = root.to_string_lossy().into_owned();
    let res = settings.update(|g| {
        if !g.game.folders.iter().any(|f| f.path == root_s) {
            let n = root.file_name().and_then(|n| n.to_str()).unwrap_or("Minecraft").to_string();
            g.game.folders.push(crate::settings::McFolderEntry { name: n, path: root_s.clone() });
        }
        g.game.selected_folder = Some(root_s.clone());
        g.game.selected_instance = Some(name.to_string());
    });
    if let Err(e) = res {
        tracing::warn!("could not register instance {name}: {e}");
    }
}

/// Java for the Forge/NeoForge processors (they execute jars).
pub async fn java_for_processors(discover: &super::Discover, settings: &SettingsStore, mc: &str, root: &Path) -> AppResult<String> {
    let required = crate::java::default_major_for_mc(mc);
    let extra = [root.join("runtime")];
    let d = discover.clone();
    let found = tokio::task::spawn_blocking(move || d(&extra)).await.map_err(AppError::internal)?;
    let preferred: Vec<String> = settings.get().java.selected.clone().into_iter().collect();
    tuning::pick_java(&preferred, found, required, None).map(|(j, _)| j.path).map_err(|f| AppError::java_missing(required, None, &f))
}

/// Start the install task and return its id.
pub async fn install_game(
    ctx: &AppCtx,
    mc_version: String,
    loader: Option<LoaderChoice>,
    instance_name: Option<String>,
) -> AppResult<String> {
    validate_name(&mc_version).map_err(AppError::invalid)?;
    if let Some(l) = &loader {
        validate_loader(l)?;
    }
    let g = ctx.settings.get();
    let root = super::config::selected_folder(&g);
    let name = instance_name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
    if let Some(n) = &name {
        validate_name(n).map_err(AppError::invalid)?;
        if instance_exists(&root, n) || (loader.is_some() && *n == mc_version) {
            return Err(AppError::new("INSTANCE_EXISTS", format!("已存在同名实例：{n}")).with(json!({ "name": n })));
        }
    }
    let java = match &loader {
        Some(l) if l.kind == "forge" || l.kind == "neoforge" => {
            Some(java_for_processors(&ctx.discover, ctx.settings, &mc_version, &root).await?)
        }
        _ => None,
    };
    let cfg = DownloadConfig { game_dir: root.clone(), ..ctx.mgr.cfg() };
    let label = match (&loader, &name) {
        (Some(l), Some(n)) => format!("{n}（Minecraft {mc_version} + {} {}）", l.kind, l.version),
        (Some(l), None) => format!("Minecraft {mc_version} + {} {}", l.kind, l.version),
        (None, Some(n)) => format!("{n}（Minecraft {mc_version}）"),
        (None, None) => format!("Minecraft {mc_version}"),
    };
    let kind = loader.as_ref().map(|l| l.kind.clone()).unwrap_or_else(|| "vanilla".into());
    tracing::info!(%mc_version, loader = ?loader, name = ?name, root = %root.display(), "install game");
    let (mgr, settings, emit) = (ctx.mgr.clone(), ctx.settings, ctx.emit.clone());
    let id = ctx.mgr.spawn_with(SpawnOpts { cfg: Some(cfg), on_progress: None }, &kind, &label, move |t| async move {
        let pre_existed = vanilla::version_json_path(&root, &mc_version).is_file();
        t.tracker.set_scale(0.0, 0.97);
        let produced = install_version(&t, &mgr, &mc_version, loader.as_ref(), java).await?;
        t.tracker.set_scale(0.0, 1.0);
        t.tracker.begin_stage("finalize", 0.97, 0.03, 0, 0);
        let final_name = name.unwrap_or_else(|| produced.clone());
        let (r, p, n) = (root.clone(), produced.clone(), final_name.clone());
        tokio::task::spawn_blocking(move || finalize_name(&r, &p, &n, pre_existed))
            .await
            .map_err(|e| DlError::Other(e.to_string()))?
            .map_err(|e| into_task_error(AppError::io(e)))?;
        register_instance(settings, &root, &final_name);
        let instance_id: PathBuf = root.join("versions").join(&final_name);
        emit("app://instance-installed", json!({ "name": final_name, "instanceId": instance_id, "mcVersion": mc_version }));
        t.tracker.begin_stage("done", 1.0, 0.0, 0, 0);
        Ok(())
    });
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, id: &str, body: &str, jar: bool) {
        let d = root.join("versions").join(id);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(format!("{id}.json")), body).unwrap();
        if jar {
            std::fs::write(d.join(format!("{id}.jar")), b"jar").unwrap();
        }
    }

    fn id_of(root: &Path, id: &str) -> String {
        let v: Value = serde_json::from_slice(&std::fs::read(vanilla::version_json_path(root, id)).unwrap()).unwrap();
        v["id"].as_str().unwrap().to_string()
    }

    #[test]
    fn renames_loader_versions_by_moving() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "1.20.1", r#"{"id":"1.20.1"}"#, true);
        write(d.path(), "fabric-loader-0.15.0-1.20.1", r#"{"id":"fabric-loader-0.15.0-1.20.1","inheritsFrom":"1.20.1"}"#, false);
        finalize_name(d.path(), "fabric-loader-0.15.0-1.20.1", "My Fabric", true).unwrap();
        assert_eq!(id_of(d.path(), "My Fabric"), "My Fabric");
        assert!(!d.path().join("versions/fabric-loader-0.15.0-1.20.1").exists());
        assert!(vanilla::version_json_path(d.path(), "1.20.1").is_file(), "parent stays");
        let json: Value = serde_json::from_slice(&std::fs::read(vanilla::version_json_path(d.path(), "My Fabric")).unwrap()).unwrap();
        assert_eq!(json["inheritsFrom"], "1.20.1");
    }

    #[test]
    fn vanilla_rename_copies_when_preexisting_and_moves_when_fresh() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "1.20.1", r#"{"id":"1.20.1","mainClass":"x"}"#, true);
        finalize_name(d.path(), "1.20.1", "Copy", true).unwrap();
        assert!(d.path().join("versions/Copy/Copy.jar").is_file());
        assert!(d.path().join("versions/1.20.1/1.20.1.jar").is_file(), "pre-existing vanilla is kept");
        finalize_name(d.path(), "1.20.1", "Moved", false).unwrap();
        assert!(d.path().join("versions/Moved/Moved.jar").is_file());
        assert!(!d.path().join("versions/1.20.1").exists(), "freshly installed original is removed");
        assert_eq!(id_of(d.path(), "Moved"), "Moved");
        let moved: Value = serde_json::from_slice(&std::fs::read(vanilla::version_json_path(d.path(), "Moved")).unwrap()).unwrap();
        assert_eq!(moved["clientVersion"], "1.20.1", "the Minecraft version survives the rename");
    }

    #[test]
    fn rename_refuses_existing_target_and_noop_for_same_name() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "a", r#"{"id":"a"}"#, true);
        write(d.path(), "b", r#"{"id":"b"}"#, true);
        assert!(finalize_name(d.path(), "a", "b", true).is_err());
        finalize_name(d.path(), "a", "a", true).unwrap();
    }

    #[test]
    fn validation() {
        assert!(validate_loader(&LoaderChoice { kind: "fabric".into(), version: "0.16.9".into() }).is_ok());
        assert!(validate_loader(&LoaderChoice { kind: "optifine".into(), version: "1".into() }).is_err());
        assert!(validate_loader(&LoaderChoice { kind: "forge".into(), version: "../x".into() }).is_err());
        assert!(validate_loader(&LoaderChoice { kind: "forge".into(), version: "".into() }).is_err());
    }
}
