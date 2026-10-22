//! `app_launch` pipeline against a local server and a fake JVM.

use super::*;
use crate::app::launch;
use crate::launch::ExitInfo;

/// Pretend a previous install left only the version json behind.
async fn half_installed(h: &Harness, id: &str) -> Served {
    let s = serve_vanilla(id).await;
    h.ctx.mgr.set_manifest_url(&s.manifest_url);
    write_version_json(&h.root, id, &s.version_json);
    s
}

fn stages(h: &Harness) -> Vec<String> {
    let mut v: Vec<String> = h.names("launch://progress").iter().map(|e| e["stage"].as_str().unwrap().to_string()).collect();
    v.dedup();
    v
}

#[tokio::test]
async fn launches_and_repairs_missing_files() {
    let mut h = Harness::new();
    let java = h.use_java(OK_JAVA);
    let _s = half_installed(&h, "1.20.1").await;
    let acc = h.offline_account("Steve");
    let inst = h.root.join("versions/1.20.1").to_string_lossy().into_owned();

    let id = launch::launch(&h.ctx, &inst, &acc, Some("test-launch-1".into())).await.expect("launch succeeds");
    assert_eq!(id, "test-launch-1");

    // files were repaired through the download engine
    assert!(h.root.join("versions/1.20.1/1.20.1.jar").is_file());
    assert!(h.root.join("libraries/com/example/lib/1.0/lib-1.0.jar").is_file());
    assert!(h.root.join("assets/indexes/8.json").is_file());

    // stage order and monotonic progress, every event tagged with the launch id
    assert_eq!(stages(&h)[..4], ["check", "java", "download", "natives"]);
    let events = h.names("launch://progress");
    assert!(events.iter().all(|e| e["launchId"] == "test-launch-1"));
    let progress: Vec<f64> = events.iter().map(|e| e["progress"].as_f64().unwrap()).collect();
    assert!(progress.windows(2).all(|w| w[1] + 1e-9 >= w[0]), "progress must not go backwards: {progress:?}");

    // the fake JVM ran and printed a "Setting user" line -> running stage + exit
    let exit = h.wait_event("launch://exit", 20).await;
    assert_eq!(exit["launchId"], "test-launch-1");
    assert_eq!(exit["code"], 0);
    assert_eq!(exit["crashed"], false);
    assert!(exit["crash"].is_null());
    assert!(stages(&h).contains(&"running".to_string()), "{:?}", stages(&h));

    // arguments: memory merged from settings, game dir isolated, library on the classpath
    let log: Vec<String> = h.names("launch://log").iter().filter(|e| e["stream"] == "stdout").map(|e| e["line"].as_str().unwrap().to_string()).collect();
    let args: Vec<&str> = log.iter().filter_map(|l| l.strip_prefix("ARG:")).collect();
    let after = |flag: &str| args.iter().position(|a| *a == flag).map(|i| args[i + 1]);
    assert!(args.contains(&"-Xmx3000m"), "memory from settings: {args:?}");
    assert!(args.iter().any(|a| a.contains("lib-1.0.jar")), "classpath has the library");
    assert_eq!(after("--username"), Some("Steve"));
    let version_dir = h.root.join("versions/1.20.1").to_string_lossy().into_owned();
    assert_eq!(after("--gameDir"), Some(version_dir.as_str()), "version isolation is honoured");
    assert_eq!(after("--version"), Some("1.20.1"));

    // header line is masked and the java used is ours
    let header = h.names("launch://log").into_iter().find(|e| e["stream"] == "launcher").unwrap();
    assert!(header["line"].as_str().unwrap().starts_with(&java.path));
    // game log written
    assert!(h.ctx.data_dir.join("logs/launch/test-launch-1.log").is_file());
}

#[tokio::test]
async fn second_launch_needs_no_download() {
    let mut h = Harness::new();
    h.use_java(OK_JAVA);
    let s = half_installed(&h, "1.20.1").await;
    let acc = h.offline_account("Steve");
    let inst = h.root.join("versions/1.20.1").to_string_lossy().into_owned();
    launch::launch(&h.ctx, &inst, &acc, Some("l1".into())).await.unwrap();
    h.wait_event("launch://exit", 20).await;
    let before = s.server.received_requests().await.unwrap().len();
    h.events.lock().unwrap().clear();
    assert!(h.ctx.mgr.list().iter().any(|t| t.kind == "repair"), "the first launch repaired the instance");
    h.ctx.mgr.forget_finished();
    launch::launch(&h.ctx, &inst, &acc, Some("l2".into())).await.unwrap();
    assert_eq!(s.server.received_requests().await.unwrap().len(), before, "nothing downloaded again");
    assert!(h.names("launch://progress").iter().all(|e| e.get("taskId").is_none()), "no download task was started");
    assert!(h.ctx.mgr.list().is_empty(), "no task for the second launch: {:?}", h.ctx.mgr.list().iter().map(|t| &t.kind).collect::<Vec<_>>());
}

#[tokio::test]
async fn java_missing_is_structured() {
    let mut h = Harness::new();
    h.ctx.discover = Arc::new(|_| vec![]);
    let s = serve_vanilla("1.20.1").await;
    write_version_json(&h.root, "1.20.1", &s.version_json);
    let acc = h.offline_account("Steve");
    let e = launch::launch(&h.ctx, "1.20.1", &acc, None).await.unwrap_err();
    assert_eq!(e.code, "JAVA_MISSING");
    assert_eq!(e.details.as_ref().unwrap()["requiredMajor"], 17);
    assert_eq!(e.details.as_ref().unwrap()["found"], json!([]));
    let wire = serde_json::to_value(&e).unwrap();
    assert_eq!(wire["code"], "JAVA_MISSING");
    assert!(wire["message"].as_str().unwrap().contains("17"));

    // too old a Java is reported with what was found
    let old = JavaInfo { major: 8, ..fake_java(h.dir.path(), "j8", "exit 0") };
    h.ctx.discover = Arc::new(move |_| vec![old.clone()]);
    let e = launch::launch(&h.ctx, "1.20.1", &acc, None).await.unwrap_err();
    assert_eq!((e.code, e.details.unwrap()["found"].clone()), ("JAVA_MISSING", json!([8])));
}

#[tokio::test]
async fn account_and_instance_errors() {
    let mut h = Harness::new();
    h.use_java(OK_JAVA);
    let s = serve_vanilla("1.20.1").await;
    write_version_json(&h.root, "1.20.1", &s.version_json);
    assert_eq!(launch::launch(&h.ctx, "1.20.1", "nope", None).await.unwrap_err().code, "ACCOUNT_NOT_FOUND");
    let acc = h.offline_account("Steve");
    assert_eq!(launch::launch(&h.ctx, "ghost", &acc, None).await.unwrap_err().code, "INSTANCE_NOT_FOUND");
    assert_eq!(launch::launch(&h.ctx, "/etc/versions/x", &acc, None).await.unwrap_err().code, "FORBIDDEN_PATH");
    assert_eq!(launch::launch(&h.ctx, "1.20.1", &acc, Some("../evil".into())).await.unwrap_err().code, "INVALID_ARGUMENT");

    // a Microsoft account without stored credentials must ask for a new login
    let id = h.add_expired_ms_account();
    let e = launch::launch(&h.ctx, "1.20.1", &id, None).await.unwrap_err();
    assert_eq!(e.code, "ACCOUNT_EXPIRED", "{e}");
}

#[tokio::test]
async fn files_missing_when_repair_cannot_help() {
    let mut h = Harness::new();
    h.use_java(OK_JAVA);
    let s = serve_vanilla("1.20.1").await;
    // point the version json at urls that 404
    let broken = s.version_json.replace(&s.server.uri(), &format!("{}/gone", s.server.uri()));
    write_version_json(&h.root, "1.20.1", &broken);
    h.ctx.mgr.set_manifest_url(&s.manifest_url);
    let acc = h.offline_account("Steve");
    let e = launch::launch(&h.ctx, "1.20.1", &acc, None).await.unwrap_err();
    assert!(matches!(e.code, "NETWORK" | "DOWNLOAD_FAILED"), "download failure surfaces as a typed error: {e}");
    assert!(h.names("launch://exit").is_empty(), "no process was started");
}

#[tokio::test]
async fn crash_exit_carries_analysis() {
    let mut h = Harness::new();
    h.use_java(r#"echo "[12:00:00] [main/ERROR]: java.lang.OutOfMemoryError: Java heap space" >&2
echo "[12:00:00] [Render thread/INFO]: Setting user: Steve"
exit 1"#);
    let _s = half_installed(&h, "1.20.1").await;
    let acc = h.offline_account("Steve");
    launch::launch(&h.ctx, "1.20.1", &acc, Some("crashy".into())).await.unwrap();
    let exit = h.wait_event("launch://exit", 20).await;
    assert_eq!(exit["code"], 1);
    assert_eq!(exit["crashed"], true);
    let ids: Vec<&str> = exit["crash"]["reasons"].as_array().unwrap().iter().map(|r| r["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&"out_of_memory"), "{exit}");
    assert!(exit["crash"]["summary"].as_str().unwrap().len() > 5);
    assert_eq!(exit["reason"], exit["crash"]["summary"]);
}

#[tokio::test]
async fn kill_is_not_a_crash() {
    let mut h = Harness::new();
    h.use_java("echo '[x] [Render thread/INFO]: Setting user: Steve'\nexec sleep 30");
    let _s = half_installed(&h, "1.20.1").await;
    let acc = h.offline_account("Steve");
    let id = launch::launch(&h.ctx, "1.20.1", &acc, Some("to-kill".into())).await.unwrap();
    h.wait_event("launch://log", 5).await;
    crate::launch::kill_by_id(&id).unwrap();
    let exit = h.wait_event("launch://exit", 20).await;
    assert_eq!(exit["crashed"], false, "{exit}");
    assert!(exit["code"].is_null(), "terminated by a signal has no exit code: {exit}");
    assert!(exit["crash"].is_null());
}

#[tokio::test]
async fn exit_zero_and_quick_nonzero_semantics() {
    let info = |code, killed| ExitInfo { code, killed, duration: std::time::Duration::from_secs(1), tail: vec![] };
    assert!(!info(Some(0), false).crashed());
    assert!(info(Some(1), false).crashed());
    assert!(info(None, false).crashed(), "died from a signal we did not send");
    assert!(!info(None, true).crashed());
    let d = tempfile::tempdir().unwrap();
    assert_eq!(launch::exit_extras(&info(Some(0), false), d.path(), d.path()), json!({}));
    let extra = launch::exit_extras(&info(Some(137), false), d.path(), d.path());
    assert!(extra["crash"]["summary"].is_string());
}

#[tokio::test]
async fn tokens_never_reach_logs_or_events() {
    let mut h = Harness::new();
    h.use_java(OK_JAVA);
    let _s = half_installed(&h, "1.20.1").await;
    let acc = h.offline_account("Steve");
    // Make the session look like a real account: register a distinctive token and make the
    // fake JVM echo it back like a misbehaving mod would.
    let secret = "eyJhbGciOiJSUzI1NiJ9.super-secret-payload-123456.signature-abcdef";
    logging::register_secret(secret);
    h.use_java(&format!("echo \"leak {secret}\"\necho \"--accessToken {secret}\"\nexit 0"));
    launch::launch(&h.ctx, "1.20.1", &acc, Some("masking".into())).await.unwrap();
    h.wait_event("launch://exit", 20).await;
    let all = serde_json::to_string(&*h.events.lock().unwrap()).unwrap();
    assert!(!all.contains("super-secret-payload"), "token leaked into events");
    assert!(all.contains("leak ***"));
    let file = std::fs::read_to_string(h.ctx.data_dir.join("logs/launch/masking.log")).unwrap();
    assert!(!file.contains("super-secret-payload"), "token leaked into the game log file");
}

#[tokio::test]
async fn authlib_injector_is_added_for_thirdparty_accounts() {
    let mut h = Harness::new();
    h.use_java(OK_JAVA);
    let _s = half_installed(&h, "1.20.1").await;

    // yggdrasil server with metadata + auth; the injector artifact is served locally too
    let ygg = MockServer::start().await;
    Mock::given(method("GET")).and(path("/")).respond_with(ResponseTemplate::new(200).set_body_string(r#"{"meta":{"serverName":"Skins"}}"#)).mount(&ygg).await;
    Mock::given(method("POST")).and(path("/authserver/authenticate")).respond_with(ResponseTemplate::new(200).set_body_string(
        r#"{"accessToken":"ygg-access-token-123456","clientToken":"client-token-xyz","selectedProfile":{"id":"0123456789abcdef0123456789abcdef","name":"Skinny"}}"#)).mount(&ygg).await;
    Mock::given(method("POST")).and(path("/authserver/validate")).respond_with(ResponseTemplate::new(204)).mount(&ygg).await;
    let acc = h.ctx.accounts().unwrap().add_yggdrasil(&ygg.uri(), "user@example.org", "pw").await.unwrap().remove(0);

    // pre-seed the jar: the "download" branch is covered by `injector_download_goes_through_engine`
    let jar = paths::authlib_injector_jar(&h.ctx.data_dir);
    std::fs::create_dir_all(jar.parent().unwrap()).unwrap();
    std::fs::write(&jar, b"PK-fake-injector").unwrap();

    launch::launch(&h.ctx, "1.20.1", &acc.id, Some("ygg".into())).await.unwrap();
    h.wait_event("launch://exit", 20).await;
    let out: Vec<String> = h.names("launch://log").iter().filter(|e| e["stream"] == "stdout").map(|e| e["line"].as_str().unwrap().to_string()).collect();
    let agent = out.iter().find(|l| l.starts_with("ARG:-javaagent:")).expect("-javaagent passed");
    assert!(agent.contains(&jar.display().to_string()) && agent.ends_with(&format!("={}", ygg.uri())), "{agent}");
    assert!(out.iter().any(|l| l.starts_with("ARG:-Dauthlibinjector.yggdrasil.prefetched=")));
    let javaagent_pos = out.iter().position(|l| l.starts_with("ARG:-javaagent:")).unwrap();
    let cp_pos = out.iter().position(|l| l == "ARG:-cp").unwrap();
    assert!(javaagent_pos < cp_pos, "agent comes before the main arguments");
    // the access token never shows in the launcher header
    let header = h.names("launch://log").into_iter().find(|e| e["stream"] == "launcher").unwrap();
    assert!(!header["line"].as_str().unwrap().contains("ygg-access-token-123456"));
}

#[tokio::test]
async fn injector_download_goes_through_engine() {
    let h = Harness::new();
    let art = MockServer::start().await;
    let body = b"fake-injector-jar".to_vec();
    Mock::given(method("GET")).and(path("/a.jar")).respond_with(ResponseTemplate::new(200).set_body_bytes(body.clone())).mount(&art).await;
    // Same call the pipeline makes, with the artifact meta redirected to the local server.
    let dest = paths::authlib_injector_jar(&h.ctx.data_dir);
    let url = format!("{}/a.jar", art.uri());
    let d2 = dest.clone();
    let sum = hex::encode(sha2_of(&body));
    tasks::run_task(&h.ctx.mgr, crate::download::SpawnOpts::default(), "authlib", "authlib-injector", move |t| async move {
        let mut spec = crate::download::engine::FileSpec::new(url, &d2);
        spec.sha256 = Some(sum);
        t.file(spec).await
    })
    .await
    .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    // a wrong checksum is rejected and leaves no file behind
    let bad = h.dir.path().join("bad.jar");
    let (url, b2) = (format!("{}/a.jar", art.uri()), bad.clone());
    let e = tasks::run_task(&h.ctx.mgr, crate::download::SpawnOpts::default(), "authlib", "x", move |t| async move {
        let mut spec = crate::download::engine::FileSpec::new(url, &b2);
        spec.sha256 = Some("00".repeat(32));
        t.file(spec).await
    })
    .await
    .unwrap_err();
    assert_eq!(e.code, "DOWNLOAD_FAILED");
    assert!(!bad.exists());
}

fn sha2_of(b: &[u8]) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    Sha256::digest(b).to_vec()
}

#[tokio::test]
async fn concurrent_launch_of_same_instance_is_rejected() {
    let mut h = Harness::new();
    h.use_java("sleep 0.2\nexit 0");
    let _s = half_installed(&h, "1.20.1").await;
    let acc = h.offline_account("Steve");
    let (a, b) = tokio::join!(
        launch::launch(&h.ctx, "1.20.1", &acc, Some("c1".into())),
        launch::launch(&h.ctx, "1.20.1", &acc, Some("c2".into()))
    );
    let codes: Vec<_> = [&a, &b].iter().map(|r| r.as_ref().err().map(|e| e.code)).collect();
    assert_eq!(codes.iter().filter(|c| **c == Some("ALREADY_LAUNCHING")).count(), 1, "{codes:?}");
    assert_eq!([&a, &b].iter().filter(|r| r.is_ok()).count(), 1);
}
