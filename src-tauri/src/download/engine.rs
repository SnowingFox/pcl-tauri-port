//! Single-file and batch download engine: segmented transfers, retries, hash
//! verification, pause/cancel, throttled progress reporting.
//! Concept ported from PCL's ModNet.vb (multi-thread downloader); implementation is original.

use super::error::{DlError, DlResult};
use super::hash::{file_hash, HashKind};
use super::mirror::{candidates, Source};
use super::segment::plan_segments;
use futures_util::StreamExt;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::{watch, Semaphore};

#[derive(Clone, Debug)]
pub struct EngineConfig {
    pub source: Source,
    /// Files downloaded in parallel within a batch.
    pub concurrency: usize,
    /// Max connections per large file.
    pub segments: usize,
    /// Files smaller than segments * this are fetched on one connection.
    pub min_segment: u64,
    /// Attempts per candidate URL.
    pub retries: u32,
    pub user_agent: String,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            source: Source::Official,
            concurrency: 16,
            segments: 4,
            min_segment: 2 * 1024 * 1024,
            retries: 3,
            user_agent: "PCL-Tauri-Port/0.1".into(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct FileSpec {
    pub urls: Vec<String>,
    pub dest: PathBuf,
    pub size: Option<u64>,
    pub sha1: Option<String>,
    pub sha256: Option<String>,
}

impl FileSpec {
    pub fn new(url: impl Into<String>, dest: impl Into<PathBuf>) -> Self {
        Self { urls: vec![url.into()], dest: dest.into(), ..Default::default() }
    }
    pub fn with_sha1(mut self, h: Option<String>) -> Self {
        self.sha1 = h.filter(|s| !s.is_empty());
        self
    }
    pub fn with_size(mut self, s: Option<u64>) -> Self {
        self.size = s.filter(|n| *n > 0);
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RunState {
    Running,
    Paused,
    Cancelled,
}

/// Pause / resume / cancel handle shared by all workers of a task.
#[derive(Clone)]
pub struct TaskCtl {
    tx: Arc<watch::Sender<RunState>>,
}

impl Default for TaskCtl {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskCtl {
    pub fn new() -> Self {
        Self { tx: Arc::new(watch::channel(RunState::Running).0) }
    }
    pub fn state(&self) -> RunState {
        *self.tx.borrow()
    }
    pub fn pause(&self) {
        self.tx.send_if_modified(|s| {
            if *s == RunState::Running {
                *s = RunState::Paused;
                true
            } else {
                false
            }
        });
    }
    pub fn resume(&self) {
        self.tx.send_if_modified(|s| {
            if *s == RunState::Paused {
                *s = RunState::Running;
                true
            } else {
                false
            }
        });
    }
    pub fn cancel(&self) {
        self.tx.send_replace(RunState::Cancelled);
    }
    /// Blocks while paused; errors if cancelled.
    pub async fn checkpoint(&self) -> DlResult<()> {
        let mut rx = self.tx.subscribe();
        loop {
            match *rx.borrow_and_update() {
                RunState::Running => return Ok(()),
                RunState::Cancelled => return Err(DlError::Cancelled),
                RunState::Paused => {}
            }
            if rx.changed().await.is_err() {
                return Err(DlError::Cancelled);
            }
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ProgressEvent {
    pub task_id: String,
    pub stage: String,
    pub done_bytes: u64,
    pub total_bytes: u64,
    pub done_files: u64,
    pub total_files: u64,
    pub speed_bps: u64,
    /// Overall task fraction 0..1 (set by the task runner; stage-local otherwise).
    pub fraction: f64,
    pub state: RunState,
}

pub type Sink = Arc<dyn Fn(&ProgressEvent) + Send + Sync>;

/// Shared counters for a batch; emits throttled progress events.
pub struct Tracker {
    pub task_id: String,
    pub stage: Mutex<String>,
    pub done_bytes: AtomicU64,
    pub total_bytes: AtomicU64,
    pub done_files: AtomicU64,
    pub total_files: AtomicU64,
    /// (weight_start, weight_span) mapping stage fraction into task fraction.
    pub weight: Mutex<(f64, f64)>,
    /// (offset, factor) applied on top of the stage weight, for tasks made of several phases.
    pub scale: Mutex<(f64, f64)>,
    last: Mutex<(Instant, u64, u64)>,
    sink: Sink,
    ctl: TaskCtl,
}

impl Tracker {
    pub fn new(task_id: &str, sink: Sink, ctl: TaskCtl) -> Arc<Self> {
        Arc::new(Self {
            task_id: task_id.into(),
            stage: Mutex::new(String::new()),
            done_bytes: 0.into(),
            total_bytes: 0.into(),
            done_files: 0.into(),
            total_files: 0.into(),
            weight: Mutex::new((0.0, 1.0)),
            scale: Mutex::new((0.0, 1.0)),
            last: Mutex::new((Instant::now(), 0, 0)),
            sink,
            ctl,
        })
    }

    /// Map the stage fractions of the following stages into `offset..offset+factor`.
    pub fn set_scale(&self, offset: f64, factor: f64) {
        *self.scale.lock().unwrap() = (offset, factor);
    }

    pub fn begin_stage(&self, name: &str, start: f64, span: f64, files: u64, bytes: u64) {
        *self.stage.lock().unwrap() = name.into();
        *self.weight.lock().unwrap() = (start, span);
        self.done_bytes.store(0, Ordering::Relaxed);
        self.done_files.store(0, Ordering::Relaxed);
        self.total_files.store(files, Ordering::Relaxed);
        self.total_bytes.store(bytes, Ordering::Relaxed);
        *self.last.lock().unwrap() = (Instant::now(), 0, 0);
        self.emit(true);
    }

    pub fn add_bytes(&self, n: u64) {
        self.done_bytes.fetch_add(n, Ordering::Relaxed);
        self.emit(false);
    }
    pub fn sub_bytes(&self, n: u64) {
        let _ = self.done_bytes.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| Some(v.saturating_sub(n)));
    }
    pub fn file_done(&self) {
        self.done_files.fetch_add(1, Ordering::Relaxed);
        self.emit(false);
    }

    pub fn snapshot(&self, speed: u64) -> ProgressEvent {
        let db = self.done_bytes.load(Ordering::Relaxed);
        let tb = self.total_bytes.load(Ordering::Relaxed);
        let df = self.done_files.load(Ordering::Relaxed);
        let tf = self.total_files.load(Ordering::Relaxed);
        let local = if tb > 0 {
            (db as f64 / tb as f64).min(1.0)
        } else if tf > 0 {
            (df as f64 / tf as f64).min(1.0)
        } else {
            0.0
        };
        let (s, w) = *self.weight.lock().unwrap();
        let (off, fac) = *self.scale.lock().unwrap();
        ProgressEvent {
            task_id: self.task_id.clone(),
            stage: self.stage.lock().unwrap().clone(),
            done_bytes: db,
            total_bytes: tb,
            done_files: df,
            total_files: tf,
            speed_bps: speed,
            fraction: off + fac * (s + w * local),
            state: self.ctl.state(),
        }
    }

    pub fn emit(&self, force: bool) {
        let mut last = self.last.lock().unwrap();
        let el = last.0.elapsed();
        if !force && el.as_millis() < 150 {
            return;
        }
        let db = self.done_bytes.load(Ordering::Relaxed);
        let secs = el.as_secs_f64().max(0.001);
        let speed = (db.saturating_sub(last.1) as f64 / secs) as u64;
        *last = (Instant::now(), db, speed);
        drop(last);
        (self.sink)(&self.snapshot(speed));
    }
}

/// Is the file already present and valid?
pub async fn is_valid(spec: &FileSpec) -> bool {
    let Ok(meta) = tokio::fs::metadata(&spec.dest).await else { return false };
    if !meta.is_file() {
        return false;
    }
    if let Some(sz) = spec.size {
        if meta.len() != sz {
            return false;
        }
    }
    if let Some(h) = &spec.sha1 {
        return file_hash(HashKind::Sha1, spec.dest.clone()).await.map(|a| a.eq_ignore_ascii_case(h)).unwrap_or(false);
    }
    if let Some(h) = &spec.sha256 {
        return file_hash(HashKind::Sha256, spec.dest.clone()).await.map(|a| a.eq_ignore_ascii_case(h)).unwrap_or(false);
    }
    // No hash and no size: any non-empty file counts.
    spec.size.is_some() || meta.len() > 0
}

async fn verify(spec: &FileSpec, path: &Path) -> DlResult<()> {
    let meta = tokio::fs::metadata(path).await?;
    if let Some(sz) = spec.size {
        if meta.len() != sz {
            return Err(DlError::Size { path: path.display().to_string(), expected: sz, actual: meta.len() });
        }
    }
    for (kind, exp) in [(HashKind::Sha1, &spec.sha1), (HashKind::Sha256, &spec.sha256)] {
        if let Some(exp) = exp {
            let act = file_hash(kind, path.to_path_buf()).await?;
            if !act.eq_ignore_ascii_case(exp) {
                return Err(DlError::Hash { path: path.display().to_string(), expected: exp.clone(), actual: act });
            }
        }
    }
    Ok(())
}

fn part_path(dest: &Path) -> PathBuf {
    let mut s = dest.as_os_str().to_owned();
    s.push(".part");
    PathBuf::from(s)
}

/// Download one file (skips if already valid). Tries mirror candidates in order,
/// each with `cfg.retries` attempts and exponential backoff.
pub async fn download_file(
    client: &reqwest::Client,
    cfg: &EngineConfig,
    spec: &FileSpec,
    ctl: &TaskCtl,
    tracker: &Arc<Tracker>,
) -> DlResult<()> {
    ctl.checkpoint().await?;
    if is_valid(spec).await {
        if let Some(sz) = spec.size {
            tracker.add_bytes(sz);
        }
        return Ok(());
    }
    if let Some(parent) = spec.dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let urls: Vec<String> = spec.urls.iter().flat_map(|u| candidates(u, cfg.source)).fold(Vec::new(), |mut acc, u| {
        if !acc.contains(&u) {
            acc.push(u);
        }
        acc
    });
    let mut last_err = DlError::Other("no url".into());
    for url in urls {
        for attempt in 0..cfg.retries.max(1) {
            ctl.checkpoint().await?;
            let counted = Arc::new(AtomicU64::new(0));
            match try_once(client, cfg, spec, &url, ctl, tracker, &counted).await {
                Ok(()) => return Ok(()),
                Err(DlError::Cancelled) => return Err(DlError::Cancelled),
                Err(e) => {
                    tracker.sub_bytes(counted.load(Ordering::Relaxed));
                    last_err = e;
                    // 4xx other than 408/429 will not get better on retry.
                    if let DlError::Status(c, _) = &last_err {
                        if (400..500).contains(c) && *c != 408 && *c != 429 {
                            break;
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(200 * (1 << attempt.min(5)))).await;
                }
            }
        }
    }
    Err(last_err)
}

async fn try_once(
    client: &reqwest::Client,
    cfg: &EngineConfig,
    spec: &FileSpec,
    url: &str,
    ctl: &TaskCtl,
    tracker: &Arc<Tracker>,
    counted: &Arc<AtomicU64>,
) -> DlResult<()> {
    let part = part_path(&spec.dest);
    let big = spec.size.map(|s| cfg.segments > 1 && s >= cfg.min_segment * 2).unwrap_or(false);
    let res = if big {
        match segmented(client, cfg, spec, url, &part, ctl, tracker, counted).await {
            Err(DlError::Other(m)) if m == "no-range" => {
                tracker.sub_bytes(counted.swap(0, Ordering::Relaxed));
                single(client, spec, url, &part, ctl, tracker, counted).await
            }
            r => r,
        }
    } else {
        single(client, spec, url, &part, ctl, tracker, counted).await
    };
    if let Err(e) = res {
        // Keep the partial only for a resumable single-stream failure; drop on hash/size problems.
        if matches!(e, DlError::Hash { .. } | DlError::Size { .. }) {
            let _ = tokio::fs::remove_file(&part).await;
        }
        return Err(e);
    }
    if let Err(e) = verify(spec, &part).await {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(e);
    }
    tokio::fs::rename(&part, &spec.dest).await?;
    tracker.file_done();
    Ok(())
}

async fn check_status(resp: reqwest::Response, url: &str) -> DlResult<reqwest::Response> {
    if resp.status().is_success() {
        Ok(resp)
    } else {
        Err(DlError::Status(resp.status().as_u16(), url.to_string()))
    }
}

/// One connection; resumes from an existing `.part` via Range when possible.
async fn single(
    client: &reqwest::Client,
    spec: &FileSpec,
    url: &str,
    part: &Path,
    ctl: &TaskCtl,
    tracker: &Tracker,
    counted: &Arc<AtomicU64>,
) -> DlResult<()> {
    let mut existing = tokio::fs::metadata(part).await.map(|m| m.len()).unwrap_or(0);
    if let Some(sz) = spec.size {
        if existing > sz {
            existing = 0;
        }
    }
    let mut req = client.get(url);
    if existing > 0 {
        req = req.header("Range", format!("bytes={existing}-"));
    }
    let resp = req.send().await?;
    if resp.status().as_u16() == 416 && existing > 0 {
        // Partial is complete or stale; let verification decide.
        return Ok(());
    }
    let resp = check_status(resp, url).await?;
    let resumed = existing > 0 && resp.status().as_u16() == 206;
    let mut file = if resumed {
        tracker.add_bytes(existing);
        counted.fetch_add(existing, Ordering::Relaxed);
        tokio::fs::OpenOptions::new().append(true).open(part).await?
    } else {
        tokio::fs::File::create(part).await?
    };
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        ctl.checkpoint().await?;
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        counted.fetch_add(chunk.len() as u64, Ordering::Relaxed);
        tracker.add_bytes(chunk.len() as u64);
    }
    file.flush().await?;
    Ok(())
}

/// Several parallel Range requests writing into one preallocated file.
/// Not resumable across process restarts (stage-level resume skips finished files).
#[allow(clippy::too_many_arguments)]
async fn segmented(
    client: &reqwest::Client,
    cfg: &EngineConfig,
    spec: &FileSpec,
    url: &str,
    part: &Path,
    ctl: &TaskCtl,
    tracker: &Arc<Tracker>,
    counted: &Arc<AtomicU64>,
) -> DlResult<()> {
    let total = spec.size.unwrap();
    let ranges = plan_segments(total, cfg.segments, cfg.min_segment);
    {
        let f = tokio::fs::File::create(part).await?;
        f.set_len(total).await?;
    }
    let mut set = tokio::task::JoinSet::new();
    for (s, e) in ranges {
        let client = client.clone();
        let url = url.to_string();
        let part = part.to_path_buf();
        let ctl = ctl.clone();
        let counted = counted.clone();
        let tracker = tracker.clone();
        set.spawn(async move {
            let resp = client.get(&url).header("Range", format!("bytes={s}-{e}")).send().await?;
            let resp = check_status(resp, &url).await?;
            if resp.status().as_u16() != 206 {
                return Err(DlError::Other("no-range".into()));
            }
            let mut f = tokio::fs::OpenOptions::new().write(true).open(&part).await?;
            f.seek(std::io::SeekFrom::Start(s)).await?;
            let mut stream = resp.bytes_stream();
            let mut got = 0u64;
            while let Some(chunk) = stream.next().await {
                ctl.checkpoint().await?;
                let chunk = chunk?;
                f.write_all(&chunk).await?;
                got += chunk.len() as u64;
                counted.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                tracker.add_bytes(chunk.len() as u64);
            }
            f.flush().await?;
            if got != e - s + 1 {
                return Err(DlError::Size { path: part.display().to_string(), expected: e - s + 1, actual: got });
            }
            Ok::<(), DlError>(())
        });
    }
    let mut result = Ok(());
    while let Some(r) = set.join_next().await {
        match r {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                if result.is_ok() {
                    result = Err(e);
                    set.abort_all();
                }
            }
            Err(_) => {}
        }
    }
    result
}

/// Download many files with bounded concurrency. Fails fast on first error.
pub async fn download_batch(
    client: &reqwest::Client,
    cfg: &EngineConfig,
    files: Vec<FileSpec>,
    ctl: &TaskCtl,
    tracker: &Arc<Tracker>,
) -> DlResult<()> {
    let sem = Arc::new(Semaphore::new(cfg.concurrency.max(1)));
    let mut set = tokio::task::JoinSet::new();
    for spec in files {
        let permit = sem.clone().acquire_owned().await.unwrap();
        let (client, cfg, ctl, tracker) = (client.clone(), cfg.clone(), ctl.clone(), tracker.clone());
        set.spawn(async move {
            let _p = permit;
            download_file(&client, &cfg, &spec, &ctl, &tracker).await
        });
        // Surface early failures without queueing the rest.
        while let Some(done) = set.try_join_next() {
            if let Err(e) = flatten(done) {
                set.abort_all();
                return Err(e);
            }
        }
    }
    while let Some(done) = set.join_next().await {
        if let Err(e) = flatten(done) {
            set.abort_all();
            return Err(e);
        }
    }
    tracker.emit(true);
    Ok(())
}

fn flatten(r: Result<DlResult<()>, tokio::task::JoinError>) -> DlResult<()> {
    r.map_err(|e| DlError::Other(e.to_string()))?
}
