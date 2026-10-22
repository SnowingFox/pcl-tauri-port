mod modpack;
mod resources;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![greet, resources::commands::resource_search, resources::commands::resource_project, resources::commands::resource_versions, resources::commands::curseforge_download_url, resources::commands::local_mods_scan, resources::commands::local_mod_set_enabled, resources::commands::local_mods_check_updates, modpack::commands::modpack_parse, modpack::commands::modpack_resolve, modpack::commands::modpack_extract_overrides])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
