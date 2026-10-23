//! Single-file and batch download engine: segmented transfers, retries, hash
//! verification, pause/cancel, throttled progress reporting.
//! Concept ported from PCL's ModNet.vb (multi-thread downloader); implementation is original.

use super::error::{DlError, DlResult};
use super::hash::{HashKind, file_hash};
use super::health::{self, Plan};
use super::mirror::{Source, candidates};
use super::segment::plan_segments;
use futures_util::StreamExt;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::{Semaphore, watch};

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
    /// A connection that delivers fewer than `stall_min_bytes` within `stall_window` is dropped and
    /// retried (resuming where it stopped). A throttled CDN edge can keep trickling a few hundred
    /// bytes per second for hours; that never trips the client's read timeout, which only fires on
    /// silence. `stall_min_bytes == 0` turns the check off.
    pub stall_window: Duration,
    pub stall_min_bytes: u64,
    /// Give up on a connection that is alive but slow when finishing from it would take longer than
    /// this, provided another origin is available. Judged only after `slow_grace` of measurement.
    pub slow_eta: Duration,
    pub slow_grace: Duration,
    /// How long to wait for the response headers from an origin that has an alternative. A host can
    /// accept the connection and then take 10-20 s to answer; that wait happens before any byte
    /// arrives, so the throughput checks above never see it.
    pub header_timeout: Duration,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            source: Source::Official,
            concurrency: 16,
            segments: 4,
            min_segment: 2 * 1024 * 1024,
            retries: 3,
            stall_window: Duration::from_secs(20),
            // 1 KiB/s. Far below any working link, even one shared by dozens of connections.
            stall_min_bytes: 20 * 1024,
            slow_eta: Duration::from_secs(15),
            slow_grace: Duration::from_secs(4),
            header_timeout: Duration::from_secs(8),
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
    /// Files being transferred right now / HTTP connections open right now.
    pub active_files: u64,
    pub active_connections: u64,
    /// Configured ceilings: files in flight, and connections per large file.
    pub concurrency: u64,
    pub segments: u64,
    /// Async runtime worker threads, and logical cores on the machine.
    pub worker_threads: u64,
    pub cores: u64,
    /// CPU the whole process used over the last interval, in cores (0.5 = half of one core).
    pub cpu_cores: f64,
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
    /// Live gauges: files being transferred, connections open.
    pub active_files: AtomicU64,
    pub active_conns: AtomicU64,
    /// Configured ceilings, for display only (see [`Tracker::set_limits`]).
    max_files: AtomicU64,
    max_segments: AtomicU64,
    /// Process CPU over the last interval, in thousandths of a core.
    cpu_milli: AtomicU64,
    /// (time, bytes, speed, process cpu seconds) at the previous emit.
    last: Mutex<(Instant, u64, u64, f64)>,
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
            active_files: 0.into(),
            active_conns: 0.into(),
            max_files: 0.into(),
            max_segments: 0.into(),
            cpu_milli: 0.into(),
            last: Mutex::new((Instant::now(), 0, 0, process_cpu_secs())),
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
        *self.last.lock().unwrap() = (Instant::now(), 0, 0, process_cpu_secs());
        self.emit(true);
    }

    /// Record the configured ceilings so the UI can show "24 of 64 files".
    pub fn set_limits(&self, files: usize, segments: usize) {
        self.max_files.store(files as u64, Ordering::Relaxed);
        self.max_segments.store(segments as u64, Ordering::Relaxed);
    }

    pub fn add_bytes(&self, n: u64) {
        self.done_bytes.fetch_add(n, Ordering::Relaxed);
        self.emit(false);
    }
    pub fn sub_bytes(&self, n: u64) {
        let _ = self.done_bytes.try_update(Ordering::Relaxed, Ordering::Relaxed, |v| Some(v.saturating_sub(n)));
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
            active_files: self.active_files.load(Ordering::Relaxed),
            active_connections: self.active_conns.load(Ordering::Relaxed),
            concurrency: self.max_files.load(Ordering::Relaxed),
            segments: self.max_segments.load(Ordering::Relaxed),
            worker_threads: tokio::runtime::Handle::try_current().map(|h| h.metrics().num_workers() as u64).unwrap_or(1),
            cores: std::thread::available_parallelism().map(|n| n.get() as u64).unwrap_or(1),
            cpu_cores: self.cpu_milli.load(Ordering::Relaxed) as f64 / 1000.0,
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
        let cpu = process_cpu_secs();
        self.cpu_milli.store(((cpu - last.3).max(0.0) / secs * 1000.0) as u64, Ordering::Relaxed);
        *last = (Instant::now(), db, speed, cpu);
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
    if let Some(sz) = spec.size
        && meta.len() != sz
    {
        return false;
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
    if let Some(sz) = spec.size
        && meta.len() != sz
    {
        return Err(DlError::Size { path: path.display().to_string(), expected: sz, actual: meta.len() });
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

/// Download one file (skips if already valid).
///
/// A file often has two origins (the official CDN and a mirror). Attempts alternate between them
/// rather than spending every retry on one, a host that just failed or crawled is tried last for a
/// while (see [`health`]), and a connection that is alive but slow is dropped when a healthy other
/// origin could finish sooner. The caller's order (`cfg.source`) only breaks ties.
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
    tracker.set_limits(cfg.concurrency, cfg.segments);
    let _file = Gauge::new(&tracker.active_files);
    let urls: Vec<String> = spec.urls.iter().flat_map(|u| candidates(u, cfg.source)).fold(Vec::new(), |mut acc, u| {
        if !acc.contains(&u) {
            acc.push(u);
        }
        acc
    });
    let mut plan = Plan::new(urls, cfg.retries);
    let mut last_err = DlError::Other("no url".into());
    let mut round = 0u32;
    while let Some(i) = plan.next() {
        ctl.checkpoint().await?;
        let url = plan.url(i).to_string();
        let counted = Arc::new(AtomicU64::new(0));
        // Only worth abandoning a slow connection if somewhere better exists.
        let may_switch = plan.has_alternative(i);
        match try_once(client, cfg, spec, &url, may_switch, ctl, tracker, &counted).await {
            Ok(()) => {
                health::succeeded(&url);
                return Ok(());
            }
            Err(DlError::Cancelled) => return Err(DlError::Cancelled),
            Err(e) => {
                tracker.sub_bytes(counted.load(Ordering::Relaxed));
                tracing::warn!(attempt = plan.tries(i) + 1, of = cfg.retries.max(1), "download attempt failed for {url}: {e}");
                plan.failed(i, &e);
                last_err = e;
                // Back off only between rounds over the same origins, not when switching to a fresh one.
                if plan.next().is_some_and(|n| plan.tries(n) > 0) {
                    tokio::time::sleep(backoff(round)).await;
                    round += 1;
                }
            }
        }
    }
    Err(last_err)
}

/// CPU seconds (user + system) the whole process has used so far.
pub fn process_cpu_secs() -> f64 {
    #[cfg(unix)]
    {
        let mut ru = std::mem::MaybeUninit::<libc::rusage>::zeroed();
        // SAFETY: `ru` is a valid, writable rusage for the duration of the call.
        if unsafe { libc::getrusage(libc::RUSAGE_SELF, ru.as_mut_ptr()) } == 0 {
            // SAFETY: getrusage succeeded, so it filled the struct.
            let ru = unsafe { ru.assume_init() };
            let secs = |t: libc::timeval| t.tv_sec as f64 + t.tv_usec as f64 / 1e6;
            return secs(ru.ru_utime) + secs(ru.ru_stime);
        }
    }
    0.0
}

/// Counts one unit on a live gauge for as long as it is alive.
struct Gauge<'a>(&'a AtomicU64);

impl<'a> Gauge<'a> {
    fn new(counter: &'a AtomicU64) -> Self {
        counter.fetch_add(1, Ordering::Relaxed);
        Self(counter)
    }
}

impl Drop for Gauge<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

fn backoff(attempt: u32) -> Duration {
    Duration::from_millis(200 * (1 << attempt.min(5)))
}

/// Watches one connection's throughput (see [`EngineConfig::stall_window`]).
struct RateGuard<'a> {
    window: Duration,
    min_bytes: u64,
    url: &'a str,
    since: Instant,
    got: u64,
    /// Give up on a slow-but-alive connection when finishing would take longer than `eta_limit`.
    /// `None` when there is no other origin to switch to.
    eta_limit: Option<(Duration, Duration)>,
    started: Instant,
    total_got: u64,
    /// Bytes this transfer still has to receive, if the size is known.
    remaining: Option<u64>,
}

impl<'a> RateGuard<'a> {
    fn new(cfg: &EngineConfig, url: &'a str) -> Self {
        let now = Instant::now();
        Self {
            window: cfg.stall_window,
            min_bytes: cfg.stall_min_bytes,
            url,
            since: now,
            got: 0,
            eta_limit: None,
            started: now,
            total_got: 0,
            remaining: None,
        }
    }

    /// Allow abandoning this connection for being too slow, given how much is left to download.
    fn with_eta(mut self, cfg: &EngineConfig, may_switch: bool, remaining: Option<u64>) -> Self {
        if may_switch && remaining.is_some() {
            self.eta_limit = Some((cfg.slow_eta, cfg.slow_grace));
            self.remaining = remaining;
        }
        self
    }

    /// Start a fresh window, e.g. after the user paused the task.
    fn reset(&mut self) {
        self.since = Instant::now();
        self.got = 0;
        // Time spent paused must not count towards the speed measurement either.
        self.started = Instant::now();
        self.total_got = 0;
    }

    /// Count `n` received bytes; errors when a whole window passed with too few.
    fn feed(&mut self, n: u64) -> DlResult<()> {
        self.got += n;
        self.total_got += n;
        if let (Some((limit, grace)), Some(remaining)) = (self.eta_limit, self.remaining) {
            let el = self.started.elapsed();
            if el >= grace && self.total_got > 0 {
                let left = remaining.saturating_sub(self.total_got);
                let bps = self.total_got as f64 / el.as_secs_f64();
                let eta = left as f64 / bps;
                if eta > limit.as_secs_f64() {
                    return Err(DlError::TooSlow { url: self.url.to_string(), eta_secs: eta as u64 });
                }
            }
        }
        if self.since.elapsed() >= self.window {
            if self.got < self.min_bytes {
                return Err(DlError::Slow { url: self.url.to_string(), min_bytes: self.min_bytes, secs: self.window.as_secs() });
            }
            self.reset();
        }
        Ok(())
    }
}

/// Send a request. When another origin could take over, only wait `cfg.header_timeout` for the
/// response headers: a host that accepts the connection and then stays silent would otherwise hold the
/// download up before any byte arrives, where no throughput check can see it. With no alternative the
/// wait is left to the client's own timeouts.
async fn send(req: reqwest::RequestBuilder, cfg: &EngineConfig, url: &str, may_switch: bool) -> DlResult<reqwest::Response> {
    if !may_switch {
        return Ok(req.send().await?);
    }
    match tokio::time::timeout(cfg.header_timeout, req.send()).await {
        Ok(r) => Ok(r?),
        Err(_) => Err(DlError::NoAnswer { url: url.to_string(), secs: cfg.header_timeout.as_secs_f64().ceil() as u64 }),
    }
}

/// Wait while paused. Time spent paused must not count against the connection.
async fn checkpoint(ctl: &TaskCtl, guard: &mut RateGuard<'_>) -> DlResult<()> {
    let was_paused = ctl.state() == RunState::Paused;
    ctl.checkpoint().await?;
    if was_paused {
        guard.reset();
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn try_once(
    client: &reqwest::Client,
    cfg: &EngineConfig,
    spec: &FileSpec,
    url: &str,
    may_switch: bool,
    ctl: &TaskCtl,
    tracker: &Arc<Tracker>,
    counted: &Arc<AtomicU64>,
) -> DlResult<()> {
    let part = part_path(&spec.dest);
    let big = spec.size.map(|s| cfg.segments > 1 && s >= cfg.min_segment * 2).unwrap_or(false);
    let res = if big {
        match segmented(client, cfg, spec, url, may_switch, &part, ctl, tracker, counted).await {
            Err(DlError::Other(m)) if m == "no-range" => {
                tracker.sub_bytes(counted.swap(0, Ordering::Relaxed));
                // `segmented` preallocated the file with zeros; `single` would resume after them.
                let _ = tokio::fs::remove_file(&part).await;
                single(client, cfg, spec, url, may_switch, &part, ctl, tracker, counted).await
            }
            r => r,
        }
    } else {
        single(client, cfg, spec, url, may_switch, &part, ctl, tracker, counted).await
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
    if resp.status().is_success() { Ok(resp) } else { Err(DlError::Status(resp.status().as_u16(), url.to_string())) }
}

/// One connection; resumes from an existing `.part` via Range when possible.
#[allow(clippy::too_many_arguments)]
async fn single(
    client: &reqwest::Client,
    cfg: &EngineConfig,
    spec: &FileSpec,
    url: &str,
    may_switch: bool,
    part: &Path,
    ctl: &TaskCtl,
    tracker: &Tracker,
    counted: &Arc<AtomicU64>,
) -> DlResult<()> {
    let mut existing = tokio::fs::metadata(part).await.map(|m| m.len()).unwrap_or(0);
    if let Some(sz) = spec.size
        && existing > sz
    {
        existing = 0;
    }
    let _conn = Gauge::new(&tracker.active_conns);
    let mut req = client.get(url);
    if existing > 0 {
        req = req.header("Range", format!("bytes={existing}-"));
    }
    let resp = send(req, cfg, url, may_switch).await?;
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
    // How much is left on THIS response: the server's Content-Length is exact for it (also for a
    // resumed 206), so it works for files whose size the version JSON does not list (Fabric and Quilt
    // libraries); otherwise fall back to the size we were given.
    let remaining = resp.content_length().or_else(|| spec.size.map(|sz| sz.saturating_sub(if resumed { existing } else { 0 })));
    let mut guard = RateGuard::new(cfg, url).with_eta(cfg, may_switch, remaining);
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        checkpoint(ctl, &mut guard).await?;
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        counted.fetch_add(chunk.len() as u64, Ordering::Relaxed);
        tracker.add_bytes(chunk.len() as u64);
        guard.feed(chunk.len() as u64)?;
    }
    file.flush().await?;
    Ok(())
}

/// Several parallel Range requests writing into one preallocated file.
/// Every segment retries on its own and resumes from the last byte it wrote, so a stalled or
/// throttled connection costs a reconnect, not the whole file.
/// Not resumable across process restarts (stage-level resume skips finished files).
#[allow(clippy::too_many_arguments)]
async fn segmented(
    client: &reqwest::Client,
    cfg: &EngineConfig,
    spec: &FileSpec,
    url: &str,
    may_switch: bool,
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
    for range in ranges {
        let (client, cfg, url, part) = (client.clone(), cfg.clone(), url.to_string(), part.to_path_buf());
        let (ctl, counted, tracker) = (ctl.clone(), counted.clone(), tracker.clone());
        set.spawn(async move { fetch_segment(&client, &cfg, &url, may_switch, &part, range, &ctl, &tracker, &counted).await });
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

/// Download bytes `s..=e` into `part`, reconnecting (and resuming) up to `cfg.retries` times.
#[allow(clippy::too_many_arguments)]
async fn fetch_segment(
    client: &reqwest::Client,
    cfg: &EngineConfig,
    url: &str,
    may_switch: bool,
    part: &Path,
    (s, e): (u64, u64),
    ctl: &TaskCtl,
    tracker: &Arc<Tracker>,
    counted: &Arc<AtomicU64>,
) -> DlResult<()> {
    let mut pos = s;
    let mut last = DlError::Other("segment not attempted".into());
    for attempt in 0..cfg.retries.max(1) {
        ctl.checkpoint().await?;
        match segment_attempt(client, cfg, url, may_switch, part, (s, e), &mut pos, ctl, tracker, counted).await {
            Ok(()) => return Ok(()),
            Err(DlError::Cancelled) => return Err(DlError::Cancelled),
            // The server ignores Range: the caller falls back to one plain connection.
            Err(DlError::Other(m)) if m == "no-range" => return Err(DlError::Other(m)),
            Err(err) => {
                // A slow segment means this origin is slow for the whole file: stop all segments and let
                // the caller pick another origin instead of retrying here.
                if err.is_permanent() || matches!(err, DlError::TooSlow { .. } | DlError::NoAnswer { .. }) {
                    return Err(err);
                }
                tracing::warn!(attempt = attempt + 1, resume_at = pos, "segment {s}-{e} of {url} failed: {err}");
                last = err;
                tokio::time::sleep(backoff(attempt)).await;
            }
        }
    }
    Err(last)
}

/// One connection for the rest of a segment, starting at `*pos` (advanced as bytes are written).
#[allow(clippy::too_many_arguments)]
async fn segment_attempt(
    client: &reqwest::Client,
    cfg: &EngineConfig,
    url: &str,
    may_switch: bool,
    part: &Path,
    (s, e): (u64, u64),
    pos: &mut u64,
    ctl: &TaskCtl,
    tracker: &Arc<Tracker>,
    counted: &Arc<AtomicU64>,
) -> DlResult<()> {
    let _conn = Gauge::new(&tracker.active_conns);
    let resp = send(client.get(url).header("Range", format!("bytes={}-{e}", *pos)), cfg, url, may_switch).await?;
    let resp = check_status(resp, url).await?;
    if resp.status().as_u16() != 206 {
        return Err(DlError::Other("no-range".into()));
    }
    let mut f = tokio::fs::OpenOptions::new().write(true).open(part).await?;
    f.seek(std::io::SeekFrom::Start(*pos)).await?;
    let mut guard = RateGuard::new(cfg, url).with_eta(cfg, may_switch, Some(e + 1 - *pos));
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        checkpoint(ctl, &mut guard).await?;
        let chunk = chunk?;
        // Never write past the end of this segment, whatever the server sends.
        let take = chunk.len().min((e + 1 - *pos) as usize);
        f.write_all(&chunk[..take]).await?;
        *pos += take as u64;
        counted.fetch_add(take as u64, Ordering::Relaxed);
        tracker.add_bytes(take as u64);
        if *pos > e {
            break;
        }
        guard.feed(chunk.len() as u64)?;
    }
    f.flush().await?;
    if *pos <= e {
        return Err(DlError::Size { path: part.display().to_string(), expected: e - s + 1, actual: *pos - s });
    }
    Ok(())
}

/// Download many files with bounded concurrency. Fails fast on first error.
pub async fn download_batch(
    client: &reqwest::Client,
    cfg: &EngineConfig,
    files: Vec<FileSpec>,
    ctl: &TaskCtl,
    tracker: &Arc<Tracker>,
) -> DlResult<()> {
    tracker.set_limits(cfg.concurrency, cfg.segments);
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

#[cfg(test)]
mod rate_guard_tests {
    use super::*;

    fn cfg(window_ms: u64, min: u64) -> EngineConfig {
        EngineConfig { stall_window: Duration::from_millis(window_ms), stall_min_bytes: min, ..Default::default() }
    }

    #[test]
    fn a_slow_window_is_an_error_and_a_fast_one_is_not() {
        let c = cfg(30, 100);
        let mut g = RateGuard::new(&c, "u");
        g.feed(10).unwrap(); // window not over yet: nothing to judge
        std::thread::sleep(Duration::from_millis(40));
        assert!(matches!(g.feed(10), Err(DlError::Slow { .. })), "20 bytes in a 30 ms window with a 100 byte floor");

        let mut g = RateGuard::new(&c, "u");
        std::thread::sleep(Duration::from_millis(40));
        g.feed(500).unwrap();
        std::thread::sleep(Duration::from_millis(40));
        g.feed(500).unwrap(); // each window met the floor and starts over
    }

    #[test]
    fn reset_forgives_time_spent_paused_and_zero_disables_the_check() {
        let c = cfg(30, 100);
        let mut g = RateGuard::new(&c, "u");
        std::thread::sleep(Duration::from_millis(40));
        g.reset();
        g.feed(1).unwrap();

        let off = cfg(1, 0);
        let mut g = RateGuard::new(&off, "u");
        std::thread::sleep(Duration::from_millis(5));
        g.feed(0).unwrap();
    }
}

#[cfg(all(test, unix))]
mod cpu_meter_tests {
    use super::*;

    #[test]
    fn process_cpu_time_advances_when_the_process_works() {
        let before = process_cpu_secs();
        let t = Instant::now();
        let mut x = 0u64;
        while t.elapsed() < Duration::from_millis(80) {
            x = x.wrapping_add(std::hint::black_box(1));
        }
        std::hint::black_box(x);
        let used = process_cpu_secs() - before;
        assert!(used >= 0.04, "busy-looping for 80 ms registered only {used:.3} s of CPU");
    }
}
