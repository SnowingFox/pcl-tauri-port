//! Vanilla install as a resumable multi-stage task. Every stage skips files that already
//! validate (size + SHA-1), so re-running a failed/cancelled install resumes where it stopped.

use crate::download::engine::FileSpec;
use crate::download::error::{DlError, DlResult};
use crate::download::manifest::{host_os, AssetIndex, VersionJson};
use crate::download::{Manager, TaskCtx};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub fn version_dir(game_dir: &Path, id: &str) -> PathBuf {
    game_dir.join("versions").join(id)
}
pub fn version_json_path(game_dir: &Path, id: &str) -> PathBuf {
    version_dir(game_dir, id).join(format!("{id}.json"))
}
pub fn version_jar_path(game_dir: &Path, id: &str) -> PathBuf {
    version_dir(game_dir, id).join(format!("{id}.jar"))
}

/// Download libraries described by a parsed version json (vanilla or loader).
pub async fn install_libraries(ctx: &TaskCtx, vj: &VersionJson, start: f64, span: f64, stage: &str) -> DlResult<()> {
    let (os, arch) = host_os();
    let libs = ctx.cfg.game_dir.join("libraries");
    let mut files = vec![];
    let mut seen = std::collections::HashSet::new();
    for l in &vj.libraries {
        for f in l.files(os, arch) {
            if seen.insert(f.path.clone()) {
                files.push(FileSpec::new(f.url, libs.join(&f.path)).with_sha1(f.sha1).with_size(f.size));
            }
        }
    }
    let bytes: u64 = files.iter().filter_map(|f| f.size).sum();
    ctx.tracker.begin_stage(stage, start, span, files.len() as u64, bytes);
    ctx.batch(files).await
}

/// Install (or resume) vanilla `mc`. Returns the raw version json.
pub async fn install_vanilla(ctx: &TaskCtx, mgr: &Arc<Manager>, mc: &str, start: f64, span: f64) -> DlResult<Value> {
    let gd = ctx.cfg.game_dir.clone();
    let manifest = mgr.version_manifest(false).await?;
    let entry = manifest.find(mc).ok_or_else(|| DlError::Other(format!("unknown Minecraft version {mc}")))?.clone();

    // 1. version json
    ctx.tracker.begin_stage("version-json", start, span * 0.02, 1, 0);
    let jpath = version_json_path(&gd, mc);
    ctx.file(FileSpec::new(&entry.url, &jpath).with_sha1(entry.sha1.clone())).await?;
    let raw: Value = serde_json::from_slice(&tokio::fs::read(&jpath).await?)?;
    let vj: VersionJson = serde_json::from_value(raw.clone())?;
    install_version_files(ctx, mc, raw, &vj, start + span * 0.02, span * 0.98).await
}

/// Steps 2-6 of the vanilla install for an already present version json (client jar, libraries,
/// assets, log config). Lets callers repair an install without consulting the version manifest.
pub async fn install_version_files(ctx: &TaskCtx, mc: &str, raw: Value, vj: &VersionJson, start: f64, span: f64) -> DlResult<Value> {
    let gd = ctx.cfg.game_dir.clone();
    let span = span / 0.98; // the step weights below are fractions of the whole install
    let mut at = start;

    // 2. client jar
    if let Some(c) = vj.downloads.as_ref().and_then(|d| d.get("client")) {
        ctx.tracker.begin_stage("client-jar", at, span * 0.13, 1, c.size.unwrap_or(0));
        ctx.file(FileSpec::new(&c.url, version_jar_path(&gd, mc)).with_sha1(c.sha1.clone()).with_size(c.size)).await?;
    }
    at += span * 0.13;

    // 3. libraries
    install_libraries(ctx, vj, at, span * 0.25, "libraries").await?;
    at += span * 0.25;

    // 4. asset index
    if let Some(ai) = &vj.asset_index {
        ctx.tracker.begin_stage("asset-index", at, span * 0.02, 1, ai.size.unwrap_or(0));
        let ipath = gd.join("assets/indexes").join(format!("{}.json", ai.id));
        ctx.file(FileSpec::new(&ai.url, &ipath).with_sha1(ai.sha1.clone())).await?;
        at += span * 0.02;

        // 5. asset objects
        let idx: AssetIndex = serde_json::from_slice(&tokio::fs::read(&ipath).await?)?;
        let objs = gd.join("assets/objects");
        let mut seen = std::collections::HashSet::new();
        let files: Vec<FileSpec> = idx
            .objects
            .values()
            .filter(|o| seen.insert(o.hash.clone()))
            .map(|o| FileSpec::new(o.url(), objs.join(o.rel_path())).with_sha1(Some(o.hash.clone())).with_size(Some(o.size)))
            .collect();
        let bytes = files.iter().filter_map(|f| f.size).sum();
        ctx.tracker.begin_stage("assets", at, span * 0.55, files.len() as u64, bytes);
        ctx.batch(files).await?;
        at += span * 0.55;

        // Legacy layouts expect real file names on disk.
        if idx.r#virtual || idx.map_to_resources {
            let targets = [(idx.r#virtual, gd.join("assets/virtual").join(&ai.id)), (idx.map_to_resources, gd.join("resources"))];
            let list: Vec<(String, String)> = idx.objects.iter().map(|(k, o)| (k.clone(), o.rel_path())).collect();
            let objs = objs.clone();
            tokio::task::spawn_blocking(move || -> std::io::Result<()> {
                for (on, root) in targets.iter() {
                    if !*on {
                        continue;
                    }
                    for (name, rel) in &list {
                        let dst = root.join(name);
                        if !dst.exists() {
                            if let Some(p) = dst.parent() {
                                std::fs::create_dir_all(p)?;
                            }
                            std::fs::copy(objs.join(rel), dst)?;
                        }
                    }
                }
                Ok(())
            })
            .await
            .map_err(|e| DlError::Other(e.to_string()))??;
        }
    }

    // 6. logging config
    if let Some(f) = raw.pointer("/logging/client/file") {
        if let (Some(id), Some(url)) = (f["id"].as_str(), f["url"].as_str()) {
            ctx.tracker.begin_stage("log-config", at, span * 0.03, 1, 0);
            ctx.file(FileSpec::new(url, gd.join("assets/log_configs").join(id)).with_sha1(f["sha1"].as_str().map(String::from))).await?;
        }
    }
    ctx.tracker.begin_stage("done", start + span * 0.98, 0.0, 0, 0);
    Ok(raw)
}

