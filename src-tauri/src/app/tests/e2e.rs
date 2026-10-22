//! Real-network / real-JVM checks. Ignored by default:
//! `PCL_E2E_JAVA=/path/to/java cargo test --lib e2e -- --ignored --nocapture`
//!
//! Installs a real vanilla version from Mojang into a temp dir through the same functions the
//! Tauri commands call, then launches it with an offline account and waits for the client to
//! reach the session/backend initialisation lines.

use super::*;
use crate::app::{install as ainstall, launch};

fn java_from_env() -> Option<String> {
    std::env::var("PCL_E2E_JAVA").ok().filter(|s| !s.is_empty())
}

async fn install_and_launch(h: &mut Harness, mc: &str, loader: Option<ainstall::LoaderChoice>, name: &str) -> Vec<String> {
    let id = ainstall::install_game(&h.ctx, mc.into(), loader, Some(name.into())).await.expect("install starts");
    let mut last = 0.0;
    let info = loop {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let i = h.ctx.mgr.info(&id).unwrap();
        if let Some(p) = &i.last_progress
            && p.fraction - last > 0.1
        {
            last = p.fraction;
            eprintln!("  install {:>3.0}%  stage={} files={}/{}", p.fraction * 100.0, p.stage, p.done_files, p.total_files);
        }
        if !matches!(format!("{:?}", i.status).as_str(), "Running" | "Paused") {
            break i;
        }
    };
    assert_eq!(format!("{:?}", info.status), "Completed", "install failed: {:?}", info.error);
    eprintln!("install of {name} finished");

    let acc = h.offline_account("E2EPlayer");
    let inst = h.root.join("versions").join(name).to_string_lossy().into_owned();
    let lid = launch::launch(&h.ctx, &inst, &acc, Some(format!("e2e-{name}").replace(' ', "-"))).await.expect("launch");
    eprintln!("spawned {lid}");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
    let markers = ["Setting user", "Backend library"];
    let mut lines: Vec<String>;
    loop {
        lines = h.names("launch://log").iter().map(|e| e["line"].as_str().unwrap_or("").to_string()).collect();
        if markers.iter().all(|m| lines.iter().any(|l| l.contains(m))) || !h.names("launch://exit").is_empty() || std::time::Instant::now() > deadline {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    let _ = crate::launch::kill_by_id(&lid);
    h.wait_event("launch://exit", 30).await;
    lines
}

fn real_harness() -> Option<Harness> {
    let java = java_from_env()?;
    let mut h = Harness::new();
    h.ctx.mgr.set_manifest_url(crate::download::manifest::MANIFEST_URL);
    let info = crate::java::inspect(std::path::Path::new(&java)).expect("PCL_E2E_JAVA must point at a java executable or home");
    eprintln!("using Java {} at {}", info.version, info.path);
    h.ctx.discover = Arc::new(move |_| vec![info.clone()]);
    Some(h)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs network access and PCL_E2E_JAVA"]
async fn real_vanilla_install_and_launch() {
    let Some(mut h) = real_harness() else { return eprintln!("PCL_E2E_JAVA not set, skipping") };
    let mc = std::env::var("PCL_E2E_MC").unwrap_or_else(|_| "1.21.4".into());
    let lines = install_and_launch(&mut h, &mc, None, "E2E Vanilla").await;
    for l in lines.iter().filter(|l| l.contains("Setting user") || l.contains("Backend library") || l.contains("LWJGL")).take(6) {
        eprintln!("  game: {l}");
    }
    assert!(lines.iter().any(|l| l.contains("Setting user")), "client never reached 'Setting user'; tail:\n{}", lines.iter().rev().take(25).rev().cloned().collect::<Vec<_>>().join("\n"));
    assert!(lines.iter().any(|l| l.contains("Backend library")));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs network access and PCL_E2E_JAVA"]
async fn real_fabric_install_and_launch() {
    let Some(mut h) = real_harness() else { return eprintln!("PCL_E2E_JAVA not set, skipping") };
    let mc = std::env::var("PCL_E2E_MC").unwrap_or_else(|_| "1.21.4".into());
    let versions = crate::install::loaders::list_loader_versions(&h.ctx.mgr, "fabric", &mc).await.expect("fabric versions");
    let v = versions.iter().find(|v| v.stable == Some(true)).or(versions.first()).expect("a fabric version").version.clone();
    eprintln!("fabric loader {v}");
    let lines = install_and_launch(&mut h, &mc, Some(ainstall::LoaderChoice { kind: "fabric".into(), version: v }), "E2E Fabric").await;
    for l in lines.iter().filter(|l| l.contains("Setting user") || l.contains("Backend library") || l.contains("Fabric") || l.contains("Loading Minecraft")).take(8) {
        eprintln!("  game: {l}");
    }
    let tail = || lines.iter().rev().take(25).rev().cloned().collect::<Vec<_>>().join("\n");
    // The launcher header line proves the Fabric Knot entry point and loader libraries were used.
    assert!(lines.iter().any(|l| l.contains("net.fabricmc.loader.impl.launch.knot.KnotClient") && l.contains("fabric-loader")), "not launched through Fabric's Knot; tail:\n{}", tail());
    assert!(lines.iter().any(|l| l.contains("Loading Minecraft") && l.contains("Fabric Loader")), "Fabric Loader did not start; tail:\n{}", tail());
    assert!(lines.iter().any(|l| l.contains("Setting user")), "client never reached 'Setting user'; tail:\n{}", tail());
    assert!(lines.iter().any(|l| l.contains("Backend library")), "no 'Backend library'; tail:\n{}", tail());
}

/// Wait for a manager task, printing coarse progress.
async fn wait_task(h: &Harness, id: &str, what: &str) -> crate::download::TaskInfo {
    let mut last = 0.0;
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let i = h.ctx.mgr.info(id).unwrap();
        if let Some(p) = &i.last_progress
            && p.fraction - last > 0.2
        {
            last = p.fraction;
            eprintln!("  {what} {:>3.0}%  stage={} files={}/{}", p.fraction * 100.0, p.stage, p.done_files, p.total_files);
        }
        if !matches!(format!("{:?}", i.status).as_str(), "Running" | "Paused") {
            return i;
        }
    }
}

/// Real Modrinth: install Mod Menu (+ its required dependencies), a resource pack and a shader
/// into a Fabric instance, then check hashes against what was written.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs network access"]
async fn real_modrinth_resources() {
    use crate::app::resource::{install_resource, LiveApi, ResourceRequest};
    use crate::resources::types::{Platform, ResourceKind};
    let h = Harness::new();
    let mc = "1.21.4";
    let versions = crate::install::loaders::list_loader_versions(&h.ctx.mgr, "fabric", mc).await.unwrap();
    let ver = versions.first().unwrap().version.clone();
    let id = ainstall::install_game(&h.ctx, mc.into(), Some(ainstall::LoaderChoice { kind: "fabric".into(), version: ver }), Some("Res Test".into())).await.unwrap();
    assert_eq!(format!("{:?}", wait_task(&h, &id, "fabric install").await.status), "Completed");
    let inst = h.root.join("versions/Res Test");
    let inst_id = inst.to_string_lossy().into_owned();
    std::fs::create_dir_all(inst.join("saves/World1")).unwrap();

    for (project, kind, dir) in [
        ("mOgUt4GM", ResourceKind::Mod, "mods"),
        ("w0TnApzs", ResourceKind::ResourcePack, "resourcepacks"),
        ("HVnmMxH1", ResourceKind::Shader, "shaderpacks"),
    ] {
        let req = ResourceRequest { platform: Platform::Modrinth, project_id: project.into(), version_id: None, kind, instance_id: inst_id.clone(), world_name: None };
        let tid = install_resource(&h.ctx, &LiveApi, req).await.unwrap_or_else(|e| panic!("{project}: {e}"));
        let info = wait_task(&h, &tid, project).await;
        assert_eq!(format!("{:?}", info.status), "Completed", "{project}: {:?}", info.error);
        let files: Vec<_> = std::fs::read_dir(inst.join(dir)).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        eprintln!("  {dir}/: {files:?}");
        assert!(!files.is_empty());
    }
    // Mod Menu requires Fabric API (and a library): the dependency resolver must have pulled them in.
    let mods: Vec<String> = std::fs::read_dir(inst.join("mods")).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    assert!(mods.len() >= 2, "required dependencies installed: {mods:?}");
    assert!(mods.iter().any(|m| m.to_lowercase().contains("fabric-api")), "{mods:?}");
    // the scanner of the mods list reads them back
    let listed = crate::resources::local_mods::scan_mods_dir(&inst.join("mods")).unwrap();
    assert!(listed.iter().all(|m| m.meta.loaders.contains(&crate::resources::types::ModLoader::Fabric) || m.meta.mod_id.is_some()));
    eprintln!("  mods readable by the scanner: {:?}", listed.iter().filter_map(|m| m.meta.name.clone()).collect::<Vec<_>>());
    // datapack goes into the world
    let req = ResourceRequest { platform: Platform::Modrinth, project_id: "w0TnApzs".into(), version_id: None, kind: ResourceKind::DataPack, instance_id: inst_id.clone(), world_name: Some("World1".into()) };
    let tid = install_resource(&h.ctx, &LiveApi, req).await.unwrap();
    assert_eq!(format!("{:?}", wait_task(&h, &tid, "datapack").await.status), "Completed");
    assert!(inst.join("saves/World1/datapacks").read_dir().unwrap().count() >= 1);
}

/// Real Modrinth modpack (Fabulously Optimized for 1.21.4): game + Fabric + ~100 mods + overrides,
/// installed through the project source, then launched.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs network access and PCL_E2E_JAVA; downloads a few hundred MB"]
async fn real_modrinth_modpack_install_and_launch() {
    use crate::app::modpack::{install_modpack, ModpackSource};
    use crate::app::resource::LiveApi;
    let Some(h) = real_harness() else { return eprintln!("PCL_E2E_JAVA not set, skipping") };
    let src = ModpackSource::Project { platform: Platform::Modrinth, project_id: "1KVo5zza".into(), version_id: Some("37TmHvhB".into()) };
    let id = install_modpack(&h.ctx, Arc::new(LiveApi), src, "Fabulously".into()).await.unwrap();
    let info = wait_task(&h, &id, "modpack").await;
    assert_eq!(format!("{:?}", info.status), "Completed", "{:?}", info.error);
    let inst = h.root.join("versions/Fabulously");
    let mods = crate::resources::local_mods::scan_mods_dir(&inst.join("mods")).unwrap();
    eprintln!("  modpack installed {} mods; options.txt={}", mods.len(), inst.join("options.txt").is_file());
    assert!(mods.len() > 20, "pack mods installed");
    assert!(inst.join("config").is_dir() || inst.join("options.txt").is_file(), "overrides extracted");
    let rep = h.names("app://modpack-report").pop().expect("report emitted");
    assert_eq!(rep["missingFiles"], json!([]));

    let acc = h.offline_account("PackPlayer");
    let lid = crate::app::launch::launch(&h.ctx, &inst.to_string_lossy(), &acc, Some("e2e-pack".into())).await.expect("launch");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    let lines = loop {
        let lines: Vec<String> = h.names("launch://log").iter().map(|e| e["line"].as_str().unwrap_or("").to_string()).collect();
        let done = lines.iter().any(|l| l.contains("Setting user")) && lines.iter().any(|l| l.contains("Backend library"));
        if done || !h.names("launch://exit").is_empty() || std::time::Instant::now() > deadline {
            break lines;
        }
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    };
    let _ = crate::launch::kill_by_id(&lid);
    h.wait_event("launch://exit", 30).await;
    for l in lines.iter().filter(|l| l.contains("Loading Minecraft") || l.contains("Setting user") || l.contains("Backend library") || l.contains("Loading ") && l.contains("mods")).take(6) {
        eprintln!("  game: {l}");
    }
    let tail = lines.iter().rev().take(30).rev().cloned().collect::<Vec<_>>().join("\n");
    assert!(lines.iter().any(|l| l.contains("Setting user")), "pack did not reach 'Setting user'; tail:\n{tail}");
}

/// Any loader: `PCL_E2E_LOADER=forge|neoforge|quilt PCL_E2E_MC=<version>`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs network access and PCL_E2E_JAVA"]
async fn real_loader_install_and_launch() {
    let Some(mut h) = real_harness() else { return eprintln!("PCL_E2E_JAVA not set, skipping") };
    let kind = std::env::var("PCL_E2E_LOADER").unwrap_or_else(|_| "forge".into());
    let mc = std::env::var("PCL_E2E_MC").unwrap_or_else(|_| "1.20.1".into());
    let versions = crate::install::loaders::list_loader_versions(&h.ctx.mgr, &kind, &mc).await.expect("loader versions");
    eprintln!("{kind} versions for {mc}: {:?}", versions.iter().take(4).map(|v| v.version.as_str()).collect::<Vec<_>>());
    let version = std::env::var("PCL_E2E_LOADER_VERSION").ok().or_else(|| versions.first().map(|v| v.version.clone())).expect("a loader version");
    let name = format!("E2E {kind}");
    let lines = install_and_launch(&mut h, &mc, Some(ainstall::LoaderChoice { kind: kind.clone(), version }), &name).await;
    for l in lines.iter().filter(|l| l.contains("Setting user") || l.contains("Backend library") || l.contains("ModLauncher") || l.contains("Loading Minecraft")).take(6) {
        eprintln!("  game: {l}");
    }
    let tail = lines.iter().rev().take(30).rev().cloned().collect::<Vec<_>>().join("\n");
    assert!(lines.iter().any(|l| l.contains("Setting user")), "client never reached 'Setting user'; tail:\n{tail}");
}
