//! Tauri commands for global and per-instance settings.

use super::guard;
use super::instance_settings::InstanceSettings;
use super::model::GlobalSettings;
use super::store::SettingsStore;
use serde_json::Value;
use std::path::Path;
use std::sync::OnceLock;

fn store() -> &'static SettingsStore {
    static STORE: OnceLock<SettingsStore> = OnceLock::new();
    STORE.get_or_init(|| SettingsStore::load(SettingsStore::default_path()))
}

/// Shared handle for other modules (launch, download) that need to read settings.
pub fn global() -> &'static SettingsStore {
    store()
}

#[tauri::command]
pub fn settings_get() -> GlobalSettings {
    store().get()
}

/// Reject JVM arguments that load agents or run commands (`-javaagent`, `-XX:OnError=...`).
/// The string is split on whitespace like the launcher does.
pub(crate) fn check_jvm_args(args: &str) -> Result<(), String> {
    match args.split_whitespace().find(|a| guard::jvm_arg_is_dangerous(a)) {
        Some(a) => Err(format!("不允许的 JVM 参数：{a}")),
        None => Ok(()),
    }
}

/// Settings that decide what the launcher registers, executes or loads are checked when they
/// change (existing values are left alone so old files keep loading):
/// - new Minecraft folders pass the same checks as `mc_folder_add` (they are the trust anchor
///   for every file operation),
/// - Java paths must be a real `java`,
/// - JVM arguments must not load agents or run commands.
pub(crate) fn check_settings_change(old: &GlobalSettings, next: &GlobalSettings) -> Result<(), String> {
    for f in &next.game.folders {
        if !old.game.folders.iter().any(|o| o.path == f.path) {
            guard::validate_new_root(Path::new(&f.path))?;
        }
    }
    if next.java.selected != old.java.selected
        && let Some(j) = next.java.selected.as_deref().filter(|j| !j.is_empty())
    {
        crate::java::check_java_path(j)?;
    }
    for j in next.java.custom_paths.iter().filter(|j| !old.java.custom_paths.contains(j)) {
        crate::java::check_java_path(j)?;
    }
    if next.game.jvm_args != old.game.jvm_args {
        check_jvm_args(&next.game.jvm_args)?;
    }
    Ok(())
}

#[tauri::command]
pub fn settings_set(settings: GlobalSettings) -> Result<GlobalSettings, String> {
    check_settings_change(&store().get(), &settings)?;
    store().set(settings).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn settings_patch(patch: Value) -> Result<GlobalSettings, String> {
    let cur = store().get();
    let mut doc = serde_json::to_value(&cur).map_err(|e| e.to_string())?;
    super::store::merge(&mut doc, patch);
    let next: GlobalSettings = serde_json::from_value(doc).map_err(|e| e.to_string())?;
    check_settings_change(&cur, &next)?;
    store().set(next).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn settings_reset() -> Result<GlobalSettings, String> {
    store().reset().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn settings_path() -> String {
    store().path().to_string_lossy().into_owned()
}

/// `instance_dir` must be `<registered root>/versions/<name>`; anything else could read or
/// create `PCL/config.json` in an arbitrary folder.
fn checked_instance_dir(dir: &str) -> Result<std::path::PathBuf, String> {
    guard::confine_instance_dir(&guard::registered_roots(), Path::new(dir))
}

#[tauri::command]
pub fn instance_settings_get(instance_dir: String) -> Result<InstanceSettings, String> {
    Ok(InstanceSettings::load(&checked_instance_dir(&instance_dir)?))
}

#[tauri::command]
pub fn instance_settings_set(instance_dir: String, settings: InstanceSettings) -> Result<(), String> {
    let dir = checked_instance_dir(&instance_dir)?;
    check_jvm_args(&settings.jvm_args)?;
    if let Some(j) = settings.java_path.as_deref().filter(|j| !j.is_empty()) {
        crate::java::check_java_path(j)?;
    }
    settings.save(&dir).map_err(|e| e.to_string())
}

#[cfg(test)]
mod guard_tests {
    use super::*;
    use crate::settings::McFolderEntry;

    #[test]
    fn settings_changes_are_checked() {
        let old = GlobalSettings::default();
        let mut next = old.clone();
        assert!(check_settings_change(&old, &next).is_ok(), "no-op change");

        // A new Minecraft folder is the trust anchor for file access: refuse system and home folders.
        for bad in ["/", "/etc", "/usr/local", "/System/Library", "/Users", "relative/dir"] {
            let mut n = old.clone();
            n.game.folders.push(McFolderEntry { name: "x".into(), path: bad.into() });
            assert!(check_settings_change(&old, &n).is_err(), "{bad}");
        }
        if let Some(home) = std::env::var_os("HOME") {
            let home = std::path::PathBuf::from(home);
            for bad in [home.clone(), home.join(".ssh"), home.join("Library"), home.join("Documents")] {
                let mut n = old.clone();
                n.game.folders.push(McFolderEntry { name: "x".into(), path: bad.display().to_string() });
                assert!(check_settings_change(&old, &n).is_err(), "{}", bad.display());
            }
        }

        // Java must be a real `java`; JVM arguments must not load agents.
        next.java.selected = Some("/bin/sh".into());
        assert!(check_settings_change(&old, &next).is_err());
        next.java.selected = None;
        next.java.custom_paths = vec!["/usr/bin/open".into()];
        assert!(check_settings_change(&old, &next).is_err());
        next.java.custom_paths.clear();
        next.game.jvm_args = "-Xmx4G -javaagent:/tmp/x.jar".into();
        assert!(check_settings_change(&old, &next).is_err());
        next.game.jvm_args = "-Xmx4G -XX:+UseG1GC".into();
        assert!(check_settings_change(&old, &next).is_ok());

        // Values that were already stored keep loading: only changes are checked.
        let mut stored = old.clone();
        stored.game.jvm_args = "-javaagent:/legacy.jar".into();
        assert!(check_settings_change(&stored, &stored.clone()).is_ok());
    }

    #[test]
    fn instance_settings_reject_agents() {
        assert!(check_jvm_args("").is_ok());
        assert!(check_jvm_args("-Xmx2G -XX:+UseG1GC").is_ok());
        assert!(check_jvm_args("-Xmx2G  -agentlib:jdwp=transport=dt_socket,server=y,address=5005").is_err());
        assert!(check_jvm_args("-XX:OnError=sh").is_err());
    }
}
