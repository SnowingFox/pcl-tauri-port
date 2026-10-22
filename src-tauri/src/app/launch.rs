//! `app_launch`: instance + account -> running game.
//!
//! Stages (all reported on `launch://progress`):
//! `check` (instance, settings, account session) -> `java` -> `download` (auto repair of missing
//! libraries / assets / client jar, authlib-injector) -> `natives` -> `start` -> `running`.

use super::error::{AppError, AppResult};
use super::instance::{self, ResolvedInstance};
use super::tasks::run_task;
use super::{paths, tuning, AppCtx};
use crate::accounts::{AuthSession, AuthlibInjectorConfig};
use crate::download::engine::{FileSpec, ProgressEvent};
use crate::download::manifest::VersionJson as DlVersionJson;
use crate::download::{DownloadConfig, SpawnOpts};
use crate::java::JavaInfo;
use crate::launch::{self as core, AuthData, Emit, ExitInfo, LaunchRequest, PrecheckIssue, SpawnHooks};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tracing::Instrument;

/// Seconds after spawn at which `running` is reported even if the game printed nothing yet.
const RUNNING_FALLBACK_SECS: u64 = 15;
/// Log fragments that show the client got past argument parsing and is initialising.
const RUNNING_MARKERS: [&str; 5] = ["Setting user", "Backend library", "LWJGL Version", "OpenAL initialized", "Sound engine started"];
const KEEP_LAUNCH_LOGS: usize = 30;

/// Emits `launch://progress` for one launch. `instanceId` lets a UI that does not know the
/// launch id yet (the command is still pending) correlate events with its request.
#[derive(Clone)]
pub struct Reporter {
    pub emit: Emit,
    pub id: String,
    pub instance_id: String,
}

impl Reporter {
    pub fn stage(&self, stage: &str, progress: f64, message: &str) {
        self.stage_task(stage, progress, message, None);
    }

    pub fn stage_task(&self, stage: &str, progress: f64, message: &str, task_id: Option<&str>) {
        let mut v = json!({
            "launchId": self.id, "instanceId": self.instance_id, "stage": stage,
            "progress": progress.clamp(0.0, 1.0), "message": message,
        });
        if let Some(t) = task_id {
            v["taskId"] = json!(t);
        }
        (self.emit)("launch://progress", v);
    }
}

fn inflight() -> &'static Mutex<HashSet<PathBuf>> {
    static S: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
    S.get_or_init(Default::default)
}

/// Prevents two simultaneous launch pipelines for the same instance (double click).
struct Guard(PathBuf);

impl Guard {
    fn acquire(dir: &Path) -> AppResult<Self> {
        if !inflight().lock().unwrap().insert(dir.to_path_buf()) {
            return Err(AppError::new("ALREADY_LAUNCHING", "该实例正在启动中"));
        }
        Ok(Guard(dir.to_path_buf()))
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        inflight().lock().unwrap().remove(&self.0);
    }
}

/// A client supplied launch id becomes part of a file name, so keep it boring.
pub fn validate_launch_id(s: &str) -> AppResult<String> {
    let ok = !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')) && !s.starts_with('.');
    ok.then(|| s.to_string()).ok_or_else(|| AppError::invalid("launchId 只能包含字母、数字、-、_、.（最长 64 位）"))
}

pub fn authlib_args(jar: &Path, cfg: &AuthlibInjectorConfig) -> Vec<String> {
    let mut v = vec![format!("-javaagent:{}={}", jar.display(), cfg.server_url)];
    if let Some(p) = &cfg.prefetched {
        v.push(format!("-Dauthlibinjector.yggdrasil.prefetched={p}"));
    }
    v
}

fn repairable(i: &PrecheckIssue) -> bool {
    matches!(
        i,
        PrecheckIssue::ClientJarMissing { .. }
            | PrecheckIssue::LibrariesMissing { .. }
            | PrecheckIssue::AssetIndexMissing { .. }
            | PrecheckIssue::AssetsMissing { .. }
    )
}

fn stage_message(stage: &str) -> &'static str {
    match stage {
        "version-json" => "正在下载版本信息",
        "client-jar" => "正在下载游戏本体",
        "libraries" | "loader-libraries" => "正在下载依赖库",
        "asset-index" => "正在下载资源索引",
        "assets" => "正在下载游戏资源",
        "log-config" => "正在下载日志配置",
        _ => "正在补全游戏文件",
    }
}

fn session_to_auth(s: &AuthSession) -> AuthData {
    AuthData { name: s.player_name.clone(), uuid: s.uuid.clone(), access_token: s.access_token.clone(), user_type: s.user_type.clone() }
}

/// `[(id, raw json)]` from the launched version up to its root, child first.
pub fn version_chain(root: &Path, name: &str) -> AppResult<Vec<(String, Value)>> {
    let mut out: Vec<(String, Value)> = Vec::new();
    let mut cur = name.to_string();
    for _ in 0..10 {
        crate::instances::detect::validate_name(&cur).map_err(AppError::invalid)?;
        let p = root.join("versions").join(&cur).join(format!("{cur}.json"));
        let text = std::fs::read_to_string(&p).map_err(|e| AppError::new("INSTANCE_INVALID", format!("无法读取 {}：{e}", p.display())))?;
        let raw: Value = serde_json::from_str(&text).map_err(|e| AppError::new("INSTANCE_INVALID", format!("{} 不是有效的 JSON：{e}", p.display())))?;
        let next = raw["inheritsFrom"].as_str().filter(|s| !s.is_empty()).map(String::from);
        out.push((cur.clone(), raw));
        match next {
            Some(n) if !out.iter().any(|(i, _)| *i == n) => cur = n,
            _ => break,
        }
    }
    Ok(out)
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> AppResult<T> {
    tokio::task::spawn_blocking(f).await.map_err(|e| AppError::internal(e))
}

/// Download whatever the precheck found missing, through the download engine, and wait for it.
async fn repair(ctx: &AppCtx, inst: &ResolvedInstance, rep: &Reporter) -> AppResult<()> {
    let chain = version_chain(&inst.root, &inst.name)?;
    let cfg = DownloadConfig { game_dir: inst.root.clone(), ..ctx.mgr.cfg() };
    let r = rep.clone();
    let tap: crate::download::engine::Sink = Arc::new(move |p: &ProgressEvent| {
        r.stage_task("download", 0.10 + 0.60 * p.fraction, stage_message(&p.stage), Some(&p.task_id));
    });
    let label = format!("补全游戏文件：{}", inst.name);
    run_task(&ctx.mgr, SpawnOpts { cfg: Some(cfg), on_progress: Some(tap) }, "repair", &label, move |t| async move {
        let n = chain.len();
        let children = n.saturating_sub(1);
        let child_span = if children > 0 { 0.2 } else { 0.0 };
        for (i, (_, raw)) in chain.iter().take(children).enumerate() {
            let vj: DlVersionJson = serde_json::from_value(raw.clone())?;
            let step = child_span / children as f64;
            crate::install::vanilla::install_libraries(&t, &vj, step * i as f64, step, "loader-libraries").await?;
        }
        let (rid, raw) = &chain[n - 1];
        let vj: DlVersionJson = serde_json::from_value(raw.clone())?;
        crate::install::vanilla::install_version_files(&t, rid, raw.clone(), &vj, child_span, 1.0 - child_span).await?;
        Ok(())
    })
    .await?;
    Ok(())
}

/// Path of the authlib-injector jar, downloading it first if needed. An existing jar is used
/// as is, so launching a third-party account keeps working offline.
pub async fn ensure_authlib_injector(ctx: &AppCtx, rep: &Reporter) -> AppResult<PathBuf> {
    let jar = paths::authlib_injector_jar(&ctx.data_dir);
    if std::fs::metadata(&jar).map(|m| m.is_file() && m.len() > 0).unwrap_or(false) {
        return Ok(jar);
    }
    rep.stage("download", 0.10, "正在获取 authlib-injector");
    let (url, sha256) = crate::accounts::yggdrasil::latest_injector(&crate::accounts::microsoft::http(), crate::accounts::yggdrasil::AUTHLIB_INJECTOR_LATEST_META).await?;
    let dest = jar.clone();
    let r = rep.clone();
    let tap: crate::download::engine::Sink = Arc::new(move |p: &ProgressEvent| {
        r.stage_task("download", 0.10 + 0.60 * p.fraction, "正在下载 authlib-injector", Some(&p.task_id));
    });
    run_task(&ctx.mgr, SpawnOpts { cfg: None, on_progress: Some(tap) }, "authlib", "authlib-injector", move |t| async move {
        t.tracker.begin_stage("authlib-injector", 0.0, 1.0, 1, 0);
        let mut spec = FileSpec::new(url, &dest);
        spec.sha256 = sha256;
        t.file(spec).await
    })
    .await?;
    Ok(jar)
}

fn prune_logs(dir: &Path, keep: usize) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> =
        rd.flatten().filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path()))).collect();
    files.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, p) in files.into_iter().skip(keep) {
        let _ = std::fs::remove_file(p);
    }
}

/// Run the full pipeline. Returns the launch id once the game process has been spawned.
pub async fn launch(ctx: &AppCtx, instance_id: &str, account_id: &str, launch_id: Option<String>) -> AppResult<String> {
    let g = ctx.settings.get();
    let inst = instance::resolve(&g, instance_id)?;
    let _guard = Guard::acquire(&inst.version_dir)?;
    let id = match launch_id {
        Some(s) => validate_launch_id(&s)?,
        None => core::next_id(&inst.name),
    };
    let span = tracing::info_span!("launch", id = %id, instance = %inst.name);
    let rep = Reporter { emit: ctx.emit.clone(), id: id.clone(), instance_id: inst.version_dir.to_string_lossy().into_owned() };
    run(ctx, &g, inst, account_id, rep).instrument(span).await.inspect_err(|e| {
        tracing::warn!(code = e.code, "launch failed: {}", e.message);
    })
}

async fn run(ctx: &AppCtx, g: &crate::settings::GlobalSettings, inst: ResolvedInstance, account_id: &str, rep: Reporter) -> AppResult<String> {
    let id = rep.id.clone();
    // ---- check
    rep.stage("check", 0.0, "正在检查实例与账号");
    let session = ctx.accounts()?.session(account_id).await?;
    if session.account_kind != "offline" {
        super::logging::register_secret(&session.access_token);
    }
    tracing::info!(account_kind = %session.account_kind, "session ready");
    let mods = tuning::count_mod_files(&inst.game_dir.join("mods"));
    let tune = tuning::merge(g, &inst.settings, &inst.info, mods, tuning::physical_memory_mb());
    let resolved = {
        let (root, name) = (inst.root.clone(), inst.name.clone());
        blocking(move || core::version::resolve_version(&root, &name)).await??
    };
    rep.stage("check", 0.05, "实例检查完成");

    // ---- java
    rep.stage("java", 0.05, "正在选择 Java");
    let required = resolved
        .json
        .java_version
        .as_ref()
        .map(|j| j.major_version)
        .filter(|m| *m > 0)
        .unwrap_or_else(|| crate::java::default_major_for_mc(inst.info.mc_version.as_deref().unwrap_or(&resolved.root_id)));
    let max = tuning::max_java_major(required, resolved.json.main_class.as_deref().unwrap_or(""), &inst.info);
    let discovered = {
        let extra = [inst.root.join("runtime")];
        let f = ctx.discover.clone();
        blocking(move || f(&extra)).await?
    };
    let (java, note): (JavaInfo, _) = tuning::pick_java(&tune.java_preferred, discovered, required, max)
        .map_err(|found| AppError::java_missing(required, max, &found))?;
    tracing::info!(java = %java.path, major = java.major, required, "java selected");
    rep.stage("java", 0.10, &note.unwrap_or_else(|| format!("使用 Java {}（{}）", java.major, java.vendor)));

    // ---- request + precheck + repair
    let mut req = LaunchRequest {
        minecraft_dir: inst.root.to_string_lossy().into_owned(),
        version_id: inst.name.clone(),
        auth: session_to_auth(&session),
        java_path: Some(java.path.clone()),
        game_dir: Some(inst.game_dir.to_string_lossy().into_owned()),
        max_memory_mb: Some(tune.max_memory_mb),
        min_memory_mb: None,
        extra_jvm_args: tune.jvm_args.clone(),
        extra_game_args: tune.game_args.clone(),
        demo: false,
        resolution: tune.resolution,
    };
    let (mut report, _) = { let r = req.clone(); blocking(move || core::precheck(&r)).await? };
    if report.issues.iter().any(repairable) {
        rep.stage("download", 0.10, "发现缺失文件，正在补全");
        tracing::info!(issues = ?report.issues, "repairing missing files");
        repair(ctx, &inst, &rep).await?;
        report = { let r = req.clone(); blocking(move || core::precheck(&r)).await?.0 };
    }
    let blocking_issues: Vec<&PrecheckIssue> = report.issues.iter().filter(|i| i.is_blocking()).collect();
    if !blocking_issues.is_empty() {
        let details = json!({ "issues": report.issues });
        return Err(if blocking_issues.iter().any(|i| repairable(i)) {
            AppError::new("FILES_MISSING", "仍有游戏文件缺失，请尝试重新安装该版本").with(details)
        } else if blocking_issues.iter().any(|i| matches!(i, PrecheckIssue::VersionJsonMissing { .. })) {
            AppError::new("INSTANCE_INVALID", "版本文件缺失或损坏").with(details)
        } else {
            AppError::new("PRECHECK_FAILED", "启动前检查未通过").with(details)
        });
    }

    // ---- third-party accounts
    if let Some(cfg) = &session.authlib_injector {
        let jar = ensure_authlib_injector(ctx, &rep).await?;
        let mut v = authlib_args(&jar, cfg);
        v.extend(std::mem::take(&mut req.extra_jvm_args));
        req.extra_jvm_args = v;
    }
    rep.stage("download", 0.70, "文件检查完成");

    // ---- natives
    rep.stage("natives", 0.70, "正在解压本地库");
    let cmd = { let r = req.clone(); blocking(move || core::prepare(&r)).await?.map(|(c, _)| c)? };
    tracing::info!(command = %super::logging::mask(&cmd.display(&req.auth)), "launch command ready");
    rep.stage("natives", 0.90, "本地库就绪");

    // ---- start
    rep.stage("start", 0.90, "正在启动游戏进程");
    let logs = paths::launch_logs_dir(&ctx.data_dir);
    prune_logs(&logs, KEEP_LAUNCH_LOGS);
    let announced = Arc::new(AtomicBool::new(false));
    let announce = {
        let (rep, flag) = (rep.clone(), announced.clone());
        Arc::new(move || {
            if !flag.swap(true, Ordering::SeqCst) {
                rep.stage("running", 1.0, "游戏已启动");
            }
        })
    };
    let on_line: Arc<dyn Fn(&str, &str) + Send + Sync> = {
        let announce = announce.clone();
        Arc::new(move |stream, line| {
            let _ = stream;
            if RUNNING_MARKERS.iter().any(|m| line.contains(m)) {
                announce();
            }
        })
    };
    let (game_dir, root) = (inst.game_dir.clone(), inst.root.clone());
    let on_exit = Box::new(move |info: &ExitInfo| exit_extras(info, &game_dir, &root));
    let hooks = SpawnHooks { launch_id: Some(id.clone()), on_line: Some(on_line), on_exit: Some(on_exit) };
    let started = core::spawn_cmd(ctx.emit.clone(), &req, cmd, Some(logs), hooks)?;
    tracing::info!("game process spawned");
    rep.stage("start", 0.95, "游戏进程已创建");
    // Safety net: report `running` even when the game stays silent.
    let watch_id = id.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(RUNNING_FALLBACK_SECS));
        if core::launch_list_running().contains(&watch_id) {
            announce();
        }
    });
    Ok(started)
}

/// Extra fields of `launch://exit` for an abnormal exit: crash analysis over the logs written in
/// the last three minutes plus the captured output.
pub fn exit_extras(info: &ExitInfo, game_dir: &Path, root: &Path) -> Value {
    tracing::info!(code = ?info.code, killed = info.killed, secs = info.duration.as_secs(), "game exited");
    if !info.crashed() {
        return json!({});
    }
    let analysis = crate::crash::commands::analyze_game(game_dir, root, Some(&info.tail));
    tracing::warn!(reasons = analysis.reasons.len(), "crash analysed: {}", analysis.summary.lines().next().unwrap_or(""));
    json!({
        "reason": analysis.summary,
        "reportPath": analysis.primary_file,
        "crash": analysis,
    })
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn launch_ids_are_filename_safe() {
        assert!(validate_launch_id("abc-1_2.3").is_ok());
        for bad in ["", "../x", "a/b", ".hidden", "a b", &"x".repeat(65)] {
            assert!(validate_launch_id(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn javaagent_args() {
        let cfg = AuthlibInjectorConfig { server_url: "https://example.org/api/yggdrasil".into(), prefetched: Some("QUJD".into()) };
        assert_eq!(
            authlib_args(Path::new("/d/authlib-injector.jar"), &cfg),
            vec!["-javaagent:/d/authlib-injector.jar=https://example.org/api/yggdrasil", "-Dauthlibinjector.yggdrasil.prefetched=QUJD"]
        );
        let none = AuthlibInjectorConfig { server_url: "u".into(), prefetched: None };
        assert_eq!(authlib_args(Path::new("/j"), &none).len(), 1);
    }

    #[test]
    fn repairable_issue_kinds() {
        assert!(repairable(&PrecheckIssue::AssetsMissing { count: 1 }));
        assert!(repairable(&PrecheckIssue::ClientJarMissing { path: "p".into() }));
        assert!(!repairable(&PrecheckIssue::EmptyPlayerName));
        assert!(!repairable(&PrecheckIssue::NoJava { required_major: 17 }));
    }
}
