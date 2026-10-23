//! Orchestration layer: turns UI intents ("launch instance X as account Y", "install Fabric for
//! 1.21", "install this modpack") into calls on the lower modules (accounts, instances, settings,
//! java, launch, download, install, resources, modpack, crash).
//!
//! Contract: `docs/api/app.md`.

pub mod commands;
pub mod config;
pub mod error;
pub mod install;
pub mod instance;
pub mod launch;
pub mod limits;
pub mod logging;
pub mod modpack;
pub mod paths;
pub mod resource;
pub mod saves;
pub mod tasks;
pub mod tuning;

#[cfg(all(test, unix))]
mod tests;

use crate::accounts::manager::AccountManager;
use crate::download::Manager;
use crate::launch::Emit;
use crate::settings::store::SettingsStore;
use error::AppResult;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub type Discover = Arc<dyn Fn(&[std::path::PathBuf]) -> Vec<crate::java::JavaInfo> + Send + Sync>;

/// Everything the orchestration needs, injectable so the pipeline can run without a window.
#[derive(Clone)]
pub struct AppCtx {
    pub mgr: Arc<Manager>,
    pub emit: Emit,
    pub settings: &'static SettingsStore,
    /// App data directory (logs, tools). Defaults to `settings::store::data_dir()`.
    pub data_dir: std::path::PathBuf,
    /// `None` = the process-wide manager (`accounts::commands::manager`).
    pub accounts: Option<&'static AccountManager>,
    /// Java discovery, replaceable in tests.
    pub discover: Discover,
    /// Modpack reports by task id (also pushed as `app://modpack-report`).
    pub reports: Arc<Mutex<HashMap<String, modpack::ModpackReport>>>,
}

impl AppCtx {
    pub fn new(mgr: Arc<Manager>, emit: Emit) -> Self {
        Self {
            mgr,
            emit,
            settings: crate::settings::commands::global(),
            data_dir: crate::settings::store::data_dir(),
            accounts: None,
            discover: Arc::new(crate::java::discover),
            reports: Arc::default(),
        }
    }

    pub fn accounts(&self) -> AppResult<&'static AccountManager> {
        match self.accounts {
            Some(a) => Ok(a),
            None => Ok(crate::accounts::commands::manager()?),
        }
    }
}

/// Run once from the Tauri `setup` hook.
pub fn setup(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::Manager as _;
    // 0. Finder/Dock launches get a 256-file limit; downloads need far more (see limits.rs).
    let fd_limit = limits::raise_open_file_limit();
    // 1. Move data from the pre-identifier directory before anything reads settings.
    let migrated = paths::migrate_legacy();
    let settings = crate::settings::commands::global();
    // 2. Logging + panic hook.
    logging::init(&paths::logs_dir(&crate::settings::store::data_dir()), settings.get().launcher.debug_mode);
    match migrated {
        Ok(0) => {}
        Ok(n) => tracing::info!("migrated {n} file(s) from the legacy data directory"),
        Err(e) => tracing::warn!("legacy data migration failed: {e}"),
    }
    tracing::info!(data_dir = %crate::settings::store::data_dir().display(), "starting");
    match fd_limit {
        Ok((before, after)) => tracing::info!(before, after, "open file limit"),
        Err(e) => tracing::warn!("could not raise the open file limit: {e}"),
    }
    // 3. Download manager, wired to the persisted settings.
    let mgr = crate::download::new_manager(app.clone());
    config::wire(mgr.clone());
    app.manage(mgr.clone());
    // 4. Accounts are loaded lazily by the commands; warm them up so a corrupt store shows in the log.
    if let Err(e) = crate::accounts::commands::manager() {
        tracing::error!("account store could not be opened: {e}");
    }
    app.manage(Arc::new(AppCtx::new(mgr, crate::launch::tauri_emit(app))));
    Ok(())
}
