mod accounts;
mod java;
mod launch;
mod crash;
mod instances;
mod settings;
mod download;
mod install;
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
        .setup(|app| {
            use tauri::Manager;
            app.manage(download::new_manager(app.handle().clone()));
            Ok(())
        })
        .plugin(settings::guard::navigation_guard())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            accounts::commands::accounts_list,
            accounts::commands::accounts_get_selected,
            accounts::commands::accounts_select,
            accounts::commands::accounts_remove,
            accounts::commands::accounts_add_offline,
            accounts::commands::accounts_offline_uuid,
            accounts::commands::accounts_get_ms_client_id,
            accounts::commands::accounts_set_ms_client_id,
            accounts::commands::accounts_microsoft_login,
            accounts::commands::accounts_cancel_login,
            accounts::commands::accounts_yggdrasil_server_info,
            accounts::commands::accounts_add_yggdrasil,
            accounts::commands::accounts_get_session,
            accounts::commands::accounts_get_skin,
            accounts::commands::accounts_authlib_injector_info,
            java::java_discover,
            java::java_inspect,
            launch::launch_precheck,
            launch::launch_build_command,
            launch::launch_start,
            launch::launch_kill,
            launch::launch_list_running,
            launch::launch_missing_assets,
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
            resources::commands::resource_search,
            resources::commands::resource_project,
            resources::commands::resource_versions,
            resources::commands::curseforge_download_url,
            resources::commands::local_mods_scan,
            resources::commands::local_mod_set_enabled,
            resources::commands::local_mods_check_updates,
            modpack::commands::modpack_parse,
            modpack::commands::modpack_resolve,
            modpack::commands::modpack_extract_overrides,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
