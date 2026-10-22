mod accounts;

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
            accounts::commands::accounts_authlib_injector_info
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
