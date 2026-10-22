//! `app_install_game`, `app_install_resource`, `app_install_modpack` against a local server.

use super::*;
use crate::app::{install as ainstall, modpack as amodpack, resource as ares};
use crate::resources::types::ResourceKind;
use std::io::Write;

fn zip_of(files: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let o = zip::write::SimpleFileOptions::default();
    for (n, c) in files {
        w.start_file(*n, o).unwrap();
        w.write_all(c).unwrap();
    }
    w.finish().unwrap().into_inner()
}

#[tokio::test]
async fn install_vanilla_with_custom_name_registers_instance() {
    let h = Harness::new();
    let s = serve_vanilla("1.20.1").await;
    h.ctx.mgr.set_manifest_url(&s.manifest_url);
    let id = ainstall::install_game(&h.ctx, "1.20.1".into(), None, Some("My Survival".into())).await.unwrap();
    let info = h.finish(&id).await;
    assert_eq!(format!("{:?}", info.status), "Completed", "{:?}", info.error);

    assert!(h.root.join("versions/My Survival/My Survival.json").is_file());
    assert!(h.root.join("versions/My Survival/My Survival.jar").is_file());
    assert!(!h.root.join("versions/1.20.1").exists(), "no stray vanilla folder");
    assert!(h.root.join("libraries/com/example/lib/1.0/lib-1.0.jar").is_file());
    let v: Value = serde_json::from_slice(&std::fs::read(h.root.join("versions/My Survival/My Survival.json")).unwrap()).unwrap();
    assert_eq!(v["id"], "My Survival");

    // listed by the instances module, selected in settings, finish event emitted
    let g = h.ctx.settings.get();
    assert_eq!(g.game.selected_instance.as_deref(), Some("My Survival"));
    let list = crate::instances::scan::scan_folder(&h.root, g.game.isolation);
    assert_eq!(list.len(), 1);
    assert_eq!((list[0].name.as_str(), list[0].mc_version.as_deref(), list[0].error.as_deref()), ("My Survival", Some("1.20.1"), None));
    let fin = h.names("download://finished");
    assert_eq!(fin.last().unwrap()["status"], "completed");
    let prog = h.names("download://progress");
    assert!(prog.iter().any(|p| p["task_id"] == json!(id)));
    let fr: Vec<f64> = prog.iter().filter(|p| p["task_id"] == json!(id)).map(|p| p["fraction"].as_f64().unwrap()).collect();
    assert!(fr.windows(2).all(|w| w[1] + 1e-9 >= w[0]), "monotonic: {fr:?}");
    assert!(fr.iter().all(|f| (0.0..=1.0 + 1e-9).contains(f)));
    assert_eq!(h.names("app://instance-installed")[0]["name"], "My Survival");

    // name clash is rejected up front; invalid names too
    let e = ainstall::install_game(&h.ctx, "1.20.1".into(), None, Some("My Survival".into())).await.unwrap_err();
    assert_eq!(e.code, "INSTANCE_EXISTS");
    assert_eq!(ainstall::install_game(&h.ctx, "1.20.1".into(), None, Some("a/b".into())).await.unwrap_err().code, "INVALID_ARGUMENT");
    let bad = ainstall::LoaderChoice { kind: "optifine".into(), version: "1".into() };
    assert_eq!(ainstall::install_game(&h.ctx, "1.20.1".into(), Some(bad), None).await.unwrap_err().code, "INVALID_ARGUMENT");
}

#[tokio::test]
async fn install_vanilla_default_name_keeps_version_id_and_unknown_version_fails() {
    let h = Harness::new();
    let s = serve_vanilla("1.20.1").await;
    h.ctx.mgr.set_manifest_url(&s.manifest_url);
    let id = ainstall::install_game(&h.ctx, "1.20.1".into(), None, None).await.unwrap();
    assert_eq!(format!("{:?}", h.finish(&id).await.status), "Completed");
    assert!(h.root.join("versions/1.20.1/1.20.1.jar").is_file());
    assert_eq!(h.ctx.settings.get().game.selected_instance.as_deref(), Some("1.20.1"));

    let id = ainstall::install_game(&h.ctx, "9.9.9".into(), None, None).await.unwrap();
    let info = h.finish(&id).await;
    assert_eq!(format!("{:?}", info.status), "Failed");
    assert!(info.error.unwrap().contains("unknown Minecraft version"));
    assert!(!h.root.join("versions/9.9.9").exists());
}

fn write_mod_instance(h: &Harness, mc: &str) -> String {
    let id = "fabric-loader-0.15.0-1.20.1";
    write_version_json(&h.root, mc, &format!(r#"{{"id":"{mc}","type":"release","mainClass":"net.minecraft.client.main.Main","libraries":[]}}"#));
    write_version_json(
        &h.root,
        id,
        &format!(r#"{{"id":"{id}","inheritsFrom":"{mc}","type":"release","mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient","libraries":[{{"name":"net.fabricmc:fabric-loader:0.15.0"}}]}}"#),
    );
    h.root.join("versions").join(id).to_string_lossy().into_owned()
}

#[tokio::test]
async fn resources_land_in_the_right_folders_with_hash_checks() {
    let h = Harness::new();
    let inst = write_mod_instance(&h, "1.20.1");
    let srv = MockServer::start().await;
    let (mod_body, lib_body, pack_body, shader_body) = (b"MOD".to_vec(), b"LIB".to_vec(), b"RESOURCEPACK".to_vec(), b"SHADER".to_vec());
    for (p, b) in [("/mod.jar", &mod_body), ("/lib.jar", &lib_body), ("/pack.zip", &pack_body), ("/shader.zip", &shader_body)] {
        Mock::given(method("GET")).and(path(p)).respond_with(ResponseTemplate::new(200).set_body_bytes(b.clone())).mount(&srv).await;
    }
    let api = FakeApi::default();
    let mut m = resource_file("mod", "m1", "cool-mod.jar", Some(format!("{}/mod.jar", srv.uri())), &mod_body);
    m.dependencies = vec![crate::resources::types::Dependency { project_id: Some("lib".into()), file_id: None, kind: crate::resources::types::DependencyKind::Required }];
    api.versions.lock().unwrap().insert("mod".into(), vec![m]);
    api.versions.lock().unwrap().insert("lib".into(), vec![resource_file("lib", "l1", "fabric-api.jar", Some(format!("{}/lib.jar", srv.uri())), &lib_body)]);
    let mut rp = resource_file("rp", "r1", "faithful.zip", Some(format!("{}/pack.zip", srv.uri())), &pack_body);
    rp.loaders.clear();
    api.versions.lock().unwrap().insert("rp".into(), vec![rp]);
    let mut sh = resource_file("sh", "s1", "bsl.zip", Some(format!("{}/shader.zip", srv.uri())), &shader_body);
    sh.loaders.clear();
    api.versions.lock().unwrap().insert("sh".into(), vec![sh]);

    let req = |project: &str, kind, world: Option<&str>| ares::ResourceRequest {
        platform: Platform::Modrinth, project_id: project.into(), version_id: None, kind, instance_id: inst.clone(), world_name: world.map(String::from),
    };
    // mod + its required dependency -> mods/
    let id = ares::install_resource(&h.ctx, &api, req("mod", ResourceKind::Mod, None)).await.unwrap();
    assert_eq!(format!("{:?}", h.finish(&id).await.status), "Completed");
    let game = h.root.join("versions/fabric-loader-0.15.0-1.20.1");
    assert_eq!(std::fs::read(game.join("mods/cool-mod.jar")).unwrap(), mod_body);
    assert_eq!(std::fs::read(game.join("mods/fabric-api.jar")).unwrap(), lib_body);
    assert_eq!(h.names("app://resource-report")[0]["installed"].as_array().unwrap().len(), 2);
    // it shows up in the mod list command's data source
    let listed = crate::resources::local_mods::scan_mods_dir(&game.join("mods")).unwrap();
    assert_eq!(listed.len(), 2);

    // resource pack + shader
    for (p, kind, dir, name) in [("rp", ResourceKind::ResourcePack, "resourcepacks", "faithful.zip"), ("sh", ResourceKind::Shader, "shaderpacks", "bsl.zip")] {
        let id = ares::install_resource(&h.ctx, &api, req(p, kind, None)).await.unwrap();
        assert_eq!(format!("{:?}", h.finish(&id).await.status), "Completed");
        assert!(game.join(dir).join(name).is_file(), "{dir}/{name}");
    }

    // data packs need a world
    assert_eq!(ares::install_resource(&h.ctx, &api, req("rp", ResourceKind::DataPack, None)).await.unwrap_err().code, "INVALID_ARGUMENT");
    assert_eq!(ares::install_resource(&h.ctx, &api, req("rp", ResourceKind::DataPack, Some("Nope"))).await.unwrap_err().code, "WORLD_NOT_FOUND");
    std::fs::create_dir_all(game.join("saves/World1")).unwrap();
    let id = ares::install_resource(&h.ctx, &api, req("rp", ResourceKind::DataPack, Some("World1"))).await.unwrap();
    assert_eq!(format!("{:?}", h.finish(&id).await.status), "Completed");
    assert!(game.join("saves/World1/datapacks/faithful.zip").is_file());

    // a hash mismatch fails the task and leaves nothing behind
    let mut bad = resource_file("bad", "b1", "bad.jar", Some(format!("{}/mod.jar", srv.uri())), &mod_body);
    bad.sha1 = Some("0".repeat(40)); // right size, wrong hash
    api.versions.lock().unwrap().insert("bad".into(), vec![bad]);
    let id = ares::install_resource(&h.ctx, &api, req("bad", ResourceKind::Mod, None)).await.unwrap();
    let info = h.finish(&id).await;
    assert_eq!(format!("{:?}", info.status), "Failed");
    assert!(info.error.unwrap().contains("hash mismatch"));
    assert!(!game.join("mods/bad.jar").exists() && !game.join("mods/bad.jar.part").exists());

    // mods need a moddable instance
    write_version_json(&h.root, "1.20.1-plain", r#"{"id":"1.20.1","type":"release","mainClass":"net.minecraft.client.main.Main","libraries":[]}"#);
    let mut r = req("mod", ResourceKind::Mod, None);
    r.instance_id = "1.20.1-plain".into();
    assert_eq!(ares::install_resource(&h.ctx, &api, r).await.unwrap_err().code, "INSTANCE_NOT_MODDABLE");
    // and a project without a compatible version is reported as such
    assert_eq!(ares::install_resource(&h.ctx, &api, req("ghost", ResourceKind::Mod, None)).await.unwrap_err().code, "NO_COMPATIBLE_VERSION");
}

fn modrinth_pack(base: &str, files: &[(&str, &[u8])]) -> Vec<u8> {
    let entries: Vec<Value> = files
        .iter()
        .map(|(p, b)| json!({"path": p, "hashes": {"sha1": sha(b)}, "downloads": [format!("{base}/{}", p.rsplit('/').next().unwrap())], "fileSize": b.len()}))
        .collect();
    let index = json!({"formatVersion": 1, "game": "minecraft", "versionId": "1.0", "name": "Test Pack", "files": entries,
        "dependencies": {"minecraft": "1.20.1"}});
    zip_of(&[("modrinth.index.json", index.to_string().into_bytes()), ("overrides/config/pack.toml", b"hello = true".to_vec())])
}

fn sha(b: &[u8]) -> String {
    crate::resources::hash::sha1_hex(b)
}

#[tokio::test]
async fn modrinth_pack_installs_game_files_and_overrides() {
    let h = Harness::new();
    let s = serve_vanilla("1.20.1").await;
    h.ctx.mgr.set_manifest_url(&s.manifest_url);
    let (a, b) = (b"mod-a-bytes".to_vec(), b"mod-b-bytes".to_vec());
    for (p, body) in [("/a.jar", &a), ("/b.jar", &b)] {
        Mock::given(method("GET")).and(path(p)).respond_with(ResponseTemplate::new(200).set_body_bytes(body.clone())).mount(&s.server).await;
    }
    let pack = h.dir.path().join("pack.mrpack");
    std::fs::write(&pack, modrinth_pack(&s.server.uri(), &[("mods/a.jar", &a), ("mods/b.jar", &b)])).unwrap();

    let id = amodpack::install_modpack(&h.ctx, Arc::new(FakeApi::default()), amodpack::ModpackSource::File { path: pack.to_string_lossy().into() }, "Pack Instance".into()).await.unwrap();
    let info = h.finish(&id).await;
    assert_eq!(format!("{:?}", info.status), "Completed", "{:?}", info.error);

    let inst = h.root.join("versions/Pack Instance");
    assert!(inst.join("Pack Instance.json").is_file() && inst.join("Pack Instance.jar").is_file());
    assert_eq!(std::fs::read(inst.join("mods/a.jar")).unwrap(), a);
    assert_eq!(std::fs::read(inst.join("mods/b.jar")).unwrap(), b);
    assert_eq!(std::fs::read_to_string(inst.join("config/pack.toml")).unwrap(), "hello = true");
    assert!(!h.root.join("versions/1.20.1").exists());
    assert!(!h.root.join(".pcl-tmp").join(format!("pack-{id}")).exists());
    // isolated so the pack owns its mods, and visible in the instance list
    assert_eq!(crate::settings::InstanceSettings::load(&inst).isolated, Some(true));
    let list = crate::instances::scan::scan_folder(&h.root, h.ctx.settings.get().game.isolation);
    assert!(list.iter().any(|i| i.name == "Pack Instance" && i.isolated));
    assert_eq!(h.ctx.settings.get().game.selected_instance.as_deref(), Some("Pack Instance"));
    let rep = h.names("app://modpack-report")[0].clone();
    assert_eq!(rep["missingFiles"], json!([]));
    assert_eq!(h.ctx.reports.lock().unwrap().get(&id).unwrap().missing_files.len(), 0);
}

#[tokio::test]
async fn pack_with_bad_hash_fails_and_cleans_up() {
    let h = Harness::new();
    let s = serve_vanilla("1.20.1").await;
    h.ctx.mgr.set_manifest_url(&s.manifest_url);
    Mock::given(method("GET")).and(path("/a.jar")).respond_with(ResponseTemplate::new(200).set_body_bytes(b"tampered".to_vec())).mount(&s.server).await;
    let pack = h.dir.path().join("pack.mrpack");
    std::fs::write(&pack, modrinth_pack(&s.server.uri(), &[("mods/a.jar", b"expected bytes")])).unwrap();
    let id = amodpack::install_modpack(&h.ctx, Arc::new(FakeApi::default()), amodpack::ModpackSource::File { path: pack.to_string_lossy().into() }, "Broken".into()).await.unwrap();
    let info = h.finish(&id).await;
    assert_eq!(format!("{:?}", info.status), "Failed");
    assert!(info.error.unwrap().contains("hash mismatch"));
    assert!(!h.root.join("versions/Broken").exists(), "failed installs leave no instance behind");
    assert_ne!(h.ctx.settings.get().game.selected_instance.as_deref(), Some("Broken"));
}

#[tokio::test]
async fn modpack_input_errors_are_typed() {
    let h = Harness::new();
    let api: Arc<dyn ares::ResourceApi> = Arc::new(FakeApi::default());
    let src = |p: &str| amodpack::ModpackSource::File { path: p.into() };
    assert_eq!(amodpack::install_modpack(&h.ctx, api.clone(), src("/no/such.mrpack"), "X".into()).await.unwrap_err().code, "INVALID_ARGUMENT");
    let junk = h.dir.path().join("junk.zip");
    std::fs::write(&junk, zip_of(&[("readme.txt", b"hi".to_vec())])).unwrap();
    assert_eq!(amodpack::install_modpack(&h.ctx, api.clone(), src(&junk.to_string_lossy()), "X".into()).await.unwrap_err().code, "UNSUPPORTED");
    assert_eq!(amodpack::install_modpack(&h.ctx, api.clone(), src(&junk.to_string_lossy()), "a/b".into()).await.unwrap_err().code, "INVALID_ARGUMENT");
    write_version_json(&h.root, "Taken", r#"{"id":"Taken"}"#);
    assert_eq!(amodpack::install_modpack(&h.ctx, api.clone(), src(&junk.to_string_lossy()), "Taken".into()).await.unwrap_err().code, "INSTANCE_EXISTS");
    let proj = amodpack::ModpackSource::Project { platform: Platform::Modrinth, project_id: "../x".into(), version_id: None };
    assert_eq!(amodpack::install_modpack(&h.ctx, api, proj, "Y".into()).await.unwrap_err().code, "INVALID_ARGUMENT");
}

#[tokio::test]
async fn curseforge_pack_reports_restricted_files_instead_of_failing() {
    let h = Harness::new();
    let s = serve_vanilla("1.20.1").await;
    h.ctx.mgr.set_manifest_url(&s.manifest_url);
    let (ok_body, soft_body) = (b"downloadable".to_vec(), b"restricted-but-cdn-works".to_vec());
    for (p, b) in [("/ok.jar", &ok_body), ("/soft.jar", &soft_body)] {
        Mock::given(method("GET")).and(path(p)).respond_with(ResponseTemplate::new(200).set_body_bytes(b.clone())).mount(&s.server).await;
    }
    let manifest = json!({
        "minecraft": {"version": "1.20.1", "modLoaders": []}, "manifestType": "minecraftModpack", "name": "CF", "version": "1", "overrides": "overrides",
        "files": [{"projectID": 1, "fileID": 10, "required": true}, {"projectID": 2, "fileID": 20, "required": true},
                  {"projectID": 3, "fileID": 30, "required": true}, {"projectID": 4, "fileID": 40, "required": true}]
    });
    let pack = h.dir.path().join("cf.zip");
    std::fs::write(&pack, zip_of(&[("manifest.json", manifest.to_string().into_bytes()), ("overrides/options.txt", b"x".to_vec())])).unwrap();

    let api = FakeApi::default();
    let mk = |id: &str, project: &str, name: &str, url: Option<String>, body: &[u8]| {
        let mut f = resource_file(project, id, name, url, body);
        f.platform = Platform::CurseForge;
        f
    };
    api.cf_files.lock().unwrap().extend([
        mk("10", "1", "ok.jar", Some(format!("{}/ok.jar", s.server.uri())), &ok_body),
        mk("20", "2", "blocked.jar", None, b"never available"),
        mk("30", "3", "soft.jar", None, &soft_body),
        // file 40 was deleted upstream: absent from the API answer
    ]);
    api.cf_urls.lock().unwrap().insert("30".into(), format!("{}/soft.jar", s.server.uri()));

    let id = amodpack::install_modpack(&h.ctx, Arc::new(api), amodpack::ModpackSource::File { path: pack.to_string_lossy().into() }, "CF Pack".into()).await.unwrap();
    let info = h.finish(&id).await;
    assert_eq!(format!("{:?}", info.status), "Completed", "restricted files must not fail the install: {:?}", info.error);

    let inst = h.root.join("versions/CF Pack");
    assert_eq!(std::fs::read(inst.join("mods/ok.jar")).unwrap(), ok_body);
    assert_eq!(std::fs::read(inst.join("mods/soft.jar")).unwrap(), soft_body, "CDN fallback was usable");
    assert!(!inst.join("mods/blocked.jar").exists());
    assert_eq!(std::fs::read_to_string(inst.join("options.txt")).unwrap(), "x");

    let rep = h.wait_event("app://modpack-report", 5).await;
    let missing = rep["missingFiles"].as_array().unwrap();
    let ids: Vec<u64> = missing.iter().map(|m| m["fileId"].as_u64().unwrap()).collect();
    assert_eq!(ids.len(), 2, "{rep}");
    assert!(ids.contains(&20) && ids.contains(&40));
    let blocked = missing.iter().find(|m| m["fileId"] == 20).unwrap();
    assert_eq!(blocked["path"], "mods/blocked.jar");
    assert!(blocked["pageUrl"].as_str().unwrap().starts_with("https://www.curseforge.com/"));
    // the same report is retrievable later (UI opened after the event)
    assert_eq!(h.ctx.reports.lock().unwrap().get(&id).unwrap().missing_files.len(), 2);
}

#[tokio::test]
async fn project_source_downloads_the_archive_first() {
    let h = Harness::new();
    let s = serve_vanilla("1.20.1").await;
    h.ctx.mgr.set_manifest_url(&s.manifest_url);
    let a = b"remote-mod".to_vec();
    Mock::given(method("GET")).and(path("/a.jar")).respond_with(ResponseTemplate::new(200).set_body_bytes(a.clone())).mount(&s.server).await;
    let archive = modrinth_pack(&s.server.uri(), &[("mods/a.jar", &a)]);
    Mock::given(method("GET")).and(path("/pack.mrpack")).respond_with(ResponseTemplate::new(200).set_body_bytes(archive.clone())).mount(&s.server).await;
    let api = FakeApi::default();
    api.versions.lock().unwrap().insert("packproj".into(), vec![resource_file("packproj", "v1", "pack.mrpack", Some(format!("{}/pack.mrpack", s.server.uri())), &archive)]);
    let src = amodpack::ModpackSource::Project { platform: Platform::Modrinth, project_id: "packproj".into(), version_id: None };
    let id = amodpack::install_modpack(&h.ctx, Arc::new(api), src, "From Project".into()).await.unwrap();
    let info = h.finish(&id).await;
    assert_eq!(format!("{:?}", info.status), "Completed", "{:?}", info.error);
    assert_eq!(std::fs::read(h.root.join("versions/From Project/mods/a.jar")).unwrap(), a);
    assert!(!h.root.join(".pcl-tmp").join(format!("pack-{id}")).exists(), "temp archive removed");
}
