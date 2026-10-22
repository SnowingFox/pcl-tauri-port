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
        engine::download_batch(&self.client, &self.engine(), files, &self.ctl, &self.tracker).await
    }
    pub async fn file(&self, spec: FileSpec) -> DlResult<()> {
        engine::download_file(&self.client, &self.engine(), &spec, &self.ctl, &self.tracker).await
    }
    /// Fetch a small document into memory, honouring the mirror setting.
    pub async fn get_bytes(&self, url: &str) -> DlResult<Vec<u8>> {
        fetch_bytes(&self.client, self.cfg.source, url).await
    }
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
            let res = f(ctx).await;
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
        if !force {
            if let Some((t, m)) = g.as_ref() {
                if t.elapsed().as_secs() < 300 {
                    return Ok(m.clone());
                }
            }
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

#[tauri::command]
pub fn download_set_config(m: Mgr<'_>, config: DownloadConfig) {
    *m.config.lock().unwrap() = config.clone();
    crate::app::config::persist_download_config(&config);
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

/// Generic file download task (e.g. mods). Returns task id.
#[tauri::command]
pub fn download_start_file(m: Mgr<'_>, req: FileRequest) -> String {
    let label = req.dest.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let spec = FileSpec::new(req.url, req.dest).with_sha1(req.sha1).with_size(req.size);
    m.spawn("file", &label, move |ctx| async move {
        ctx.tracker.begin_stage("download", 0.0, 1.0, 1, spec.size.unwrap_or(0));
        ctx.file(spec).await
    })
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
