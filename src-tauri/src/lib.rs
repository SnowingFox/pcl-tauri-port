mod crash;
mod instances;
mod settings;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            settings::commands::settings_get,
            settings::commands::settings_set,
            settings::commands::settings_patch,
            settings::commands::settings_reset,
            settings::commands::settings_path,
            settings::commands::instance_settings_get,
            settings::commands::instance_settings_set,
            instances::commands::mc_folders_list,
            instances::commands::mc_folder_add,
            instances::commands::mc_folder_remove,
            instances::commands::mc_folder_select,
            instances::commands::instances_scan,
            instances::commands::instance_get,
            instances::commands::instance_select,
            instances::commands::instance_rename,
            instances::commands::instance_duplicate,
            instances::commands::instance_delete,
            instances::commands::instance_open_in_finder,
            instances::commands::instance_export_info,
            crash::commands::crash_analyze_instance,
            crash::commands::crash_analyze_file,
            crash::commands::crash_analyze_text,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
