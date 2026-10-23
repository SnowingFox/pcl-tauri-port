//! Minecraft launch core: version resolution, pre-launch checks, argument
//! building and process management. No UI code lives here.
//!
//! Derived from the behaviour of Plain Craft Launcher (see LICENCE in the
//! upstream project); this is a clean Rust re-implementation.

pub mod args;
pub mod assets;
pub mod natives;
pub mod version;

use crate::java::{self, JavaInfo};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use version::{RuleContext, resolve_version, version_jar_path};

#[derive(Debug, thiserror::Error)]
pub enum LaunchError {
    #[error("version json error: {0}")]
    VersionJson(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("no usable Java (need >= {0})")]
    NoJava(u32),
    #[error("spawn failed: {0}")]
    Spawn(String),
    #[error("no such running instance: {0}")]
    NotRunning(String),
}

impl Serialize for LaunchError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

/// Supplied by the account module.
#[derive(Clone, Serialize, Deserialize)]
pub struct AuthData {
    pub name: String,
    pub uuid: String,
    pub access_token: String,
    /// "msa", "legacy" or "mojang". Offline accounts typically use "msa" with a dummy token.
    pub user_type: String,
}

impl std::fmt::Debug for AuthData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthData")
            .field("name", &self.name)
            .field("uuid", &self.uuid)
            .field("access_token", &"<redacted>")
            .field("user_type", &self.user_type)
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchRequest {
    pub minecraft_dir: String,
    pub version_id: String,
    pub auth: AuthData,
    /// Explicit java executable; None = auto select.
    pub java_path: Option<String>,
    /// Working dir (version isolation); None = versions/<id>.
    pub game_dir: Option<String>,
    pub max_memory_mb: Option<u32>,
    pub min_memory_mb: Option<u32>,
    #[serde(default)]
    pub extra_jvm_args: Vec<String>,
    #[serde(default)]
    pub extra_game_args: Vec<String>,
    #[serde(default)]
    pub demo: bool,
    pub resolution: Option<(u32, u32)>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PrecheckIssue {
    VersionJsonMissing { path: String },
    ClientJarMissing { path: String },
    AssetIndexMissing { id: String },
    LibrariesMissing { count: usize, first: Vec<String> },
    AssetsMissing { count: usize },
    NoJava { required_major: u32 },
    JavaArchMismatch { java_arch: String, system_arch: String },
    InvalidMemory,
    EmptyPlayerName,
}

impl PrecheckIssue {
    /// Blocking issues prevent launch; others are warnings.
    pub fn is_blocking(&self) -> bool {
        !matches!(self, PrecheckIssue::JavaArchMismatch { .. } | PrecheckIssue::AssetsMissing { .. })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PrecheckReport {
    pub ok: bool,
    pub issues: Vec<PrecheckIssue>,
    pub required_java_major: u32,
    pub java: Option<JavaInfo>,
}

fn system_arch() -> &'static str {
    if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" }
}

/// Pick the java for a request: explicit path, else auto-select.
fn pick_java(req: &LaunchRequest, required: u32) -> Option<JavaInfo> {
    if let Some(p) = &req.java_path {
        return java::inspect(Path::new(p));
    }
    let extra = [Path::new(&req.minecraft_dir).join("runtime")];
    java::select_for(&java::discover(&extra), required).cloned()
}

pub fn precheck(req: &LaunchRequest) -> (PrecheckReport, Option<version::ResolvedVersion>) {
    let mc = Path::new(&req.minecraft_dir);
    let mut issues = Vec::new();
    let ctx = RuleContext::current(HashMap::new());
    if req.auth.name.trim().is_empty() {
        issues.push(PrecheckIssue::EmptyPlayerName);
    }
    if req.max_memory_mb == Some(0) {
        issues.push(PrecheckIssue::InvalidMemory);
    }
    let jp = version::version_json_path(mc, &req.version_id);
    let resolved = match resolve_version(mc, &req.version_id) {
        Ok(r) => Some(r),
        Err(_) => {
            issues.push(PrecheckIssue::VersionJsonMissing { path: jp.display().to_string() });
            None
        }
    };
    let mut required = 8;
    let mut java = None;
    if let Some(r) = &resolved {
        required = r
            .json
            .java_version
            .as_ref()
            .map(|j| j.major_version)
            .filter(|m| *m > 0)
            .unwrap_or_else(|| java::default_major_for_mc(&r.root_id));
        let jar = version_jar_path(mc, &r.jar_id);
        if !jar.is_file() {
            issues.push(PrecheckIssue::ClientJarMissing { path: jar.display().to_string() });
        }
        let missing = natives::missing_libraries(r, &mc.join("libraries"), &ctx);
        if !missing.is_empty() {
            issues.push(PrecheckIssue::LibrariesMissing {
                count: missing.len(),
                first: missing.iter().take(5).map(|p| p.display().to_string()).collect(),
            });
        }
        let aid = r.json.asset_index.as_ref().map(|a| a.id.clone()).or(r.json.assets.clone());
        if let Some(aid) = aid {
            match assets::load_index(&mc.join("assets"), &aid) {
                Ok(idx) => {
                    let m = assets::missing_assets(&mc.join("assets"), &idx);
                    if !m.is_empty() {
                        issues.push(PrecheckIssue::AssetsMissing { count: m.len() });
                    }
                }
                Err(_) => issues.push(PrecheckIssue::AssetIndexMissing { id: aid }),
            }
        }
        java = pick_java(req, required);
        match &java {
            None => issues.push(PrecheckIssue::NoJava { required_major: required }),
            Some(j) if j.major < required => issues.push(PrecheckIssue::NoJava { required_major: required }),
            Some(j) if j.arch != "unknown" && j.arch != system_arch() => {
                issues.push(PrecheckIssue::JavaArchMismatch { java_arch: j.arch.clone(), system_arch: system_arch().into() })
            }
            _ => {}
        }
    }
    let ok = !issues.iter().any(|i| i.is_blocking());
    (PrecheckReport { ok, issues, required_java_major: required, java }, resolved)
}

/// Build the full command without spawning (also extracts natives and
/// prepares legacy assets, which are launch prerequisites).
pub fn prepare(req: &LaunchRequest) -> Result<(args::BuiltCommand, PrecheckReport), LaunchError> {
    let (report, resolved) = precheck(req);
    let resolved = resolved.ok_or_else(|| LaunchError::VersionJson("version not found".into()))?;
    let java = report.java.clone().ok_or(LaunchError::NoJava(report.required_java_major))?;
    let ctx = RuleContext::current(feature_map(req));
    let mc = Path::new(&req.minecraft_dir);
    ensure_version_named_client_jar(mc, &resolved)?;
    let cmd = args::build_command(req, &resolved, Path::new(&java.path), &ctx)?;
    natives::extract_natives(&resolved, &mc.join("libraries"), &cmd.natives_dir, &ctx)?;
    if let Some(id) = resolved.json.asset_index.as_ref().map(|a| a.id.clone())
        && let Ok(idx) = assets::load_index(&mc.join("assets"), &id)
    {
        assets::materialize_legacy(&mc.join("assets"), &cmd.working_dir, &idx)?;
    }
    std::fs::create_dir_all(&cmd.working_dir).map_err(|e| LaunchError::Io(e.to_string()))?;
    Ok((cmd, report))
}

/// Make `versions/<id>/<id>.jar` exist (a copy of the inherited client jar) when the version's
/// arguments require the classpath jar to be named after the version, see
/// [`args::client_jar_is_version_named`]. A no-op otherwise and when the copy is already current.
pub fn ensure_version_named_client_jar(mc: &Path, v: &version::ResolvedVersion) -> Result<(), LaunchError> {
    if !args::client_jar_is_version_named(v) || v.id == v.jar_id {
        return Ok(());
    }
    let (src, dst) = (version_jar_path(mc, &v.jar_id), version_jar_path(mc, &v.id));
    let len = |p: &Path| std::fs::metadata(p).map(|m| m.len()).ok();
    if len(&src).is_none() || len(&src) == len(&dst) {
        return Ok(()); // nothing to copy from (precheck reports it) or already in place
    }
    let io = |e: std::io::Error| LaunchError::Io(format!("{}: {e}", dst.display()));
    if let Some(dir) = dst.parent() {
        std::fs::create_dir_all(dir).map_err(io)?;
    }
    let tmp = dst.with_extension("jar.tmp");
    std::fs::copy(&src, &tmp).map_err(io)?;
    std::fs::rename(&tmp, &dst).map_err(io)
}

fn feature_map(req: &LaunchRequest) -> HashMap<String, bool> {
    let mut m = HashMap::new();
    m.insert("is_demo_user".into(), req.demo);
    m.insert("has_custom_resolution".into(), req.resolution.is_some());
    m
}

/// Event sink abstraction so the launch pipeline can run (and be tested) without a Tauri runtime.
pub type Emit = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;

pub fn tauri_emit(app: &AppHandle) -> Emit {
    let app = app.clone();
    Arc::new(move |ev, v| {
        let _ = app.emit(ev, v);
    })
}

fn running() -> &'static Mutex<HashMap<String, u32>> {
    static R: OnceLock<Mutex<HashMap<String, u32>>> = OnceLock::new();
    R.get_or_init(Default::default)
}

/// Launch ids the user asked us to kill (so the exit is not reported as a crash).
fn killed() -> &'static Mutex<HashSet<String>> {
    static R: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    R.get_or_init(Default::default)
}

pub fn next_id(version: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(1);
    let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    format!("{version}-{ts}-{}", N.fetch_add(1, Ordering::Relaxed))
}

/// What the exit hook gets to know about a finished game process.
pub struct ExitInfo {
    pub code: Option<i32>,
    /// The user asked the launcher to kill this process.
    pub killed: bool,
    pub duration: Duration,
    /// Last lines of output (stdout and stderr interleaved), already token-masked.
    pub tail: Vec<String>,
}

impl ExitInfo {
    /// Non-zero exit that was not caused by the user.
    pub fn crashed(&self) -> bool {
        !self.killed && self.code != Some(0)
    }
}

pub type LineHook = Arc<dyn Fn(&str, &str) + Send + Sync>;
pub type ExitHook = Box<dyn FnOnce(&ExitInfo) -> serde_json::Value + Send>;

/// Optional callbacks for [`spawn_cmd`].
#[derive(Default)]
pub struct SpawnHooks {
    /// Use this id instead of generating one.
    pub launch_id: Option<String>,
    /// Called for every output line as `(stream, line)`, from the reader threads.
    pub on_line: Option<LineHook>,
    /// Called once after the process ended; the returned object's fields are merged into `launch://exit`.
    pub on_exit: Option<ExitHook>,
}

const TAIL_LINES: usize = 400;

fn line_level(stream: &str, line: &str) -> &'static str {
    if line.contains("/ERROR]") || line.contains("/FATAL]") {
        "error"
    } else if line.contains("/WARN]") {
        "warn"
    } else if line.contains("/DEBUG]") || line.contains("/TRACE]") {
        "debug"
    } else if stream == "stderr" {
        "warn"
    } else {
        "info"
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn log_event(id: &str, stream: &str, line: &str) -> serde_json::Value {
    serde_json::json!({ "id": id, "launchId": id, "stream": stream, "line": line, "level": line_level(stream, line), "ts": now_ms() })
}

/// Remove loader-search variables inherited from the launcher's own environment.
///
/// `DYLD_*` is how cargo, IDE run configurations and developer shells point *their* tools at
/// libraries. A game that inherits it loads the wrong dylibs or, with JNA < 5.13 (Minecraft 1.20.1
/// and older), aborts in `dlopen` when the value is long ("snprintf() output has been truncated").
/// (SIP already strips them for system binaries, but not for a Homebrew / Mojang JVM.)
pub fn clean_game_env(command: &mut Command, vars: impl Iterator<Item = std::ffi::OsString>) {
    for k in vars {
        if k.to_string_lossy().starts_with("DYLD_") {
            command.env_remove(&k);
        }
    }
}

/// Spawn an already prepared command; streams stdout/stderr as `launch://log`, exit as `launch://exit`.
pub fn spawn_cmd(
    emit: Emit,
    req: &LaunchRequest,
    cmd: args::BuiltCommand,
    log_dir: Option<PathBuf>,
    hooks: SpawnHooks,
) -> Result<String, LaunchError> {
    // Offline sessions carry a placeholder token (the player UUID); real tokens must never be logged.
    if req.auth.user_type != "legacy" {
        crate::app::logging::register_secret(&req.auth.access_token);
    }
    let id = hooks.launch_id.clone().unwrap_or_else(|| next_id(&req.version_id));
    let started = Instant::now();
    let mut command = Command::new(&cmd.java);
    command.args(&cmd.args).current_dir(&cmd.working_dir);
    clean_game_env(&mut command, std::env::vars_os().map(|(k, _)| k));
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| LaunchError::Spawn(format!("{}: {e}", cmd.java.display())))?;
    running().lock().unwrap().insert(id.clone(), child.id());

    let log_file = log_dir.and_then(|d| {
        std::fs::create_dir_all(&d).ok()?;
        std::fs::File::create(d.join(format!("{id}.log"))).ok()
    });
    let log_file = Arc::new(Mutex::new(log_file));
    let tail: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::new()));
    let header = crate::app::logging::mask(&cmd.display(&req.auth));
    if let Some(f) = log_file.lock().unwrap().as_mut() {
        // First line of the per-launch log: what was run and where (token already masked).
        let _ = writeln!(f, "[launcher] cwd={}\n[launcher] {header}", cmd.working_dir.display());
    }
    emit("launch://log", log_event(&id, "launcher", &header));

    let mut readers = Vec::new();
    for (stream, pipe) in [
        ("stdout", child.stdout.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>)),
        ("stderr", child.stderr.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>)),
    ] {
        let Some(pipe) = pipe else { continue };
        let (emit, id, lf, tail, on_line) = (emit.clone(), id.clone(), log_file.clone(), tail.clone(), hooks.on_line.clone());
        readers.push(std::thread::spawn(move || {
            let mut r = BufReader::new(pipe);
            let mut buf = Vec::new();
            while r.read_until(b'\n', &mut buf).map(|n| n > 0).unwrap_or(false) {
                let raw = String::from_utf8_lossy(&buf).trim_end_matches(['\r', '\n']).to_string();
                buf.clear();
                let line = crate::app::logging::mask(&raw);
                if let Some(f) = lf.lock().unwrap().as_mut() {
                    let _ = writeln!(f, "{line}");
                }
                {
                    let mut t = tail.lock().unwrap();
                    if t.len() >= TAIL_LINES {
                        t.pop_front();
                    }
                    t.push_back(line.clone());
                }
                if let Some(cb) = &on_line {
                    cb(stream, &line);
                }
                emit("launch://log", log_event(&id, stream, &line));
            }
        }));
    }
    let wait_id = id.clone();
    let on_exit = hooks.on_exit;
    std::thread::spawn(move || {
        let status = child.wait();
        for r in readers {
            let _ = r.join();
        }
        running().lock().unwrap().remove(&wait_id);
        let code = status.ok().and_then(|s| s.code());
        let was_killed = killed().lock().unwrap().remove(&wait_id);
        let info = ExitInfo { code, killed: was_killed, duration: started.elapsed(), tail: tail.lock().unwrap().iter().cloned().collect() };
        let mut payload = serde_json::json!({
            "id": wait_id, "launchId": wait_id, "code": code,
            "crashed": info.crashed(), "crash": serde_json::Value::Null,
            "durationMs": info.duration.as_millis() as u64,
        });
        if let Some(h) = on_exit
            && let (Some(base), serde_json::Value::Object(extra)) = (payload.as_object_mut(), h(&info))
        {
            base.extend(extra);
        }
        emit("launch://exit", payload);
    });
    Ok(id)
}

/// Spawn the game; streams stdout/stderr as `launch://log`, exit as `launch://exit`.
pub fn spawn(app: AppHandle, req: &LaunchRequest, log_dir: Option<PathBuf>) -> Result<String, LaunchError> {
    let (cmd, _) = prepare(req)?;
    spawn_cmd(tauri_emit(&app), req, cmd, log_dir, SpawnHooks::default())
}

#[tauri::command]
pub fn launch_precheck(request: LaunchRequest) -> Result<PrecheckReport, LaunchError> {
    check_request(&request, &crate::settings::guard::registered_roots())?;
    Ok(precheck(&request).0)
}

fn reject(msg: impl Into<String>) -> LaunchError {
    LaunchError::Spawn(format!("rejected: {}", msg.into()))
}

/// Validation for requests that come from the webview (the Tauri commands below). The
/// internal `precheck` / `prepare` / `spawn` functions stay permissive so the app layer can
/// add launcher-controlled arguments such as the authlib-injector `-javaagent`.
///
/// - `minecraft_dir` must be a registered Minecraft folder, `version_id` one path component
/// - `java_path` must be a real `java` binary, `game_dir` inside a registered folder
/// - extra JVM arguments must not load agents, patch modules or run commands
pub(crate) fn check_request(req: &LaunchRequest, roots: &[PathBuf]) -> Result<(), LaunchError> {
    use crate::settings::guard;
    let mc = guard::resolve(Path::new(&req.minecraft_dir)).map_err(reject)?;
    if !roots.iter().any(|r| guard::resolve(r).map(|r| r == mc).unwrap_or(false)) {
        return Err(reject(format!("unregistered Minecraft folder {}", req.minecraft_dir)));
    }
    if !guard::is_safe_component(&req.version_id) {
        return Err(reject(format!("bad version id {:?}", req.version_id)));
    }
    if let Some(j) = req.java_path.as_deref().filter(|j| !j.is_empty()) {
        java::check_java_exe(j).map_err(reject)?;
    }
    if let Some(g) = req.game_dir.as_deref().filter(|g| !g.is_empty()) {
        guard::confine(roots, Path::new(g)).map_err(reject)?;
    }
    if let Some(a) = req.extra_jvm_args.iter().find(|a| guard::jvm_arg_is_dangerous(a)) {
        return Err(reject(format!("JVM argument not allowed: {a}")));
    }
    Ok(())
}

/// Returns the (token-masked) command line without launching.
#[tauri::command]
pub fn launch_build_command(request: LaunchRequest) -> Result<String, LaunchError> {
    check_request(&request, &crate::settings::guard::registered_roots())?;
    let (cmd, _) = prepare(&request)?;
    Ok(cmd.display(&request.auth))
}

/// Returns an instance id used by `launch://log` / `launch://exit` events.
#[tauri::command]
pub fn launch_start(app: AppHandle, request: LaunchRequest, log_dir: Option<String>) -> Result<String, LaunchError> {
    use crate::settings::guard;
    let mut roots = guard::registered_roots();
    check_request(&request, &roots)?;
    // Game logs may only be written below a registered folder or the launcher's data directory.
    roots.push(crate::settings::store::data_dir());
    let log_dir = match log_dir.filter(|d| !d.is_empty()) {
        Some(d) => Some(guard::confine(&roots, Path::new(&d)).map_err(reject)?),
        None => None,
    };
    spawn(app, &request, log_dir)
}

#[tauri::command]
pub fn launch_kill(id: String) -> Result<(), LaunchError> {
    kill_by_id(&id)
}

/// How long a game may take to shut down after SIGTERM before it is killed outright.
pub const KILL_GRACE: Duration = Duration::from_secs(5);

pub fn kill_by_id(id: &str) -> Result<(), LaunchError> {
    kill_with_grace(id, KILL_GRACE)
}

/// Ask the game to quit (SIGTERM); if it is still running after `grace` (a hung client, a mod
/// ignoring the signal), kill it (SIGKILL). The later exit is reported as user-initiated.
pub fn kill_with_grace(id: &str, grace: Duration) -> Result<(), LaunchError> {
    let pid = running().lock().unwrap().get(id).copied().ok_or_else(|| LaunchError::NotRunning(id.to_string()))?;
    killed().lock().unwrap().insert(id.to_string());
    #[cfg(unix)]
    {
        let id = id.to_string();
        std::thread::spawn(move || {
            std::thread::sleep(grace);
            // Same pid still registered under this launch id: the process has not been reaped yet.
            if running().lock().unwrap().get(&id).copied() == Some(pid) {
                let _ = Command::new("kill").arg("-KILL").arg(pid.to_string()).status();
            }
        });
    }
    #[cfg(not(unix))]
    let _ = grace;
    #[cfg(unix)]
    {
        let _ = Command::new("kill").arg("-TERM").arg(pid.to_string()).status();
    }
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill").args(["/PID", &pid.to_string(), "/F"]).status();
    }
    Ok(())
}

#[tauri::command]
pub fn launch_list_running() -> Vec<String> {
    running().lock().unwrap().keys().cloned().collect()
}

/// Missing asset objects for a version (for the download module).
#[tauri::command]
pub fn launch_missing_assets(minecraft_dir: String, version_id: String) -> Result<Vec<assets::MissingAsset>, LaunchError> {
    let roots = crate::settings::guard::registered_roots();
    let probe = LaunchRequest { minecraft_dir: minecraft_dir.clone(), version_id: version_id.clone(), ..empty_request() };
    check_request(&probe, &roots)?;
    let mc = Path::new(&minecraft_dir);
    let r = resolve_version(mc, &version_id)?;
    let id = r.json.asset_index.map(|a| a.id).or(r.json.assets).unwrap_or_default();
    let idx = assets::load_index(&mc.join("assets"), &id)?;
    Ok(assets::missing_assets(&mc.join("assets"), &idx))
}

fn empty_request() -> LaunchRequest {
    LaunchRequest {
        minecraft_dir: String::new(),
        version_id: String::new(),
        auth: AuthData { name: String::new(), uuid: String::new(), access_token: String::new(), user_type: String::new() },
        java_path: None,
        game_dir: None,
        max_memory_mb: None,
        min_memory_mb: None,
        extra_jvm_args: vec![],
        extra_game_args: vec![],
        demo: false,
        resolution: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req_in(root: &Path) -> LaunchRequest {
        LaunchRequest { minecraft_dir: root.display().to_string(), version_id: "1.20.1".into(), ..empty_request() }
    }

    #[test]
    fn webview_requests_are_validated() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("mc");
        std::fs::create_dir_all(root.join("versions/1.20.1")).unwrap();
        let roots = vec![root.clone()];
        assert!(check_request(&req_in(&root), &roots).is_ok());

        // Unregistered folder, traversal in the version id.
        assert!(check_request(&req_in(d.path()), &roots).is_err());
        let mut r = req_in(&root);
        r.version_id = "../../etc".into();
        assert!(check_request(&r, &roots).is_err());
        r.version_id = "".into();
        assert!(check_request(&r, &roots).is_err());

        // java_path must be a real `java`, not a shell or a script called java.
        let mut r = req_in(&root);
        r.java_path = Some("/bin/sh".into());
        assert!(check_request(&r, &roots).is_err());
        r.java_path = Some("/usr/bin/open".into());
        assert!(check_request(&r, &roots).is_err());
        let script = d.path().join("bin/java");
        std::fs::create_dir_all(script.parent().unwrap()).unwrap();
        std::fs::write(&script, "#!/bin/sh\ntouch /tmp/pcl-should-not-run\n").unwrap();
        r.java_path = Some(script.display().to_string());
        assert!(check_request(&r, &roots).is_err());

        // game_dir must stay inside the registered folder.
        let mut r = req_in(&root);
        r.game_dir = Some(d.path().join("elsewhere").display().to_string());
        assert!(check_request(&r, &roots).is_err());
        r.game_dir = Some(root.join("versions/1.20.1").display().to_string());
        assert!(check_request(&r, &roots).is_ok());

        // Code-loading JVM flags are refused, memory flags are fine.
        let mut r = req_in(&root);
        r.extra_jvm_args = vec!["-Xmx2G".into(), "-javaagent:/tmp/evil.jar".into()];
        assert!(check_request(&r, &roots).is_err());
        r.extra_jvm_args = vec!["-Xmx2G".into(), "-XX:+UseG1GC".into()];
        assert!(check_request(&r, &roots).is_ok());
    }

    #[test]
    fn named_client_jar_is_copied_once_and_refreshed() {
        let mc = tempfile::tempdir().unwrap();
        let write = |id: &str, bytes: &[u8]| {
            std::fs::create_dir_all(mc.path().join("versions").join(id)).unwrap();
            std::fs::write(version_jar_path(mc.path(), id), bytes).unwrap();
        };
        let forge = |id: &str| -> version::ResolvedVersion {
            let child = version::parse_version_json(&format!(
                r#"{{"id":"{id}","inheritsFrom":"1.20.1","mainClass":"cpw.mods.bootstraplauncher.BootstrapLauncher","arguments":{{"jvm":["-DignoreList=a,${{version_name}}.jar"]}}}}"#
            ))
            .unwrap();
            let parent = version::parse_version_json(r#"{"id":"1.20.1","mainClass":"net.minecraft.client.main.Main"}"#).unwrap();
            version::resolve_with(id, &|i| Ok(if i == "1.20.1" { parent.clone() } else { child.clone() })).unwrap()
        };
        write("1.20.1", b"vanilla-client-v1");
        let v = forge("Pack One");
        ensure_version_named_client_jar(mc.path(), &v).unwrap();
        let dst = version_jar_path(mc.path(), "Pack One");
        assert_eq!(std::fs::read(&dst).unwrap(), b"vanilla-client-v1");
        assert!(!dst.with_extension("jar.tmp").exists());
        // unchanged source: nothing to do; changed source: refreshed
        ensure_version_named_client_jar(mc.path(), &v).unwrap();
        write("1.20.1", b"vanilla-client-v2-longer");
        ensure_version_named_client_jar(mc.path(), &v).unwrap();
        assert_eq!(std::fs::read(&dst).unwrap(), b"vanilla-client-v2-longer");
        // versions that do not ask for it are left alone, and a missing source is not an error here
        let plain = version::resolve_with("1.20.1", &|_| version::parse_version_json(r#"{"id":"1.20.1","mainClass":"x"}"#)).unwrap();
        ensure_version_named_client_jar(mc.path(), &plain).unwrap();
        let other = forge("Pack Two");
        std::fs::remove_file(version_jar_path(mc.path(), "1.20.1")).unwrap();
        ensure_version_named_client_jar(mc.path(), &other).unwrap();
        assert!(!version_jar_path(mc.path(), "Pack Two").exists());
    }

    #[cfg(unix)]
    #[test]
    fn game_does_not_inherit_dyld_variables() {
        let mut c = Command::new("/usr/bin/env");
        c.env("DYLD_FALLBACK_LIBRARY_PATH", "/long/path").env("DYLD_INSERT_LIBRARIES", "/evil.dylib").env("KEEP_ME", "1");
        clean_game_env(&mut c, ["DYLD_FALLBACK_LIBRARY_PATH", "DYLD_INSERT_LIBRARIES", "PATH"].iter().map(std::ffi::OsString::from));
        let out = c.output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(!text.contains("DYLD_"), "{text}");
        assert!(text.contains("KEEP_ME=1") && text.contains("PATH="), "other variables survive: {text}");
    }

    #[test]
    fn precheck_reports_missing_version() {
        let req = LaunchRequest {
            minecraft_dir: "/nonexistent-mc".into(),
            version_id: "x".into(),
            auth: AuthData { name: "".into(), uuid: "u".into(), access_token: "t".into(), user_type: "msa".into() },
            java_path: None,
            game_dir: None,
            max_memory_mb: Some(0),
            min_memory_mb: None,
            extra_jvm_args: vec![],
            extra_game_args: vec![],
            demo: false,
            resolution: None,
        };
        let (r, v) = precheck(&req);
        assert!(v.is_none() && !r.ok);
        assert!(r.issues.contains(&PrecheckIssue::EmptyPlayerName));
        assert!(r.issues.contains(&PrecheckIssue::InvalidMemory));
    }
}
