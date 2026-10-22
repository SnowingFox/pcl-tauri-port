//! Integration tests for the orchestration layer.
//!
//! These run the real download engine, install code and launch core against a local HTTP server
//! (wiremock). The only fake is the JVM: a `java` shell script that records its arguments.
//! Real-network / real-JVM verification lives in `tests/e2e.rs` (`#[ignore]`).

mod e2e;
mod install_flow;
mod ipc;
mod launch_flow;

use super::*;
use crate::accounts::secrets::MemoryStore;
use crate::download::TaskInfo;
use crate::java::JavaInfo;
use crate::resources::hash::sha1_hex;
use crate::resources::types::{Platform, ResourceFile};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub type Events = Arc<Mutex<Vec<(String, Value)>>>;

pub struct Harness {
    pub dir: tempfile::TempDir,
    pub root: PathBuf,
    pub ctx: AppCtx,
    pub events: Events,
}

impl Harness {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("mc");
        std::fs::create_dir_all(root.join("versions")).unwrap();
        let events: Events = Arc::default();
        let ev = events.clone();
        let emit: crate::launch::Emit = Arc::new(move |name, v| ev.lock().unwrap().push((name.to_string(), v)));
        let (e1, e2) = (emit.clone(), emit.clone());
        let sink: crate::download::engine::Sink = Arc::new(move |p| e1("download://progress", serde_json::to_value(p).unwrap()));
        let fin: crate::download::FinishHook = Arc::new(move |i: &TaskInfo| e2("download://finished", serde_json::to_value(i).unwrap()));
        let mgr = crate::download::Manager::new(sink, fin);
        // Per-test stores; leaked because the production code holds `&'static` handles.
        let settings: &'static SettingsStore = Box::leak(Box::new(SettingsStore::load(dir.path().join("settings.json"))));
        let accounts: &'static AccountManager =
            Box::leak(Box::new(AccountManager::new(dir.path().join("accounts"), Arc::new(MemoryStore::default())).unwrap()));
        let root_s = root.to_string_lossy().into_owned();
        settings
            .update(|g| {
                g.game.folders = vec![crate::settings::McFolderEntry { name: "test".into(), path: root_s.clone() }];
                g.game.selected_folder = Some(root_s.clone());
                g.memory.mode = crate::settings::MemoryMode::Custom;
                g.memory.custom_mb = 3000;
                g.download.version_source = crate::settings::DownloadSource::Official;
            })
            .unwrap();
        config::wire_to(settings, mgr.clone());
        let mut ctx = AppCtx::new(mgr, emit);
        ctx.settings = settings;
        ctx.accounts = Some(accounts);
        ctx.data_dir = dir.path().join("data");
        Self { dir, root, ctx, events }
    }

    pub fn names(&self, event: &str) -> Vec<Value> {
        self.events.lock().unwrap().iter().filter(|(n, _)| n == event).map(|(_, v)| v.clone()).collect()
    }

    pub async fn wait_event(&self, event: &str, secs: u64) -> Value {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(secs);
        loop {
            if let Some(v) = self.names(event).into_iter().next_back() {
                return v;
            }
            assert!(std::time::Instant::now() < deadline, "timed out waiting for {event}; got {:?}", self.events.lock().unwrap().iter().map(|e| e.0.clone()).collect::<Vec<_>>());
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }

    /// Swap in an account store that holds one Microsoft account whose refresh token is gone
    /// (what a cleared keychain or an expired login looks like). Returns its id.
    pub fn add_expired_ms_account(&mut self) -> String {
        let dir = self.dir.path().join("accounts-expired");
        let id = "ms-expired-1";
        let doc = json!({
            "version": 1, "selected": null, "settings": {"ms_client_id": "test-client"},
            "accounts": [{"id": id, "type": "microsoft", "name": "Alex", "uuid": "11111111-2222-3333-4444-555555555555"}]
        });
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("accounts.json"), doc.to_string()).unwrap();
        let mgr = AccountManager::new(dir, Arc::new(MemoryStore::default())).unwrap();
        assert_eq!(mgr.list().len(), 1, "crafted store must load");
        self.ctx.accounts = Some(Box::leak(Box::new(mgr)));
        id.into()
    }

    pub fn offline_account(&self, name: &str) -> String {
        self.ctx.accounts().unwrap().add_offline(name).unwrap().id
    }

    /// Install a fake Java that satisfies any requirement.
    pub fn use_java(&mut self, script: &str) -> JavaInfo {
        let j = fake_java(self.dir.path(), "fakejdk", script);
        let jj = j.clone();
        self.ctx.discover = Arc::new(move |_| vec![jj.clone()]);
        j
    }

    /// Wait until the task is done *and* its `download://finished` event went out (the manager flips
    /// the status first and runs the finish hook right after).
    pub async fn finish(&self, task_id: &str) -> TaskInfo {
        let info = tokio::time::timeout(std::time::Duration::from_secs(60), tasks::wait_finished(&self.ctx.mgr, task_id))
            .await
            .expect("task did not finish in time");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !self.names("download://finished").iter().any(|e| e["task_id"] == json!(task_id)) {
            assert!(std::time::Instant::now() < deadline, "no download://finished for {task_id}");
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        info
    }
}

/// Create `<dir>/<name>/bin/java` (a shell script) plus the `release` file `java::inspect` reads.
pub fn fake_java(dir: &Path, name: &str, body: &str) -> JavaInfo {
    use std::os::unix::fs::PermissionsExt;
    let home = dir.join(name);
    std::fs::create_dir_all(home.join("bin")).unwrap();
    let exe = home.join("bin/java");
    std::fs::write(&exe, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
    let arch = if cfg!(target_arch = "aarch64") { "aarch64" } else { "x86_64" };
    std::fs::write(home.join("release"), format!("JAVA_VERSION=\"21.0.1\"\nIMPLEMENTOR=\"Fake\"\nOS_ARCH=\"{arch}\"\n")).unwrap();
    crate::java::inspect(&exe).expect("fake java must be inspectable")
}

pub const OK_JAVA: &str = r#"for a in "$@"; do echo "ARG:$a"; done
echo "[12:00:00] [Render thread/INFO]: Setting user: Steve"
exit 0"#;

pub struct Served {
    pub server: MockServer,
    pub manifest_url: String,
    pub version_json: String,
}

fn sha(b: &[u8]) -> String {
    sha1_hex(b)
}

/// Serve a tiny vanilla version `id`: manifest, version json, client jar, one library, empty asset index.
pub async fn serve_vanilla(id: &str) -> Served {
    let server = MockServer::start().await;
    let base = server.uri();
    let client_jar = format!("client-jar-bytes-{id}").into_bytes();
    let lib_jar = b"library-jar-bytes".to_vec();
    let index = br#"{"objects":{}}"#.to_vec();
    let version_json = json!({
        "id": id, "type": "release", "mainClass": "net.minecraft.client.main.Main",
        "javaVersion": {"component": "java-runtime-gamma", "majorVersion": 17},
        "assetIndex": {"id": "8", "url": format!("{base}/assets/idx.json"), "sha1": sha(&index), "size": index.len()},
        "assets": "8",
        "downloads": {"client": {"url": format!("{base}/{id}/client.jar"), "sha1": sha(&client_jar), "size": client_jar.len()}},
        "arguments": {
            "game": ["--username", "${auth_player_name}", "--accessToken", "${auth_access_token}", "--uuid", "${auth_uuid}",
                     "--version", "${version_name}", "--gameDir", "${game_directory}"],
            "jvm": ["-Djava.library.path=${natives_directory}", "-cp", "${classpath}"]
        },
        "libraries": [{"name": "com.example:lib:1.0", "downloads": {"artifact": {
            "path": "com/example/lib/1.0/lib-1.0.jar", "url": format!("{base}/lib/a.jar"), "sha1": sha(&lib_jar), "size": lib_jar.len()}}}]
    })
    .to_string();
    let manifest = json!({
        "latest": {"release": id, "snapshot": id},
        "versions": [{"id": id, "type": "release", "url": format!("{base}/v/{id}.json"), "sha1": sha(version_json.as_bytes())}]
    })
    .to_string();
    for (p, body) in [
        ("/manifest.json".to_string(), manifest.into_bytes()),
        (format!("/v/{id}.json"), version_json.clone().into_bytes()),
        (format!("/{id}/client.jar"), client_jar),
        ("/lib/a.jar".to_string(), lib_jar),
        ("/assets/idx.json".to_string(), index),
    ] {
        Mock::given(method("GET")).and(path(p)).respond_with(ResponseTemplate::new(200).set_body_bytes(body)).mount(&server).await;
    }
    let manifest_url = format!("{base}/manifest.json");
    Served { server, manifest_url, version_json }
}

/// Write just the version json, as a user would have after an interrupted install.
pub fn write_version_json(root: &Path, id: &str, json: &str) {
    let d = root.join("versions").join(id);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join(format!("{id}.json")), json).unwrap();
}

pub fn resource_file(project: &str, id: &str, name: &str, url: Option<String>, body: &[u8]) -> ResourceFile {
    ResourceFile {
        platform: Platform::Modrinth, id: id.into(), project_id: project.into(), name: name.into(), file_name: name.into(), version_number: None,
        release_type: "release".into(), game_versions: vec!["1.20.1".into()], loaders: vec![crate::resources::types::ModLoader::Fabric],
        sha1: Some(sha(body)), size: body.len() as u64, download_urls: url.into_iter().collect(), date_published: "2024-01-01T00:00:00Z".into(),
        dependencies: vec![], fingerprint: None,
    }
}

/// In-memory stand-in for the Modrinth / CurseForge APIs.
#[derive(Default)]
pub struct FakeApi {
    pub versions: Mutex<std::collections::HashMap<String, Vec<ResourceFile>>>,
    pub by_id: Mutex<std::collections::HashMap<String, ResourceFile>>,
    pub cf_files: Mutex<Vec<ResourceFile>>,
    pub cf_urls: Mutex<std::collections::HashMap<String, String>>,
}

#[async_trait::async_trait]
impl resource::ResourceApi for FakeApi {
    async fn versions(&self, _: Platform, p: &str, _: Option<&str>, _: Option<crate::resources::types::ModLoader>) -> Result<Vec<ResourceFile>, String> {
        Ok(self.versions.lock().unwrap().get(p).cloned().unwrap_or_default())
    }
    async fn version(&self, _: Platform, id: &str) -> Result<ResourceFile, String> {
        self.by_id.lock().unwrap().get(id).cloned().ok_or_else(|| format!("no version {id}"))
    }
    async fn download_url(&self, f: &ResourceFile) -> Result<String, String> {
        self.cf_urls.lock().unwrap().get(&f.id).cloned().ok_or_else(|| "download disabled by the author".to_string())
    }
    async fn curseforge_files(&self, ids: &[u64]) -> Result<Vec<ResourceFile>, String> {
        Ok(self.cf_files.lock().unwrap().iter().filter(|f| f.id.parse::<u64>().is_ok_and(|i| ids.contains(&i))).cloned().collect())
    }
}

#[test]
fn harness_selfcheck() {
    let h = Harness::new();
    assert!(h.root.join("versions").is_dir());
    assert_eq!(h.ctx.mgr.cfg().game_dir, h.root, "settings were wired into the download manager");
    let j = fake_java(h.dir.path(), "j", "exit 0");
    assert_eq!(j.major, 21);
}
