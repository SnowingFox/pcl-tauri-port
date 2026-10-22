mod java;
mod launch;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![greet, java::java_discover, java::java_inspect, launch::launch_precheck, launch::launch_build_command, launch::launch_start, launch::launch_kill, launch::launch_list_running, launch::launch_missing_assets])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
