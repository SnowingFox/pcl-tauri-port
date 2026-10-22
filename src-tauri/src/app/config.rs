//! Settings <-> download-engine config wiring.
//!
//! The download config is derived from `GlobalSettings` (selected Minecraft folder and the
//! download section), applied on startup and whenever settings change. `download_set_config`
//! writes back into settings so the choice survives a restart.

use crate::download::mirror::Source;
use crate::download::{DownloadConfig, Manager};
use crate::settings::commands::global;
use crate::settings::store::SettingsStore;
use crate::settings::{DownloadSource, GlobalSettings, McFolderEntry};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub fn source_from_settings(s: DownloadSource) -> Source {
    match s {
        DownloadSource::Official => Source::Official,
        DownloadSource::MirrorFirst => Source::Bmcl,
        DownloadSource::OfficialFirst => Source::OfficialFirst,
    }
}

pub fn source_to_settings(s: Source) -> DownloadSource {
    match s {
        Source::Official => DownloadSource::Official,
        Source::Bmcl => DownloadSource::MirrorFirst,
        Source::OfficialFirst => DownloadSource::OfficialFirst,
    }
}

/// The folder game files are installed into.
pub fn selected_folder(g: &GlobalSettings) -> PathBuf {
    g.game
        .selected_folder
        .clone()
        .or_else(|| g.game.folders.first().map(|f| f.path.clone()))
        .map(PathBuf::from)
        .unwrap_or_else(crate::settings::store::default_minecraft_dir)
}

pub fn download_config_from(g: &GlobalSettings) -> DownloadConfig {
    DownloadConfig {
        game_dir: selected_folder(g),
        // Game, loader, library and asset downloads follow the "version" source setting.
        source: source_from_settings(g.download.version_source),
        concurrency: g.download.thread_count as usize,
        segments: g.download.segments as usize,
        retries: g.download.retries,
    }
}

/// Push the settings into the running manager (affects tasks started afterwards).
pub fn apply(mgr: &Manager, g: &GlobalSettings) {
    let cfg = download_config_from(g);
    tracing::info!(game_dir = %cfg.game_dir.display(), source = ?cfg.source, concurrency = cfg.concurrency, "download config applied");
    *mgr.config.lock().unwrap() = cfg;
}

/// Startup: apply once, then follow every settings change.
pub fn wire(mgr: Arc<Manager>) {
    wire_to(global(), mgr);
}

pub fn wire_to(store: &SettingsStore, mgr: Arc<Manager>) {
    apply(&mgr, &store.get());
    store.on_change(move |g| apply(&mgr, g));
}

/// Store an explicit `DownloadConfig` (from `download_set_config`) in the settings.
pub fn persist_download_config(cfg: &DownloadConfig) {
    persist_to(global(), cfg);
}

pub fn persist_to(store: &SettingsStore, cfg: &DownloadConfig) {
    let path = cfg.game_dir.to_string_lossy().into_owned();
    let res = store.update(|g| {
        let src = source_to_settings(cfg.source);
        g.download.version_source = src;
        g.download.thread_count = cfg.concurrency.clamp(1, 256) as u32;
        g.download.segments = cfg.segments.clamp(1, 16) as u32;
        g.download.retries = cfg.retries.clamp(1, 10);
        if !path.is_empty() && Path::new(&path).is_absolute() {
            if !g.game.folders.iter().any(|f| f.path == path) {
                let name = cfg.game_dir.file_name().and_then(|n| n.to_str()).unwrap_or("Minecraft").to_string();
                g.game.folders.push(McFolderEntry { name, path: path.clone() });
            }
            if g.game.selected_folder.as_deref() != Some(path.as_str()) {
                g.game.selected_folder = Some(path.clone());
                g.game.selected_instance = None;
            }
        }
    });
    if let Err(e) = res {
        tracing::warn!("could not persist download config: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_settings_to_config_and_back() {
        let mut g = GlobalSettings::default();
        g.download.version_source = DownloadSource::OfficialFirst;
        g.download.thread_count = 32;
        g.download.segments = 6;
        g.download.retries = 5;
        g.game.folders = vec![McFolderEntry { name: "x".into(), path: "/tmp/mc".into() }];
        g.game.selected_folder = Some("/tmp/mc".into());
        let c = download_config_from(&g);
        assert_eq!(c.game_dir, PathBuf::from("/tmp/mc"));
        assert_eq!(c.source, Source::OfficialFirst);
        assert_eq!((c.concurrency, c.segments, c.retries), (32, 6, 5));
        for s in [DownloadSource::Official, DownloadSource::MirrorFirst, DownloadSource::OfficialFirst] {
            assert_eq!(source_to_settings(source_from_settings(s)), s);
        }
    }

    #[test]
    fn startup_wiring_and_persistence_round_trip() {
        let d = tempfile::tempdir().unwrap();
        let file = d.path().join("settings.json");
        let store = SettingsStore::load(file.clone());
        let mgr = Manager::new(Arc::new(|_| {}), Arc::new(|_| {}));
        wire_to(&store, mgr.clone());
        // startup applied the defaults
        assert_eq!(mgr.cfg().game_dir, selected_folder(&store.get()));
        // a settings change reaches the manager
        store.patch(serde_json::json!({"download": {"version_source": "official", "thread_count": 8}})).unwrap();
        assert_eq!((mgr.cfg().source, mgr.cfg().concurrency), (Source::Official, 8));
        // download_set_config persists and survives a "restart"
        let new_dir = d.path().join("mc2");
        persist_to(
            &store,
            &DownloadConfig { game_dir: new_dir.clone(), source: Source::OfficialFirst, concurrency: 5, segments: 2, retries: 4 },
        );
        let again = SettingsStore::load(file).get();
        assert_eq!(again.download.version_source, DownloadSource::OfficialFirst);
        assert_eq!((again.download.thread_count, again.download.segments, again.download.retries), (5, 2, 4));
        assert_eq!(again.game.selected_folder.as_deref(), new_dir.to_str());
        assert!(again.game.folders.iter().any(|f| f.path == new_dir.to_string_lossy()));
        let mgr2 = Manager::new(Arc::new(|_| {}), Arc::new(|_| {}));
        wire_to(&SettingsStore::load(d.path().join("settings.json")), mgr2.clone());
        assert_eq!((mgr2.cfg().game_dir, mgr2.cfg().source), (new_dir, Source::OfficialFirst));
        // the manager follows later changes too (not only at startup)
        store.patch(serde_json::json!({"download": {"version_source": "mirror_first"}})).unwrap();
        assert_eq!(mgr.cfg().source, Source::Bmcl);
    }

    #[test]
    fn settings_listener_fires_on_set() {
        let d = tempfile::tempdir().unwrap();
        let store = crate::settings::store::SettingsStore::load(d.path().join("settings.json"));
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let s2 = seen.clone();
        store.on_change(move |g| s2.lock().unwrap().push(g.download.thread_count));
        store.update(|g| g.download.thread_count = 12).unwrap();
        store.patch(serde_json::json!({"download": {"thread_count": 20}})).unwrap();
        assert_eq!(*seen.lock().unwrap(), vec![12, 20]);
    }
}
