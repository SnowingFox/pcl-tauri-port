//! Engine tests against a local mock HTTP server (supports Range, failures, slow bodies).

use super::engine::*;
use super::hash::{HashKind, hex_of};
use super::mirror::Source;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

pub struct Mock {
    pub base: String,
    pub hits: Arc<AtomicUsize>,
}

/// Routes: /data (range capable), /norange (ignores Range), /flaky (500 first 2 times), /404.
pub async fn mock(body: Vec<u8>) -> Mock {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", l.local_addr().unwrap());
    let hits = Arc::new(AtomicUsize::new(0));
    let (h2, body) = (hits.clone(), Arc::new(body));
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = l.accept().await else { return };
            let (hits, body) = (h2.clone(), body.clone());
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
                let range = req.lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("range: bytes=").map(|r| r.to_string()));
                let hit = hits.fetch_add(1, Ordering::SeqCst);
                let resp: Vec<u8> = if path == "/404" {
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()
                } else if path == "/flaky" && hit < 2 {
                    b"HTTP/1.1 500 Err\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()
                } else if let (Some(r), true) = (range, path != "/norange") {
                    let (a, b) = r.trim().split_once('-').unwrap();
                    let a: usize = a.parse().unwrap();
                    let b: usize = if b.is_empty() { body.len() - 1 } else { b.parse().unwrap() };
                    if a >= body.len() {
                        b"HTTP/1.1 416 Bad\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()
                    } else {
                        let part = &body[a..=b.min(body.len() - 1)];
                        let mut v = format!(
                            "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nConnection: close\r\n\r\n",
                            part.len(), a, a + part.len() - 1, body.len()
                        )
                        .into_bytes();
                        v.extend_from_slice(part);
                        v
                    }
                } else {
                    let mut v = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).into_bytes();
                    v.extend_from_slice(&body);
                    v
                };
                let _ = s.write_all(&resp).await;
                let _ = s.shutdown().await;
            });
        }
    });
    Mock { base, hits }
}

fn payload(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i * 31 % 251) as u8).collect()
}

type Events = Arc<std::sync::Mutex<Vec<ProgressEvent>>>;

fn setup() -> (reqwest::Client, EngineConfig, TaskCtl, Arc<Tracker>, Events) {
    let events = Arc::new(std::sync::Mutex::new(vec![]));
    let e2 = events.clone();
    let ctl = TaskCtl::new();
    let tr = Tracker::new("t", Arc::new(move |p| e2.lock().unwrap().push(p.clone())), ctl.clone());
    let cfg = EngineConfig { source: Source::Official, retries: 3, min_segment: 1000, segments: 4, ..Default::default() };
    (reqwest::Client::new(), cfg, ctl, tr, events)
}

#[tokio::test]
async fn single_download_verifies_hash() {
    let data = payload(5000);
    let m = mock(data.clone()).await;
    let (c, cfg, ctl, tr, _) = setup();
    let d = tempfile::tempdir().unwrap();
    let dest = d.path().join("a/b.bin");
    let spec = FileSpec::new(format!("{}/norange", m.base), &dest).with_sha1(Some(hex_of(HashKind::Sha1, &data)));
    download_file(&c, &cfg, &spec, &ctl, &tr).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), data);
    assert!(!dest.with_extension("bin.part").exists());
}

#[tokio::test]
async fn segmented_download_matches() {
    let data = payload(100_000);
    let m = mock(data.clone()).await;
    let (c, cfg, ctl, tr, ev) = setup();
    let d = tempfile::tempdir().unwrap();
    let dest = d.path().join("big.bin");
    tr.begin_stage("s", 0.0, 1.0, 1, data.len() as u64);
    let spec =
        FileSpec::new(format!("{}/data", m.base), &dest).with_size(Some(data.len() as u64)).with_sha1(Some(hex_of(HashKind::Sha1, &data)));
    download_file(&c, &cfg, &spec, &ctl, &tr).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), data);
    assert!(m.hits.load(Ordering::SeqCst) >= 4, "expected multiple range requests");
    assert_eq!(tr.done_bytes.load(Ordering::SeqCst), data.len() as u64);
    assert!(!ev.lock().unwrap().is_empty());
}

#[tokio::test]
async fn falls_back_when_server_ignores_range() {
    let data = payload(50_000);
    let m = mock(data.clone()).await;
    let (c, cfg, ctl, tr, _) = setup();
    let d = tempfile::tempdir().unwrap();
    let dest = d.path().join("x.bin");
    let spec = FileSpec::new(format!("{}/norange", m.base), &dest).with_size(Some(data.len() as u64));
    download_file(&c, &cfg, &spec, &ctl, &tr).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), data);
}

#[tokio::test]
async fn retries_transient_errors() {
    let data = payload(100);
    let m = mock(data.clone()).await;
    let (c, cfg, ctl, tr, _) = setup();
    let d = tempfile::tempdir().unwrap();
    let dest = d.path().join("f");
    download_file(&c, &cfg, &FileSpec::new(format!("{}/flaky", m.base), &dest), &ctl, &tr).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), data);
}

#[tokio::test]
async fn hash_mismatch_fails_and_leaves_no_file() {
    let m = mock(payload(100)).await;
    let (c, mut cfg, ctl, tr, _) = setup();
    cfg.retries = 1;
    let d = tempfile::tempdir().unwrap();
    let dest = d.path().join("f");
    let spec = FileSpec::new(format!("{}/data", m.base), &dest).with_sha1(Some("0".repeat(40)));
    assert!(matches!(download_file(&c, &cfg, &spec, &ctl, &tr).await, Err(super::error::DlError::Hash { .. })));
    assert!(!dest.exists());
}

#[tokio::test]
async fn http_404_not_retried() {
    let m = mock(vec![1]).await;
    let (c, cfg, ctl, tr, _) = setup();
    let d = tempfile::tempdir().unwrap();
    let r = download_file(&c, &cfg, &FileSpec::new(format!("{}/404", m.base), d.path().join("f")), &ctl, &tr).await;
    assert!(matches!(r, Err(super::error::DlError::Status(404, _))));
    assert_eq!(m.hits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn resumes_from_part_file() {
    let data = payload(10_000);
    let m = mock(data.clone()).await;
    let (c, cfg, ctl, tr, _) = setup();
    let d = tempfile::tempdir().unwrap();
    let dest = d.path().join("r.bin");
    std::fs::write(d.path().join("r.bin.part"), &data[..4000]).unwrap();
    let spec = FileSpec::new(format!("{}/data", m.base), &dest).with_sha1(Some(hex_of(HashKind::Sha1, &data)));
    download_file(&c, &cfg, &spec, &ctl, &tr).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), data);
}

#[tokio::test]
async fn skips_valid_existing_file() {
    let data = payload(100);
    let m = mock(data.clone()).await;
    let (c, cfg, ctl, tr, _) = setup();
    let d = tempfile::tempdir().unwrap();
    let dest = d.path().join("f");
    std::fs::write(&dest, &data).unwrap();
    let spec = FileSpec::new(format!("{}/data", m.base), &dest).with_sha1(Some(hex_of(HashKind::Sha1, &data)));
    download_file(&c, &cfg, &spec, &ctl, &tr).await.unwrap();
    assert_eq!(m.hits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn cancel_stops_batch() {
    let m = mock(payload(100)).await;
    let (c, cfg, ctl, tr, _) = setup();
    ctl.cancel();
    let d = tempfile::tempdir().unwrap();
    let files = (0..5).map(|i| FileSpec::new(format!("{}/data", m.base), d.path().join(format!("{i}")))).collect();
    assert!(matches!(download_batch(&c, &cfg, files, &ctl, &tr).await, Err(super::error::DlError::Cancelled)));
}

#[tokio::test]
async fn batch_downloads_all_and_pause_resume_works() {
    let data = payload(2000);
    let m = mock(data.clone()).await;
    let (c, cfg, ctl, tr, _) = setup();
    let d = tempfile::tempdir().unwrap();
    ctl.pause();
    let files: Vec<_> = (0..10).map(|i| FileSpec::new(format!("{}/data", m.base), d.path().join(format!("{i}")))).collect();
    tr.begin_stage("b", 0.0, 1.0, 10, 0);
    let ctl2 = ctl.clone();
    let h = tokio::spawn({
        let (c, cfg, tr) = (c.clone(), cfg.clone(), tr.clone());
        async move { download_batch(&c, &cfg, files, &ctl2, &tr).await }
    });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert_eq!(m.hits.load(Ordering::SeqCst), 0, "paused: nothing fetched");
    ctl.resume();
    h.await.unwrap().unwrap();
    assert_eq!(tr.done_files.load(Ordering::SeqCst), 10);
}

/// Serves `body`; any ranged request that does not start at 0 sends headers and a first
/// slice, then goes silent with the connection left open (what a stalled CDN edge does).
async fn stalling_mock(body: Vec<u8>) -> String {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", l.local_addr().unwrap());
    let body = Arc::new(body);
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = l.accept().await else { return };
            let body = body.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let range = req.lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("range: bytes=").map(|r| r.to_string()));
                let Some(r) = range else { return };
                let (a, b) = r.trim().split_once('-').unwrap();
                let (a, b): (usize, usize) = (a.parse().unwrap(), b.parse().unwrap());
                let part = &body[a..=b];
                let head = format!(
                    "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\n\r\n",
                    part.len(),
                    a,
                    b,
                    body.len()
                );
                let _ = s.write_all(head.as_bytes()).await;
                if a == 0 {
                    let _ = s.write_all(part).await;
                    let _ = s.shutdown().await;
                } else {
                    let _ = s.write_all(&part[..1000.min(part.len())]).await;
                    tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
                }
            });
        }
    });
    base
}

/// A segment whose body stalls must fail the attempt (and the download) in bounded time instead of
/// hanging the install forever. The client's read timeout is what bounds it.
#[tokio::test]
async fn stalled_segment_body_fails_in_bounded_time() {
    let data = payload(100_000);
    let base = stalling_mock(data.clone()).await;
    let (_, mut cfg, ctl, tr, _) = setup();
    cfg.retries = 2;
    let client = reqwest::Client::builder().read_timeout(std::time::Duration::from_secs(1)).build().unwrap();
    let d = tempfile::tempdir().unwrap();
    let dest = d.path().join("stall.bin");
    let spec =
        FileSpec::new(format!("{base}/data"), &dest).with_size(Some(data.len() as u64)).with_sha1(Some(hex_of(HashKind::Sha1, &data)));
    let r = tokio::time::timeout(std::time::Duration::from_secs(30), download_file(&client, &cfg, &spec, &ctl, &tr)).await;
    match r {
        Err(_) => panic!("download_file still running after 30s: a stalled body is not bounded by the read timeout"),
        Ok(Ok(())) => panic!("a stalled segment must not produce a successful download"),
        Ok(Err(e)) => eprintln!("failed as expected: {e}"),
    }
    assert!(!dest.exists(), "no corrupt file may be left at the destination");
}

/// Per-connection behaviour of [`faulty_mock`].
#[derive(Clone, Copy)]
enum Fault {
    /// Headers, the first `n` bytes, then silence.
    HangAfter(usize),
    /// Headers, then `n` bytes every 200 ms: alive, but far too slow.
    Trickle(usize),
    /// Accept the request and never answer: no status line, no headers, connection left open.
    NoResponse,
}

/// Start offsets of every request a [`faulty_mock`] received, in arrival order.
type Starts = Arc<std::sync::Mutex<Vec<usize>>>;

/// Range server that applies `fault_for(connection_number)` to each connection (None = healthy).
async fn faulty_mock(
    body: Vec<u8>,
    fault_for: impl Fn(usize) -> Option<Fault> + Send + Sync + 'static,
) -> (String, Arc<AtomicUsize>, Starts) {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", l.local_addr().unwrap());
    let (body, hits, fault_for) = (Arc::new(body), Arc::new(AtomicUsize::new(0)), Arc::new(fault_for));
    let starts: Starts = Arc::default();
    let (h2, st2) = (hits.clone(), starts.clone());
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = l.accept().await else { return };
            let (body, nth, fault_for, starts) = (body.clone(), h2.fetch_add(1, Ordering::SeqCst), fault_for.clone(), st2.clone());
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let range = req.lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("range: bytes=").map(|r| r.to_string()));
                let (a, b, status) = match range {
                    Some(r) => {
                        let (a, b) = r.trim().split_once('-').unwrap();
                        let a: usize = a.parse().unwrap();
                        let b: usize = if b.is_empty() { body.len() - 1 } else { b.parse().unwrap() };
                        (a, b, "206 Partial Content")
                    }
                    // A plain GET (the single-connection path) gets the whole body.
                    None => (0, body.len() - 1, "200 OK"),
                };
                starts.lock().unwrap().push(a);
                if matches!(fault_for(nth), Some(Fault::NoResponse)) {
                    tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
                    return;
                }
                let part = &body[a..=b];
                let head = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Range: bytes {a}-{b}/{}\r\nConnection: close\r\n\r\n",
                    part.len(),
                    body.len()
                );
                let _ = s.write_all(head.as_bytes()).await;
                match fault_for(nth) {
                    None => {
                        let _ = s.write_all(part).await;
                        let _ = s.shutdown().await;
                    }
                    Some(Fault::HangAfter(k)) => {
                        let _ = s.write_all(&part[..k.min(part.len())]).await;
                        tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
                    }
                    Some(Fault::NoResponse) => unreachable!("handled before the head is written"),
                    Some(Fault::Trickle(k)) => {
                        for c in part.chunks(k) {
                            if s.write_all(c).await.is_err() {
                                return;
                            }
                            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                        }
                    }
                }
            });
        }
    });
    (base, hits, starts)
}

fn fast_stall_cfg() -> EngineConfig {
    let (_, mut cfg, ..) = setup();
    cfg.stall_window = std::time::Duration::from_millis(600);
    cfg.stall_min_bytes = 5_000;
    cfg
}

/// The failure seen in a real install: one connection of a segmented download dies mid-body.
/// The segment must reconnect and continue from where it stopped, and the file must be intact.
#[tokio::test]
async fn a_dead_segment_resumes_from_where_it_stopped() {
    let data = payload(100_000);
    // Connection #1 is the first segment's; it hangs after 1000 bytes. Every later one is healthy.
    let (base, hits, starts) = faulty_mock(data.clone(), |n| (n == 1).then_some(Fault::HangAfter(1000))).await;
    let (_, _, ctl, tr, _) = setup();
    let cfg = fast_stall_cfg();
    let client = reqwest::Client::builder().read_timeout(std::time::Duration::from_millis(700)).build().unwrap();
    let d = tempfile::tempdir().unwrap();
    let dest = d.path().join("ok.bin");
    tr.begin_stage("s", 0.0, 1.0, 1, data.len() as u64);
    let spec =
        FileSpec::new(format!("{base}/data"), &dest).with_size(Some(data.len() as u64)).with_sha1(Some(hex_of(HashKind::Sha1, &data)));
    tokio::time::timeout(std::time::Duration::from_secs(30), download_file(&client, &cfg, &spec, &ctl, &tr))
        .await
        .expect("must not hang")
        .expect("one dead connection must not fail the download");
    assert_eq!(std::fs::read(&dest).unwrap(), data);
    assert!(hits.load(Ordering::SeqCst) >= 5, "4 segments + at least one reconnect, got {}", hits.load(Ordering::SeqCst));
    // 100_000 bytes in 4 segments start at multiples of 25_000. A reconnect that resumes asks for
    // the byte after the 1000 it already has; one that starts over only ever asks for multiples.
    let starts = starts.lock().unwrap().clone();
    assert!(starts.iter().any(|s| s % 25_000 == 1000), "no request resumed at an offset inside a segment: {starts:?}");
    assert_eq!(tr.done_bytes.load(Ordering::SeqCst), data.len() as u64, "progress must not double count the resumed bytes");
}

/// A connection that stays alive but delivers almost nothing never trips a read timeout. The
/// throughput floor must drop it so a retry can get a better connection.
#[tokio::test]
async fn a_trickling_connection_is_replaced() {
    let data = payload(100_000);
    // 1 KiB every 200 ms is ~5 KiB/s: under the 5000 byte / 600 ms floor.
    let (base, hits, _) = faulty_mock(data.clone(), |n| (n == 2).then_some(Fault::Trickle(1024))).await;
    let (_, _, ctl, tr, _) = setup();
    let cfg = fast_stall_cfg();
    let client = reqwest::Client::builder().read_timeout(std::time::Duration::from_secs(30)).build().unwrap();
    let d = tempfile::tempdir().unwrap();
    let dest = d.path().join("slow.bin");
    let spec =
        FileSpec::new(format!("{base}/data"), &dest).with_size(Some(data.len() as u64)).with_sha1(Some(hex_of(HashKind::Sha1, &data)));
    let t = std::time::Instant::now();
    tokio::time::timeout(std::time::Duration::from_secs(30), download_file(&client, &cfg, &spec, &ctl, &tr))
        .await
        .expect("must not hang")
        .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), data);
    assert!(hits.load(Ordering::SeqCst) >= 5, "the slow connection was replaced by another request");
    assert!(t.elapsed() < std::time::Duration::from_secs(20), "took {:?}: the slow connection was waited out", t.elapsed());
}

/// Pausing for longer than the stall window is not a stall.
#[tokio::test]
async fn pausing_longer_than_the_stall_window_is_not_a_stall() {
    let data = payload(60_000);
    let (base, ..) = faulty_mock(data.clone(), |_| None).await;
    let (client, _, ctl, tr, _) = setup();
    let mut cfg = fast_stall_cfg();
    cfg.segments = 1; // single-stream path
    let d = tempfile::tempdir().unwrap();
    let dest = d.path().join("paused.bin");
    let spec = FileSpec::new(format!("{base}/data"), &dest).with_sha1(Some(hex_of(HashKind::Sha1, &data)));
    ctl.pause();
    let c2 = ctl.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await; // > 2x the 600 ms window
        c2.resume();
    });
    tokio::time::timeout(std::time::Duration::from_secs(20), download_file(&client, &cfg, &spec, &ctl, &tr))
        .await
        .expect("must not hang")
        .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), data);
}

/// While a segmented download runs, the live gauges show 1 file on 4 connections, plus the
/// configured ceilings and the machine's parallelism; afterwards everything is back to zero.
#[tokio::test]
async fn live_load_gauges_count_files_and_connections_and_return_to_zero() {
    let data = payload(100_000);
    // ~0.6 s per segment, long enough to sample while all four connections are open.
    let (base, ..) = faulty_mock(data.clone(), |_| Some(Fault::Trickle(10_000))).await;
    let (client, mut cfg, ctl, tr, _) = setup();
    cfg.segments = 4;
    cfg.concurrency = 7;
    let d = tempfile::tempdir().unwrap();
    let dest = d.path().join("g.bin");
    let spec =
        FileSpec::new(format!("{base}/data"), &dest).with_size(Some(data.len() as u64)).with_sha1(Some(hex_of(HashKind::Sha1, &data)));
    let job = {
        let (client, cfg, ctl, tr, spec) = (client.clone(), cfg.clone(), ctl.clone(), tr.clone(), spec.clone());
        tokio::spawn(async move { download_file(&client, &cfg, &spec, &ctl, &tr).await })
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    let mut seen = None;
    while std::time::Instant::now() < deadline {
        let s = tr.snapshot(0);
        if s.active_connections == 4 {
            seen = Some(s);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(15)).await;
    }
    let s = seen.expect("4 connections were never open at the same time");
    assert_eq!(s.active_files, 1);
    assert_eq!((s.concurrency, s.segments), (7, 4), "configured ceilings are reported");
    assert!(s.cores >= 1 && s.worker_threads >= 1);
    job.await.unwrap().unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), data);
    let s = tr.snapshot(0);
    assert_eq!((s.active_files, s.active_connections), (0, 0), "gauges must return to zero, also after a finished download");
}

// ---- choosing between an official host and a mirror -------------------------------------------

/// Two mock origins for the same path, standing in for the official CDN and a mirror. Each gets its
/// own fault; URLs are built so the mirror rule of `mirror.rs` maps the "official" one onto the
/// "mirror" one (by giving `download_file` both URLs explicitly).
struct TwoOrigins {
    official: String,
    mirror: String,
    off_hits: Arc<AtomicUsize>,
    mir_hits: Arc<AtomicUsize>,
}

async fn two_origins(
    body: Vec<u8>,
    official: impl Fn(usize) -> Option<Fault> + Send + Sync + 'static,
    mirror: impl Fn(usize) -> Option<Fault> + Send + Sync + 'static,
) -> TwoOrigins {
    let (o, off_hits, _) = faulty_mock(body.clone(), official).await;
    let (m, mir_hits, _) = faulty_mock(body, mirror).await;
    TwoOrigins { official: format!("{o}/data"), mirror: format!("{m}/data"), off_hits, mir_hits }
}

fn spec_with(urls: [&str; 2], dest: &std::path::Path, data: &[u8]) -> FileSpec {
    FileSpec {
        urls: vec![urls[0].to_string(), urls[1].to_string()],
        dest: dest.to_path_buf(),
        size: Some(data.len() as u64),
        sha1: Some(hex_of(HashKind::Sha1, data)),
        sha256: None,
    }
}

/// One spec holds ONE url that the engine expands into candidates. For the tests the "candidates"
/// are given explicitly via `FileSpec::urls`, which the engine treats as alternatives in order.
fn explicit_cfg() -> EngineConfig {
    let mut cfg = fast_stall_cfg();
    cfg.source = Source::Official; // no automatic mirror rewriting: the test supplies both origins
    cfg.segments = 1; // single-stream path keeps the byte accounting simple
    cfg.min_segment = 10_000_000;
    cfg
}

/// The reported case: the official host stays connected but crawls (~5 KB/s). That is above the
/// "dead connection" floor, so it was waited out. The engine must give up on it and finish from the
/// healthy mirror, in a time that does not depend on the size of the slow host's backlog.
#[tokio::test]
async fn a_crawling_origin_hands_over_to_a_healthy_mirror() {
    let data = payload(200_000);
    // 1 KiB / 200 ms = ~5 KiB/s: 200 KB would take 40 s from this host.
    let o = two_origins(data.clone(), |_| Some(Fault::Trickle(1024)), |_| None).await;
    let (client, _, ctl, tr, _) = setup();
    let mut cfg = explicit_cfg();
    cfg.stall_min_bytes = 1; // not a "dead" connection: the floor alone would never fire
    cfg.slow_eta = std::time::Duration::from_secs(3);
    cfg.slow_grace = std::time::Duration::from_millis(600);
    let d = tempfile::tempdir().unwrap();
    let dest = d.path().join("f.bin");
    let t = std::time::Instant::now();
    tokio::time::timeout(
        std::time::Duration::from_secs(30),
        download_file(&client, &cfg, &spec_with([&o.official, &o.mirror], &dest, &data), &ctl, &tr),
    )
    .await
    .expect("must not wait out the crawling host")
    .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), data);
    assert!(t.elapsed() < std::time::Duration::from_secs(15), "took {:?}", t.elapsed());
    assert!(o.mir_hits.load(Ordering::SeqCst) >= 1, "the mirror was never used");
}

/// Once the official host has been caught crawling, the NEXT file must not walk into it again.
#[tokio::test]
async fn later_files_skip_a_host_that_just_failed() {
    let data = payload(20_000);
    let o = two_origins(data.clone(), |_| Some(Fault::Trickle(1024)), |_| None).await;
    let (client, _, ctl, tr, _) = setup();
    let mut cfg = explicit_cfg();
    cfg.stall_min_bytes = 1;
    cfg.slow_eta = std::time::Duration::from_secs(2);
    cfg.slow_grace = std::time::Duration::from_millis(500);
    let d = tempfile::tempdir().unwrap();
    let go = |name: &str| {
        let (client, cfg, ctl, tr, spec) =
            (client.clone(), cfg.clone(), ctl.clone(), tr.clone(), spec_with([&o.official, &o.mirror], &d.path().join(name), &data));
        async move { download_file(&client, &cfg, &spec, &ctl, &tr).await }
    };
    super::health::forget(&o.official);
    super::health::forget(&o.mirror);
    go("first.bin").await.unwrap();
    let off_before = o.off_hits.load(Ordering::SeqCst);
    go("second.bin").await.unwrap();
    go("third.bin").await.unwrap();
    assert_eq!(o.off_hits.load(Ordering::SeqCst), off_before, "later files went back to the host that was just caught crawling");
    super::health::forget(&o.official);
}

/// A failing host must not use up every retry: the second attempt goes to the other origin.
#[tokio::test]
async fn retries_alternate_between_origins() {
    let data = payload(10_000);
    let (bad, bad_hits, _) = faulty_mock(data.clone(), |_| Some(Fault::HangAfter(0))).await;
    let good = two_origins(data.clone(), |_| None, |_| None).await;
    let (client, _, ctl, tr, _) = setup();
    let client = reqwest::Client::builder().read_timeout(std::time::Duration::from_millis(400)).build().unwrap_or(client);
    let mut cfg = explicit_cfg();
    cfg.retries = 3;
    let d = tempfile::tempdir().unwrap();
    let dest = d.path().join("f.bin");
    let bad_url = format!("{bad}/data");
    super::health::forget(&bad_url);
    let t = std::time::Instant::now();
    download_file(&client, &cfg, &spec_with([&bad_url, &good.mirror], &dest, &data), &ctl, &tr).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), data);
    assert_eq!(bad_hits.load(Ordering::SeqCst), 1, "only one attempt may be spent on the failing origin before switching");
    assert!(t.elapsed() < std::time::Duration::from_secs(5));
    super::health::forget(&bad_url);
}

/// A slow origin is NOT abandoned when there is nowhere better to go: it still finishes.
#[tokio::test]
async fn a_slow_origin_is_kept_when_there_is_no_alternative() {
    let data = payload(30_000);
    let o = two_origins(data.clone(), |_| Some(Fault::Trickle(3000)), |_| Some(Fault::Trickle(3000))).await;
    let (client, _, ctl, tr, _) = setup();
    let mut cfg = explicit_cfg();
    cfg.stall_min_bytes = 1;
    cfg.slow_eta = std::time::Duration::from_millis(500); // far below the real ETA (~2 s)
    cfg.slow_grace = std::time::Duration::from_millis(300);
    let d = tempfile::tempdir().unwrap();
    let dest = d.path().join("f.bin");
    super::health::forget(&o.official);
    super::health::forget(&o.mirror);
    // both origins are equally slow: switching between them would just thrash
    tokio::time::timeout(
        std::time::Duration::from_secs(40),
        download_file(&client, &cfg, &spec_with([&o.official, &o.mirror], &dest, &data), &ctl, &tr),
    )
    .await
    .expect("must finish")
    .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), data);
    super::health::forget(&o.official);
    super::health::forget(&o.mirror);
}

/// Fabric/Quilt library entries carry no size or hash. The slow-origin check must still work, using
/// the Content-Length the server sends with the response.
#[tokio::test]
async fn a_crawling_origin_is_abandoned_even_when_the_file_size_is_unknown() {
    let data = payload(200_000);
    let o = two_origins(data.clone(), |_| Some(Fault::Trickle(1024)), |_| None).await;
    let (client, _, ctl, tr, _) = setup();
    let mut cfg = explicit_cfg();
    cfg.stall_min_bytes = 1;
    cfg.slow_eta = std::time::Duration::from_secs(3);
    cfg.slow_grace = std::time::Duration::from_millis(600);
    let d = tempfile::tempdir().unwrap();
    let dest = d.path().join("nosize.bin");
    super::health::forget(&o.official);
    super::health::forget(&o.mirror);
    let spec = FileSpec { urls: vec![o.official.clone(), o.mirror.clone()], dest: dest.clone(), size: None, sha1: None, sha256: None };
    let t = std::time::Instant::now();
    tokio::time::timeout(std::time::Duration::from_secs(30), download_file(&client, &cfg, &spec, &ctl, &tr))
        .await
        .expect("a size-less file must not be waited out either")
        .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), data);
    assert!(t.elapsed() < std::time::Duration::from_secs(15), "took {:?}", t.elapsed());
    assert!(o.mir_hits.load(Ordering::SeqCst) >= 1, "the mirror was never used");
    super::health::forget(&o.official);
}

/// An origin that accepts the connection and then does not answer for a long time holds the download
/// up before a single byte arrives, so no throughput check can see it. With a healthy alternative the
/// engine must stop waiting after `header_timeout`, switch, and remember the host as unhealthy.
#[tokio::test]
async fn an_origin_that_never_answers_hands_over_to_the_mirror() {
    let data = payload(50_000);
    let o = two_origins(data.clone(), |_| Some(Fault::NoResponse), |_| None).await;
    let (client, _, ctl, tr, _) = setup();
    // The shared client has no read timeout here, like a slow-but-not-dead server.
    let client = reqwest::Client::builder().build().unwrap_or(client);
    let mut cfg = explicit_cfg();
    cfg.header_timeout = std::time::Duration::from_millis(500);
    let d = tempfile::tempdir().unwrap();
    let dest = d.path().join("f.bin");
    super::health::forget(&o.official);
    super::health::forget(&o.mirror);
    let t = std::time::Instant::now();
    tokio::time::timeout(
        std::time::Duration::from_secs(20),
        download_file(&client, &cfg, &spec_with([&o.official, &o.mirror], &dest, &data), &ctl, &tr),
    )
    .await
    .expect("waited on an origin that never answers")
    .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), data);
    assert!(t.elapsed() < std::time::Duration::from_secs(8), "took {:?}", t.elapsed());
    assert!(super::health::is_penalized(&o.official), "the silent host must be remembered as unhealthy");
    assert!(!super::health::is_penalized(&o.mirror));
    super::health::forget(&o.official);
}

/// With no alternative the header wait is NOT cut short: a slow answer from the only origin is better
/// than no answer.
#[tokio::test]
async fn the_only_origin_is_given_time_to_answer() {
    let data = payload(10_000);
    let (base, ..) = faulty_mock(data.clone(), |_| None).await;
    let (client, _, ctl, tr, _) = setup();
    let mut cfg = explicit_cfg();
    cfg.header_timeout = std::time::Duration::from_nanos(1); // would fail every request if applied
    let d = tempfile::tempdir().unwrap();
    let dest = d.path().join("only.bin");
    let url = format!("{base}/data");
    let spec = FileSpec {
        urls: vec![url.clone()],
        dest: dest.clone(),
        size: Some(data.len() as u64),
        sha1: Some(hex_of(HashKind::Sha1, &data)),
        sha256: None,
    };
    download_file(&client, &cfg, &spec, &ctl, &tr).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), data);
    super::health::forget(&url);
}
