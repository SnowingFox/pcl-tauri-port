//! The real Tauri IPC path: JSON args in (camelCase), JSON out, `AppError` objects on failure.
//! Uses tauri's mock runtime with the same command functions `lib.rs` registers.

use super::*;
use crate::app::commands::*;
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::WebviewWindow;

fn app_for(h: &Harness) -> (tauri::App<MockRuntime>, WebviewWindow<MockRuntime>) {
    let app = mock_builder()
        .manage(Arc::new(h.ctx.clone()))
        .invoke_handler(tauri::generate_handler![
            app_launch, app_launch_kill, app_install_game, app_install_resource, app_install_modpack, app_instance_mods, app_instance_saves,
            app_modpack_report, app_data_dirs, app_open_url,
        ])
        .build(mock_context(noop_assets()))
        .unwrap();
    let w = tauri::WebviewWindowBuilder::new(&app, "main", Default::default()).build().unwrap();
    (app, w)
}

fn call(w: &WebviewWindow<MockRuntime>, cmd: &str, args: Value) -> Result<Value, Value> {
    let res = get_ipc_response(
        w,
        InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: if cfg!(windows) { "http://tauri.localhost" } else { "tauri://localhost" }.parse().unwrap(),
            body: InvokeBody::Json(args),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    );
    res.map(|b| b.deserialize::<Value>().unwrap())
}

#[test]
fn commands_use_camel_case_args_and_typed_errors() {
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap();
    let _g = rt.enter();
    let mut h = Harness::new();
    h.use_java(OK_JAVA);
    let (_app, w) = app_for(&h);

    // errors are objects with a stable code, not strings
    let s = rt.block_on(serve_vanilla("1.20.1"));
    write_version_json(&h.root, "1.20.1", &s.version_json);
    let e = call(&w, "app_launch", json!({"instanceId": "1.20.1", "accountId": "nope"})).unwrap_err();
    assert_eq!(e["code"], "ACCOUNT_NOT_FOUND", "{e}");
    assert!(e["message"].is_string());
    let acc = h.offline_account("Steve");
    let e = call(&w, "app_launch", json!({"instanceId": "ghost", "accountId": acc})).unwrap_err();
    assert_eq!(e["code"], "INSTANCE_NOT_FOUND");
    assert_eq!(e["details"]["instanceId"], "ghost");
    let e = call(&w, "app_launch", json!({"instanceId": "ghost", "accountId": acc, "launchId": "../x"})).unwrap_err();
    assert!(matches!(e["code"].as_str(), Some("INSTANCE_NOT_FOUND" | "INVALID_ARGUMENT")));

    // missing / misnamed arguments are rejected by Tauri before reaching our code
    assert!(call(&w, "app_launch", json!({"instance_id": "x", "account_id": "y"})).is_err(), "snake_case keys are not the JS contract");

    // game install: camelCase args, loader object, returns { taskId }
    let e = call(&w, "app_install_game", json!({"mcVersion": "1.20.1", "loader": {"kind": "optifine", "version": "1"}})).unwrap_err();
    assert_eq!(e["code"], "INVALID_ARGUMENT");
    let e = call(&w, "app_install_game", json!({"mcVersion": "1.20.1", "instanceName": "bad/name"})).unwrap_err();
    assert_eq!(e["code"], "INVALID_ARGUMENT");

    // exactly the JSON the frontend (src/api/resources.ts, download.ts) sends
    let e = call(&w, "app_install_resource", json!({"platform": "modrinth", "projectId": "mOgUt4GM", "versionId": null, "kind": "mod", "instanceId": "ghost"})).unwrap_err();
    assert_eq!(e["code"], "INSTANCE_NOT_FOUND", "args deserialised, failure comes from the instance lookup: {e}");
    let e = call(&w, "app_install_resource", json!({"platform": "curseforge", "projectId": "1", "kind": "datapack", "instanceId": "1.20.1"})).unwrap_err();
    assert_eq!(e["code"], "INVALID_ARGUMENT", "datapack without worldName");
    let e = call(&w, "app_install_resource", json!({"platform": "gitlab", "projectId": "1", "kind": "mod", "instanceId": "x"})).unwrap_err();
    assert!(e.to_string().contains("platform") || e.to_string().contains("variant"), "unknown platform is a deserialisation error: {e}");
    let e = call(&w, "app_install_modpack", json!({"source": {"type": "file", "path": "/no/such/pack.mrpack"}, "instanceName": "P"})).unwrap_err();
    assert_eq!(e["code"], "INVALID_ARGUMENT");
    let e = call(&w, "app_install_modpack", json!({"source": {"type": "project", "platform": "modrinth", "projectId": "../x", "versionId": null}, "instanceName": "P"})).unwrap_err();
    assert_eq!(e["code"], "INVALID_ARGUMENT");
    let e = call(&w, "app_open_url", json!({"url": "file:///etc/passwd"}));
    assert!(e.is_err(), "non-http urls are rejected over IPC too");

    // saves + mods of a real instance dir
    let inst = h.root.join("versions/1.20.1");
    std::fs::create_dir_all(inst.join("saves/Alpha")).unwrap();
    std::fs::write(inst.join("saves/Alpha/level.dat"), crate::app::saves::build_level_dat("Alpha World", 1_700_000_000_000)).unwrap();
    let inst_id = inst.to_string_lossy().into_owned();
    let saves = call(&w, "app_instance_saves", json!({"instanceId": inst_id})).unwrap();
    assert_eq!(saves[0]["folder"], "Alpha");
    assert_eq!(saves[0]["name"], "Alpha World");
    assert_eq!(saves[0]["lastPlayedMs"], 1_700_000_000_000i64);
    assert!(saves[0].get("sizeBytes").is_some() && saves[0].get("iconDataUrl").is_some(), "camelCase fields: {}", saves[0]);
    assert_eq!(call(&w, "app_instance_mods", json!({"instanceId": inst_id})).unwrap(), json!([]));
    assert_eq!(call(&w, "app_modpack_report", json!({"taskId": "task-404"})).unwrap(), Value::Null);
    let dirs = call(&w, "app_data_dirs", json!({})).unwrap();
    assert!(dirs["logs"].as_str().unwrap().ends_with("logs"));
}

#[test]
fn launch_over_ipc_returns_launch_id_and_emits_events() {
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap();
    let _g = rt.enter();
    let mut h = Harness::new();
    h.use_java(OK_JAVA);
    let s = rt.block_on(serve_vanilla("1.20.1"));
    h.ctx.mgr.set_manifest_url(&s.manifest_url);
    write_version_json(&h.root, "1.20.1", &s.version_json);
    let acc = h.offline_account("Steve");
    let (_app, w) = app_for(&h);
    let inst_id = h.root.join("versions/1.20.1").to_string_lossy().into_owned();

    let r = call(&w, "app_launch", json!({"instanceId": inst_id, "accountId": acc, "launchId": "ipc-1"})).expect("launch over IPC");
    assert_eq!(r, json!({"launchId": "ipc-1"}));
    rt.block_on(h.wait_event("launch://exit", 20));
    let exit = h.names("launch://exit").pop().unwrap();
    assert_eq!((exit["launchId"].clone(), exit["code"].clone(), exit["crashed"].clone()), (json!("ipc-1"), json!(0), json!(false)));

    // installing a game through IPC returns a task id that the download manager knows
    h.ctx.mgr.forget_finished();
    let t = call(&w, "app_install_game", json!({"mcVersion": "1.20.1", "instanceName": "Via IPC"})).expect("install over IPC");
    let task_id = t["taskId"].as_str().unwrap().to_string();
    let info = rt.block_on(h.finish(&task_id));
    assert_eq!(format!("{:?}", info.status), "Completed", "{:?}", info.error);
    assert!(h.root.join("versions/Via IPC/Via IPC.json").is_file());
}
