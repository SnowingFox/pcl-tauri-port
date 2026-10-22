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
