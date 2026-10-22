use super::model::{CURRENT_SCHEMA_VERSION, GlobalSettings};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::fmt;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug)]
pub enum SettingsError {
    Io(String),
    Parse(String),
    Invalid(String),
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SettingsError::Io(s) => write!(f, "io error: {s}"),
            SettingsError::Parse(s) => write!(f, "parse error: {s}"),
            SettingsError::Invalid(s) => write!(f, "invalid value: {s}"),
        }
    }
}

impl std::error::Error for SettingsError {}

impl From<std::io::Error> for SettingsError {
    fn from(e: std::io::Error) -> Self {
        SettingsError::Io(e.to_string())
    }
}

impl From<serde_json::Error> for SettingsError {
    fn from(e: serde_json::Error) -> Self {
        SettingsError::Parse(e.to_string())
    }
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

/// Bundle identifier (must match `tauri.conf.json`); names the app data directory.
pub const APP_IDENTIFIER: &str = "com.snowingfox.pcltauriport";

/// App data directory. `PCL_DATA_DIR` overrides (used by tests and portable setups).
/// macOS: `~/Library/Application Support/<identifier>`.
pub fn data_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("PCL_DATA_DIR") {
        return PathBuf::from(p);
    }
    if cfg!(target_os = "macos") {
        home_dir().join("Library/Application Support").join(APP_IDENTIFIER)
    } else if let Some(x) = std::env::var_os("XDG_DATA_HOME") {
        PathBuf::from(x).join(APP_IDENTIFIER)
    } else {
        home_dir().join(".local/share").join(APP_IDENTIFIER)
    }
}

/// Data directory used before the identifier-based layout (migrated by `app::paths`).
pub fn legacy_data_dir() -> PathBuf {
    if cfg!(target_os = "macos") {
        home_dir().join("Library/Application Support/PCL-Tauri")
    } else if let Some(x) = std::env::var_os("XDG_DATA_HOME") {
        PathBuf::from(x).join("PCL-Tauri")
    } else {
        home_dir().join(".local/share/PCL-Tauri")
    }
}

/// The official macOS Minecraft folder.
pub fn default_minecraft_dir() -> PathBuf {
    if cfg!(target_os = "macos") { home_dir().join("Library/Application Support/minecraft") } else { home_dir().join(".minecraft") }
}

/// Write JSON atomically: temp file in the same dir, fsync, rename over target.
pub fn atomic_write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), SettingsError> {
    let parent = path.parent().ok_or_else(|| SettingsError::Invalid("path has no parent".into()))?;
    fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(".{}.{}.tmp", path.file_name().and_then(|n| n.to_str()).unwrap_or("settings"), std::process::id()));
    let bytes = serde_json::to_vec_pretty(value)?;
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
    }
    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(())
}

/// Read a JSON file into `T`. Missing file -> `Ok(None)`.
pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, SettingsError> {
    match fs::read(path) {
        Ok(b) => Ok(Some(serde_json::from_slice(&b)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Upgrade a raw settings document to `CURRENT_SCHEMA_VERSION`.
///
/// v0 was the flat prototype layout (`java_path`, `memory_mb`, `mc_folders`,
/// `download_source`); v1 is the nested layout in `model.rs`.
pub fn migrate(mut doc: Value) -> Value {
    let version = doc.get("schema_version").and_then(Value::as_u64).unwrap_or(0) as u32;
    if version < 1 {
        let mut out = json!({ "schema_version": 1 });
        if let Some(obj) = doc.as_object_mut() {
            if let Some(Value::String(p)) = obj.remove("java_path")
                && !p.is_empty()
            {
                out["java"] = json!({ "selected": p });
            }
            if let Some(mb) = obj.remove("memory_mb").and_then(|v| v.as_u64()) {
                out["memory"] = json!({ "mode": "custom", "custom_mb": mb });
            }
            if let Some(Value::Array(folders)) = obj.remove("mc_folders") {
                let list: Vec<Value> = folders.into_iter().filter_map(|f| f.as_str().map(|s| json!({ "name": s, "path": s }))).collect();
                out["game"] = json!({ "folders": list });
            }
            if let Some(Value::String(s)) = obj.remove("download_source") {
                let v = match s.as_str() {
                    "official" => "official",
                    "mirror" => "mirror_first",
                    _ => "mirror_first",
                };
                out["download"] = json!({ "source": v });
            }
            // Anything else that already matches the v1 keys is carried over.
            for key in ["ui", "window", "launcher"] {
                if let Some(v) = obj.remove(key) {
                    out[key] = v;
                }
            }
        }
        doc = out;
    }
    // Future: if version < 2 { ... }
    doc
}

type ChangeListener = Box<dyn Fn(&GlobalSettings) + Send + Sync>;

/// Thread-safe settings store backed by one JSON file.
pub struct SettingsStore {
    path: PathBuf,
    inner: Mutex<GlobalSettings>,
    listeners: Mutex<Vec<ChangeListener>>,
}

impl SettingsStore {
    pub fn default_path() -> PathBuf {
        data_dir().join("settings.json")
    }

    /// Load from `path`. Missing -> defaults. Corrupt -> file is moved to
    /// `settings.json.corrupt` and defaults are used, so launch never fails.
    pub fn load(path: PathBuf) -> Self {
        let mut settings = match fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<Value>(&bytes).and_then(|v| serde_json::from_value::<GlobalSettings>(migrate(v))) {
                Ok(s) => s,
                Err(_) => {
                    let _ = fs::rename(&path, path.with_extension("json.corrupt"));
                    GlobalSettings::default()
                }
            },
            Err(_) => GlobalSettings::default(),
        };
        settings.sanitize();
        Self { path, inner: Mutex::new(settings), listeners: Mutex::new(Vec::new()) }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn get(&self) -> GlobalSettings {
        self.inner.lock().unwrap().clone()
    }

    /// Replace everything, sanitize, persist.
    pub fn set(&self, mut new: GlobalSettings) -> Result<GlobalSettings, SettingsError> {
        new.sanitize();
        atomic_write_json(&self.path, &new)?;
        *self.inner.lock().unwrap() = new.clone();
        for l in self.listeners.lock().unwrap().iter() {
            l(&new);
        }
        Ok(new)
    }

    /// Register a callback invoked (synchronously) after every successful `set`/`update`/`patch`/`reset`.
    /// Callbacks must not call back into `on_change`.
    pub fn on_change(&self, f: impl Fn(&GlobalSettings) + Send + Sync + 'static) {
        self.listeners.lock().unwrap().push(Box::new(f));
    }

    /// Mutate in place, sanitize, persist. Memory state is unchanged if the write fails.
    pub fn update<F: FnOnce(&mut GlobalSettings)>(&self, f: F) -> Result<GlobalSettings, SettingsError> {
        let mut next = self.get();
        f(&mut next);
        self.set(next)
    }

    /// Merge a partial JSON patch (objects merge recursively, everything else replaces).
    /// The command layer merges and checks the result itself (`settings_patch`); kept for tests.
    #[allow(dead_code)]
    pub fn patch(&self, patch: Value) -> Result<GlobalSettings, SettingsError> {
        let mut doc = serde_json::to_value(self.get())?;
        merge(&mut doc, patch);
        let next: GlobalSettings = serde_json::from_value(doc).map_err(|e| SettingsError::Invalid(e.to_string()))?;
        self.set(next)
    }

    pub fn reset(&self) -> Result<GlobalSettings, SettingsError> {
        self.set(GlobalSettings::default())
    }
}

pub(crate) fn merge(base: &mut Value, patch: Value) {
    match (base, patch) {
        (Value::Object(b), Value::Object(p)) => {
            for (k, v) in p {
                merge(b.entry(k).or_insert(Value::Null), v);
            }
        }
        (b, p) => *b = p,
    }
}

#[allow(dead_code)]
const _: u32 = CURRENT_SCHEMA_VERSION;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::model::*;

    fn tmp_store() -> (tempfile::TempDir, SettingsStore) {
        let d = tempfile::tempdir().unwrap();
        let s = SettingsStore::load(d.path().join("settings.json"));
        (d, s)
    }

    #[test]
    fn defaults_seed_a_folder() {
        let (_d, s) = tmp_store();
        let g = s.get();
        assert_eq!(g.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(g.game.folders.len(), 1);
        assert_eq!(g.game.selected_folder.as_deref(), Some(g.game.folders[0].path.as_str()));
    }

    #[test]
    fn round_trip() {
        let (d, s) = tmp_store();
        s.update(|g| {
            g.memory.mode = MemoryMode::Custom;
            g.memory.custom_mb = 6144;
            g.java.selected = Some("/usr/bin/java".into());
            g.ui.theme_mode = ThemeMode::Dark;
        })
        .unwrap();
        let again = SettingsStore::load(d.path().join("settings.json")).get();
        assert_eq!(again.memory.custom_mb, 6144);
        assert_eq!(again.memory.mode, MemoryMode::Custom);
        assert_eq!(again.java.selected.as_deref(), Some("/usr/bin/java"));
        assert_eq!(again.ui.theme_mode, ThemeMode::Dark);
    }

    #[test]
    fn migrates_v0_flat_layout() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("settings.json");
        fs::write(&p, r#"{"java_path":"/opt/java/bin/java","memory_mb":8192,"mc_folders":["/games/mc"],"download_source":"official"}"#)
            .unwrap();
        let g = SettingsStore::load(p).get();
        assert_eq!(g.schema_version, 1);
        assert_eq!(g.java.selected.as_deref(), Some("/opt/java/bin/java"));
        assert_eq!(g.memory.mode, MemoryMode::Custom);
        assert_eq!(g.memory.custom_mb, 8192);
        assert_eq!(g.game.folders[0].path, "/games/mc");
        assert_eq!(g.download.source, DownloadSource::Official);
    }

    #[test]
    fn unknown_and_missing_fields_are_tolerated() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("settings.json");
        fs::write(&p, r#"{"schema_version":1,"future_field":1,"ui":{"hue":10}}"#).unwrap();
        let g = SettingsStore::load(p).get();
        assert_eq!(g.ui.hue, 10);
        assert_eq!(g.ui.saturation, 80);
    }

    #[test]
    fn corrupt_file_is_backed_up() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("settings.json");
        fs::write(&p, "{not json").unwrap();
        let s = SettingsStore::load(p.clone());
        assert_eq!(s.get().memory.mode, MemoryMode::Auto);
        assert!(d.path().join("settings.json.corrupt").exists());
    }

    #[test]
    fn sanitize_clamps() {
        let (_d, s) = tmp_store();
        let g = s
            .update(|g| {
                g.memory.custom_mb = 1;
                g.download.thread_count = 9999;
                g.ui.opacity = 5000;
            })
            .unwrap();
        assert_eq!(g.memory.custom_mb, 512);
        assert_eq!(g.download.thread_count, 256);
        assert_eq!(g.ui.opacity, 1000);
    }

    #[test]
    fn patch_merges_and_rejects_bad_types() {
        let (_d, s) = tmp_store();
        let g = s.patch(json!({"ui": {"hue": 42}})).unwrap();
        assert_eq!(g.ui.hue, 42);
        assert_eq!(g.ui.saturation, 80);
        assert!(s.patch(json!({"ui": {"hue": "red"}})).is_err());
        assert_eq!(s.get().ui.hue, 42);
    }

    #[test]
    fn atomic_write_leaves_no_temp_files() {
        let (d, s) = tmp_store();
        s.update(|g| g.launcher.debug_mode = true).unwrap();
        let names: Vec<_> = fs::read_dir(d.path()).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(names, vec!["settings.json".to_string()]);
    }

    #[test]
    fn selected_folder_falls_back_when_invalid() {
        let (_d, s) = tmp_store();
        let g = s
            .update(|g| {
                g.game.folders = vec![McFolderEntry { name: "a".into(), path: "/a".into() }];
                g.game.selected_folder = Some("/nope".into());
            })
            .unwrap();
        assert_eq!(g.game.selected_folder.as_deref(), Some("/a"));
    }
}
