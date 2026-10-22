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
        if let Some(p) = &i.last_progress {
            if p.fraction - last > 0.1 {
                last = p.fraction;
                eprintln!("  install {:>3.0}%  stage={} files={}/{}", p.fraction * 100.0, p.stage, p.done_files, p.total_files);
            }
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
    assert!(lines.iter().any(|l| l.contains("Setting user") || l.contains("Fabric") || l.contains("Backend library")), "tail:\n{}", lines.iter().rev().take(25).rev().cloned().collect::<Vec<_>>().join("\n"));
}
