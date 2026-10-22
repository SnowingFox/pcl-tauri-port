//! Download subsystem: engine, mirrors, manifest, and the task manager exposed to the UI.
//! Derived in concept from Plain Craft Launcher (ModNet.vb / ModDownload.vb); see repo LICENCE notes.

pub mod engine;
pub mod error;
pub mod hash;
pub mod manifest;
pub mod mirror;
pub mod segment;

#[cfg(test)]
mod tests;

use engine::{EngineConfig, FileSpec, ProgressEvent, Sink, TaskCtl, Tracker};
use error::{DlError, DlResult};
use mirror::Source;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};

pub const EVT_PROGRESS: &str = "download://progress";
pub const EVT_FINISHED: &str = "download://finished";

/// User-facing settings. `game_dir` is a `.minecraft`-style directory.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DownloadConfig {
    pub game_dir: PathBuf,
    pub source: Source,
    pub concurrency: usize,
    pub segments: usize,
    pub retries: u32,
}

impl Default for DownloadConfig {
    fn default() -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        Self {
            game_dir: home.join("Library/Application Support/minecraft"),
            source: Source::Official,
            concurrency: 16,
            segments: 4,
            retries: 3,
        }
    }
}

impl DownloadConfig {
    pub fn engine(&self) -> EngineConfig {
        EngineConfig {
            source: self.source,
            concurrency: self.concurrency,
            segments: self.segments,
            retries: self.retries,
            ..Default::default()
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Serialize)]
pub struct TaskInfo {
    pub task_id: String,
    pub kind: String,
    pub label: String,
    pub status: TaskStatus,
    pub error: Option<String>,
    pub last_progress: Option<ProgressEvent>,
}

/// Everything a running task needs.
#[derive(Clone)]
pub struct TaskCtx {
    pub client: reqwest::Client,
    pub cfg: DownloadConfig,
    pub ctl: TaskCtl,
    pub tracker: Arc<Tracker>,
}

impl TaskCtx {
    pub fn engine(&self) -> EngineConfig {
        self.cfg.engine()
    }
    pub async fn batch(&self, files: Vec<FileSpec>) -> DlResult<()> {
        files.iter().try_for_each(check_spec_urls)?;
        engine::download_batch(&self.client, &self.engine(), files, &self.ctl, &self.tracker).await
    }
    pub async fn file(&self, spec: FileSpec) -> DlResult<()> {
        check_spec_urls(&spec)?;
        engine::download_file(&self.client, &self.engine(), &spec, &self.ctl, &self.tracker).await
    }
    /// Fetch a small document into memory, honouring the mirror setting.
    pub async fn get_bytes(&self, url: &str) -> DlResult<Vec<u8>> {
        fetch_bytes(&self.client, self.cfg.source, url).await
    }
}

/// URLs inside version JSONs, loader profiles and modpacks are remote data: only http(s) to
/// public hosts may be fetched (no `file:`, loopback, link-local metadata endpoints, LAN).
fn check_spec_urls(spec: &FileSpec) -> DlResult<()> {
    for u in &spec.urls {
        crate::settings::guard::check_remote_url(u, crate::settings::guard::HostPolicy::Public)
            .map_err(|e| DlError::Other(format!("{e}: {u}")))?;
    }
    Ok(())
}

pub async fn fetch_bytes(client: &reqwest::Client, source: Source, url: &str) -> DlResult<Vec<u8>> {
    let mut last = DlError::Other("no url".into());
    for u in mirror::candidates(url, source) {
        for attempt in 0..3u32 {
            match client.get(&u).send().await {
                Ok(r) if r.status().is_success() => match r.bytes().await {
                    Ok(b) => return Ok(b.to_vec()),
                    Err(e) => last = e.into(),
                },
                Ok(r) => {
                    last = DlError::Status(r.status().as_u16(), u.clone());
                    if r.status().is_client_error() {
                        break;
                    }
                }
                Err(e) => last = e.into(),
            }
            tokio::time::sleep(std::time::Duration::from_millis(200 * (1 << attempt))).await;
        }
    }
    Err(last)
}

struct Entry {
    info: Mutex<TaskInfo>,
    ctl: TaskCtl,
}

pub type FinishHook = Arc<dyn Fn(&TaskInfo) + Send + Sync>;

/// Per-task options for [`Manager::spawn_with`].
#[derive(Default)]
pub struct SpawnOpts {
    /// Config snapshot for this task instead of the manager's current one (e.g. another game dir).
    pub cfg: Option<DownloadConfig>,
    /// Extra progress listener, called in addition to the global `download://progress` sink.
    pub on_progress: Option<Sink>,
}

/// Registry of running/finished tasks. Event delivery is injected so tests need no Tauri runtime.
pub struct Manager {
    pub config: Mutex<DownloadConfig>,
    pub client: reqwest::Client,
    tasks: Mutex<HashMap<String, Arc<Entry>>>,
    next: AtomicU64,
    sink: Sink,
    on_finish: FinishHook,
    manifest_cache: tokio::sync::Mutex<Option<(std::time::Instant, Arc<manifest::VersionManifest>)>>,
    manifest_url: Mutex<String>,
}

impl Manager {
    pub fn new(sink: Sink, on_finish: FinishHook) -> Arc<Self> {
        Arc::new(Self {
            config: Mutex::new(DownloadConfig::default()),
            client: reqwest::Client::builder()
                .user_agent("PCL-Tauri-Port/0.1")
                .redirect(crate::settings::guard::redirect_policy(crate::settings::guard::HostPolicy::Public))
                .dns_resolver(Arc::new(crate::settings::guard::GuardedResolver(crate::settings::guard::HostPolicy::Public)))
                .connect_timeout(std::time::Duration::from_secs(15))
                .read_timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("http client"),
            tasks: Mutex::new(HashMap::new()),
            next: AtomicU64::new(1),
            sink,
            on_finish,
            manifest_cache: tokio::sync::Mutex::new(None),
            manifest_url: Mutex::new(manifest::MANIFEST_URL.to_string()),
        })
    }

    /// Point the version manifest lookup somewhere else (test servers). Clears the cache.
    #[cfg(test)]
    pub fn set_manifest_url(&self, url: &str) {
        *self.manifest_url.lock().unwrap() = url.to_string();
        if let Ok(mut c) = self.manifest_cache.try_lock() {
            *c = None;
        }
    }

    pub fn cfg(&self) -> DownloadConfig {
        self.config.lock().unwrap().clone()
    }

    /// Start a task. `f` receives the context and returns when finished.
    pub fn spawn<F, Fut>(self: &Arc<Self>, kind: &str, label: &str, f: F) -> String
    where
        F: FnOnce(TaskCtx) -> Fut + Send + 'static,
        Fut: Future<Output = DlResult<()>> + Send + 'static,
    {
        self.spawn_with(SpawnOpts::default(), kind, label, f)
    }

    /// Like [`Manager::spawn`] but with per-task options (config snapshot override, progress tap).
    pub fn spawn_with<F, Fut>(self: &Arc<Self>, opts: SpawnOpts, kind: &str, label: &str, f: F) -> String
    where
        F: FnOnce(TaskCtx) -> Fut + Send + 'static,
        Fut: Future<Output = DlResult<()>> + Send + 'static,
    {
        let id = format!("task-{}", self.next.fetch_add(1, Ordering::Relaxed));
        let ctl = TaskCtl::new();
        let sink: Sink = match opts.on_progress {
            Some(tap) => {
                let global = self.sink.clone();
                Arc::new(move |p: &ProgressEvent| {
                    global(p);
                    tap(p);
                })
            }
            None => self.sink.clone(),
        };
        let tracker = Tracker::new(&id, sink, ctl.clone());
        let entry = Arc::new(Entry {
            info: Mutex::new(TaskInfo {
                task_id: id.clone(),
                kind: kind.into(),
                label: label.into(),
                status: TaskStatus::Running,
                error: None,
                last_progress: None,
            }),
            ctl: ctl.clone(),
        });
        self.tasks.lock().unwrap().insert(id.clone(), entry.clone());
        let ctx = TaskCtx { client: self.client.clone(), cfg: opts.cfg.unwrap_or_else(|| self.cfg()), ctl, tracker: tracker.clone() };
        let this = self.clone();
        let fut = async move {
            // Run the task body as its own task so a panic is contained and reported.
            let res = match tokio::spawn(f(ctx)).await {
                Ok(r) => r,
                Err(e) if e.is_panic() => Err(DlError::Other("internal error: the task panicked (see the launcher log)".into())),
                Err(_) => Err(DlError::Cancelled),
            };
            let mut info = entry.info.lock().unwrap();
            info.last_progress = Some(tracker.snapshot(0));
            match res {
                Ok(()) => info.status = TaskStatus::Completed,
                Err(DlError::Cancelled) => info.status = TaskStatus::Cancelled,
                Err(e) => {
                    info.status = TaskStatus::Failed;
                    info.error = Some(e.to_string());
                }
            }
            let snap = info.clone();
            drop(info);
            (this.on_finish)(&snap);
        };
        // Sync Tauri commands run on the main thread, which has no Tokio context.
        match tokio::runtime::Handle::try_current() {
            Ok(h) => {
                h.spawn(fut);
            }
            Err(_) => {
                tauri::async_runtime::spawn(fut);
            }
        }
        id
    }

    fn entry(&self, id: &str) -> DlResult<Arc<Entry>> {
        self.tasks.lock().unwrap().get(id).cloned().ok_or_else(|| DlError::Other(format!("unknown task {id}")))
    }
    pub fn pause(&self, id: &str) -> DlResult<()> {
        let e = self.entry(id)?;
        e.ctl.pause();
        let mut i = e.info.lock().unwrap();
        if i.status == TaskStatus::Running {
            i.status = TaskStatus::Paused;
        }
        Ok(())
    }
    pub fn resume(&self, id: &str) -> DlResult<()> {
        let e = self.entry(id)?;
        e.ctl.resume();
        let mut i = e.info.lock().unwrap();
        if i.status == TaskStatus::Paused {
            i.status = TaskStatus::Running;
        }
        Ok(())
    }
    pub fn cancel(&self, id: &str) -> DlResult<()> {
        self.entry(id)?.ctl.cancel();
        Ok(())
    }
    pub fn info(&self, id: &str) -> DlResult<TaskInfo> {
        Ok(self.entry(id)?.info.lock().unwrap().clone())
    }
    pub fn list(&self) -> Vec<TaskInfo> {
        let mut v: Vec<_> = self.tasks.lock().unwrap().values().map(|e| e.info.lock().unwrap().clone()).collect();
        v.sort_by_key(|i| i.task_id.trim_start_matches("task-").parse::<u64>().unwrap_or(0));
        v
    }
    pub fn forget_finished(&self) {
        self.tasks.lock().unwrap().retain(|_, e| matches!(e.info.lock().unwrap().status, TaskStatus::Running | TaskStatus::Paused));
    }

    /// Version manifest, cached in memory for 5 minutes (and on disk for offline use).
    pub async fn version_manifest(&self, force: bool) -> DlResult<Arc<manifest::VersionManifest>> {
        let mut g = self.manifest_cache.lock().await;
        if !force
            && let Some((t, m)) = g.as_ref()
            && t.elapsed().as_secs() < 300
        {
            return Ok(m.clone());
        }
        let cfg = self.cfg();
        let cache_file = cfg.game_dir.join("launcher_version_manifest_cache.json");
        let url = self.manifest_url.lock().unwrap().clone();
        let parsed = match fetch_bytes(&self.client, cfg.source, &url).await {
            Ok(b) => {
                let m = manifest::VersionManifest::parse(&b)?;
                let _ = tokio::fs::create_dir_all(&cfg.game_dir).await;
                let _ = tokio::fs::write(&cache_file, &b).await;
                m
            }
            Err(e) => match tokio::fs::read(&cache_file).await {
                Ok(b) => manifest::VersionManifest::parse(&b)?,
                Err(_) => return Err(e),
            },
        };
        let m = Arc::new(parsed);
        *g = Some((std::time::Instant::now(), m.clone()));
        Ok(m)
    }
}

/// Build the Manager wired to Tauri events.
pub fn new_manager(app: AppHandle) -> Arc<Manager> {
    let a1 = app.clone();
    let sink: Sink = Arc::new(move |p: &ProgressEvent| {
        let _ = a1.emit(EVT_PROGRESS, p);
    });
    let on_finish: FinishHook = Arc::new(move |i: &TaskInfo| {
        let _ = app.emit(EVT_FINISHED, i);
    });
    Manager::new(sink, on_finish)
}

pub type Mgr<'a> = State<'a, Arc<Manager>>;

// ---------------------------------------------------------------- commands

#[tauri::command]
pub fn download_get_config(m: Mgr<'_>) -> DownloadConfig {
    m.cfg()
}

/// Validate a config that came from the webview: the game dir must be inside a registered
/// Minecraft folder (installs write libraries, assets and versions below it) and the tuning
/// values are clamped so a bad value cannot open thousands of connections.
pub(crate) fn sanitize_config(mut c: DownloadConfig, roots: &[PathBuf]) -> Result<DownloadConfig, String> {
    c.game_dir = crate::settings::guard::confine(roots, &c.game_dir)?;
    c.concurrency = c.concurrency.clamp(1, 64);
    c.segments = c.segments.clamp(1, 16);
    c.retries = c.retries.clamp(1, 10);
    Ok(c)
}

#[tauri::command]
pub fn download_set_config(m: Mgr<'_>, config: DownloadConfig) -> Result<(), String> {
    let c = sanitize_config(config, &crate::settings::guard::registered_roots())?;
    *m.config.lock().unwrap() = c.clone();
    // Persist only after the game dir passed the registered-roots check: persisting registers the folder.
    crate::app::config::persist_download_config(&c);
    Ok(())
}

#[tauri::command]
pub async fn download_version_manifest(m: Mgr<'_>, force: Option<bool>) -> Result<manifest::VersionManifest, DlError> {
    Ok((*m.version_manifest(force.unwrap_or(false)).await?).clone())
}

#[derive(Deserialize)]
pub struct FileRequest {
    pub url: String,
    pub dest: PathBuf,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

#[cfg(test)]
mod guard_tests {
    use super::*;

    fn req(url: &str, dest: &std::path::Path) -> FileRequest {
        FileRequest { url: url.into(), dest: dest.to_path_buf(), sha1: None, size: None }
    }

    #[test]
    fn file_requests_are_confined() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("mc");
        std::fs::create_dir_all(root.join("mods")).unwrap();
        std::fs::create_dir_all(root.join("versions/1.20.1")).unwrap();
        let roots = vec![root.clone()];
        let ok = |url: &str, dest: PathBuf| check_file_request(&req(url, &dest), &roots);
        assert!(ok("https://cdn.modrinth.com/a.jar", root.join("mods/a.jar")).is_ok());
        assert!(ok("https://cdn.modrinth.com/a.jar", root.join("versions/1.20.1/mods/a.jar")).is_ok());
        // Destinations that would be code or launcher state.
        for bad in [
            "/etc/cron.d/x",
            "versions/1.20.1/1.20.1.json",
            "libraries/x.jar",
            "launcher_profiles.json",
            "mods/../versions/1.20.1/1.20.1.json",
            "mods",
        ] {
            let dest = if bad.starts_with('/') { PathBuf::from(bad) } else { root.join(bad) };
            assert!(ok("https://cdn.modrinth.com/a.jar", dest).is_err(), "{bad}");
        }
        // URLs: https + public host only.
        for bad in [
            "http://cdn.modrinth.com/a.jar",
            "file:///etc/passwd",
            "https://127.0.0.1/a.jar",
            "https://169.254.169.254/x",
            "ftp://x/a",
            "https://u:p@cdn.modrinth.com/a.jar",
        ] {
            assert!(ok(bad, root.join("mods/a.jar")).is_err(), "{bad}");
        }
        // sha1 shape.
        let mut r = req("https://cdn.modrinth.com/a.jar", &root.join("mods/a.jar"));
        r.sha1 = Some("zz".into());
        assert!(check_file_request(&r, &roots).is_err());
        r.sha1 = Some("da39a3ee5e6b4b0d3255bfef95601890afd80709".into());
        assert!(check_file_request(&r, &roots).is_ok());
    }

    #[tokio::test]
    async fn a_panicking_task_is_reported_as_failed() {
        let (tx, rx) = std::sync::mpsc::channel();
        let m = Manager::new(
            Arc::new(|_| {}),
            Arc::new(move |i: &TaskInfo| {
                let _ = tx.send(i.clone());
            }),
        );
        let id = m.spawn("test", "boom", |_ctx| async move {
            if true {
                panic!("boom");
            }
            Ok(())
        });
        let done = tokio::task::spawn_blocking(move || rx.recv_timeout(std::time::Duration::from_secs(5)))
            .await
            .unwrap()
            .expect("finish hook not called");
        assert_eq!(done.task_id, id);
        assert!(matches!(done.status, TaskStatus::Failed));
        assert!(done.error.unwrap().contains("panicked"));
        assert!(matches!(m.info(&id).unwrap().status, TaskStatus::Failed));
    }

    #[test]
    fn config_is_confined_and_clamped() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("mc");
        std::fs::create_dir_all(&root).unwrap();
        let roots = vec![root.clone()];
        let cfg =
            |dir: PathBuf| DownloadConfig { game_dir: dir, concurrency: 10_000, segments: 0, retries: 999, ..DownloadConfig::default() };
        let c = sanitize_config(cfg(root.clone()), &roots).unwrap();
        assert_eq!((c.concurrency, c.segments, c.retries), (64, 1, 10));
        assert!(sanitize_config(cfg(PathBuf::from("/")), &roots).is_err());
        assert!(sanitize_config(cfg(PathBuf::from("/etc")), &roots).is_err());
        assert!(sanitize_config(cfg(root.join("../x")), &roots).is_err());
    }

    #[test]
    fn spec_urls_are_checked() {
        let ok = FileSpec::new("https://libraries.minecraft.net/a.jar", "/tmp/x");
        assert!(check_spec_urls(&ok).is_ok());
        for bad in ["file:///etc/passwd", "http://127.0.0.1:8000/a", "http://169.254.169.254/latest", "http://localhost/a", "gopher://x/"] {
            assert!(check_spec_urls(&FileSpec::new(bad, "/tmp/x")).is_err(), "{bad}");
        }
    }
}

/// Check a file request that came from the webview: https URL on a public host, destination
/// inside a registered Minecraft folder (never a protected launcher file), sane hash / size.
pub(crate) fn check_file_request(req: &FileRequest, roots: &[PathBuf]) -> Result<PathBuf, String> {
    crate::settings::guard::check_download_url(&req.url)?;
    // Only user content may be written (mods, resource packs, shader packs, saves, config); never
    // version JSONs, libraries, launcher metadata or anything outside a registered folder.
    let (dest, depth) = crate::settings::guard::confine_content(roots, &req.dest)?;
    if depth == 0 || dest.is_dir() {
        return Err("目标必须是内容文件夹中的文件路径".into());
    }
    if std::fs::symlink_metadata(&dest).map(|m| m.file_type().is_symlink()).unwrap_or(false) {
        return Err("目标不能是符号链接".into());
    }
    if let Some(h) = req.sha1.as_deref().filter(|h| !h.is_empty())
        && (h.len() != 40 || !h.chars().all(|c| c.is_ascii_hexdigit()))
    {
        return Err("sha1 格式无效".into());
    }
    Ok(dest)
}

/// Generic file download task (e.g. mods). Returns task id.
#[tauri::command]
pub fn download_start_file(m: Mgr<'_>, req: FileRequest) -> Result<String, String> {
    let dest = check_file_request(&req, &crate::settings::guard::registered_roots())?;
    let req = FileRequest { dest, ..req };
    let label = req.dest.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let spec = FileSpec::new(req.url, req.dest).with_sha1(req.sha1).with_size(req.size);
    Ok(m.spawn("file", &label, move |ctx| async move {
        ctx.tracker.begin_stage("download", 0.0, 1.0, 1, spec.size.unwrap_or(0));
        ctx.file(spec).await
    }))
}

#[tauri::command]
pub fn download_pause(m: Mgr<'_>, task_id: String) -> Result<(), DlError> {
    m.pause(&task_id)
}
#[tauri::command]
pub fn download_resume(m: Mgr<'_>, task_id: String) -> Result<(), DlError> {
    m.resume(&task_id)
}
#[tauri::command]
pub fn download_cancel(m: Mgr<'_>, task_id: String) -> Result<(), DlError> {
    m.cancel(&task_id)
}
#[tauri::command]
pub fn download_task_info(m: Mgr<'_>, task_id: String) -> Result<TaskInfo, DlError> {
    m.info(&task_id)
}
#[tauri::command]
pub fn download_list_tasks(m: Mgr<'_>) -> Vec<TaskInfo> {
    m.list()
}
#[tauri::command]
pub fn download_clear_finished(m: Mgr<'_>) {
    m.forget_finished()
}
