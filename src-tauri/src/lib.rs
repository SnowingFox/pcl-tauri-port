mod download;
mod install;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            use tauri::Manager;
            app.manage(download::new_manager(app.handle().clone()));
            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            download::download_get_config,
            download::download_set_config,
            download::download_version_manifest,
            download::download_start_file,
            download::download_pause,
            download::download_resume,
            download::download_cancel,
            download::download_task_info,
            download::download_list_tasks,
            download::download_clear_finished,
            install::install_vanilla,
            install::install_loader,
            install::install_list_loader_versions,
            install::install_list_optifine,
            install::install_optifine_mod,
            install::install_list_installed,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
