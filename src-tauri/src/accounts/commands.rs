//! Tauri command surface. See docs/api/accounts.md.
use super::manager::AccountManager;
use super::secrets::KeychainStore;
use super::{yggdrasil, AccountError, AccountInfo, AuthSession};
use std::sync::{Arc, OnceLock};
use tauri::{AppHandle, Emitter};

static MANAGER: OnceLock<AccountManager> = OnceLock::new();

pub const EVENT_DEVICE_CODE: &str = "accounts://device-code";

/// The process-wide account manager (state lives in the app data dir shared with settings).
pub fn manager() -> Result<&'static AccountManager, AccountError> {
    if let Some(m) = MANAGER.get() {
        return Ok(m);
    }
    let m = AccountManager::new(crate::settings::store::data_dir(), Arc::new(KeychainStore))?;
    Ok(MANAGER.get_or_init(|| m))
}

fn mgr(_app: &AppHandle) -> Result<&'static AccountManager, AccountError> {
    manager()
}

type R<T> = Result<T, AccountError>;

#[tauri::command]
pub fn accounts_list(app: AppHandle) -> R<Vec<AccountInfo>> {
    Ok(mgr(&app)?.list())
}

#[tauri::command]
pub fn accounts_get_selected(app: AppHandle) -> R<Option<String>> {
    Ok(mgr(&app)?.selected())
}

#[tauri::command]
pub fn accounts_select(app: AppHandle, id: String) -> R<()> {
    mgr(&app)?.select(&id)
}

#[tauri::command]
pub fn accounts_remove(app: AppHandle, id: String) -> R<()> {
    mgr(&app)?.remove(&id)
}

#[tauri::command]
pub fn accounts_add_offline(app: AppHandle, name: String) -> R<AccountInfo> {
    mgr(&app)?.add_offline(&name)
}

#[tauri::command]
pub fn accounts_offline_uuid(name: String) -> String {
    super::offline::offline_uuid(&name)
}

#[tauri::command]
pub fn accounts_get_ms_client_id(app: AppHandle) -> R<Option<String>> {
    Ok(mgr(&app)?.client_id().ok())
}

#[tauri::command]
pub fn accounts_set_ms_client_id(app: AppHandle, client_id: Option<String>) -> R<()> {
    mgr(&app)?.set_client_id(client_id)
}

/// Starts the device-code flow. Emits `accounts://device-code` immediately, then
/// resolves when the user finished signing in (or errors / is cancelled).
#[tauri::command]
pub async fn accounts_microsoft_login(app: AppHandle) -> R<AccountInfo> {
    let m = mgr(&app)?;
    let (cid, dc) = m.ms_begin().await?;
    let _ = app.emit(EVENT_DEVICE_CODE, &dc);
    m.ms_finish(&cid, &dc).await
}

#[tauri::command]
pub fn accounts_cancel_login(app: AppHandle) -> R<()> {
    mgr(&app)?.cancel_login();
    Ok(())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfoDto {
    pub url: String,
    pub name: String,
}

#[tauri::command]
pub async fn accounts_yggdrasil_server_info(app: AppHandle, url: String) -> R<ServerInfoDto> {
    let i: yggdrasil::ServerInfo = mgr(&app)?.server_info(&url).await?;
    Ok(ServerInfoDto { url: i.url, name: i.name })
}

#[tauri::command]
pub async fn accounts_add_yggdrasil(app: AppHandle, server: String, identifier: String, password: String) -> R<Vec<AccountInfo>> {
    mgr(&app)?.add_yggdrasil(&server, &identifier, &password).await
}

/// Launch credentials (refreshes tokens). Rust callers may use `AccountManager::session` directly.
#[tauri::command]
pub async fn accounts_get_session(app: AppHandle, id: String) -> R<AuthSession> {
    mgr(&app)?.session(&id).await
}

/// Skin PNG as a `data:` URL, or null.
#[tauri::command]
pub async fn accounts_get_skin(app: AppHandle, id: String) -> R<Option<String>> {
    mgr(&app)?.skin(&id).await
}

/// Latest authlib-injector artifact: `[downloadUrl, sha256|null]`. The download agent saves it.
#[tauri::command]
pub async fn accounts_authlib_injector_info() -> R<(String, Option<String>)> {
    yggdrasil::latest_injector(&super::microsoft::http(), yggdrasil::AUTHLIB_INJECTOR_LATEST_META).await
}
