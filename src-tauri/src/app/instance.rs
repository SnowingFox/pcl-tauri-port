//! Resolve the UI's `instanceId` into a validated instance on disk.

use super::error::{AppError, AppResult};
use crate::instances::detect::{InstanceInfo, validate_name};
use crate::instances::scan::load_instance;
use crate::settings::{GlobalSettings, InstanceSettings};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ResolvedInstance {
    /// Registered `.minecraft` folder that holds `versions/<name>`.
    pub root: PathBuf,
    pub name: String,
    pub version_dir: PathBuf,
    /// Isolation-aware working directory (mods/, saves/, ...).
    pub game_dir: PathBuf,
    pub info: InstanceInfo,
    pub settings: InstanceSettings,
}

pub fn same_path(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// The registered form of `root`, or `FORBIDDEN_PATH` when it is not a registered folder.
pub fn known_root(g: &GlobalSettings, root: &Path) -> AppResult<PathBuf> {
    g.game.folders.iter().map(|f| PathBuf::from(&f.path)).find(|f| same_path(f, root)).ok_or_else(|| AppError::forbidden(root))
}

/// `instance_id` is the instance folder (`<root>/versions/<name>`); a bare name is looked up in
/// the selected folder.
pub fn resolve(g: &GlobalSettings, instance_id: &str) -> AppResult<ResolvedInstance> {
    let not_found = || {
        AppError::new("INSTANCE_NOT_FOUND", format!("找不到游戏实例：{instance_id}")).with(serde_json::json!({ "instanceId": instance_id }))
    };
    let (root, name) = if instance_id.contains('/') || instance_id.contains('\\') {
        let p = Path::new(instance_id);
        if !p.is_absolute() {
            return Err(AppError::invalid("instanceId 必须是绝对路径或实例名"));
        }
        let name = p.file_name().and_then(|n| n.to_str()).ok_or_else(not_found)?.to_string();
        let versions = p.parent().ok_or_else(not_found)?;
        if versions.file_name().and_then(|n| n.to_str()) != Some("versions") {
            return Err(AppError::forbidden(p));
        }
        let root = versions.parent().ok_or_else(not_found)?;
        (known_root(g, root)?, name)
    } else {
        let sel = super::config::selected_folder(g);
        (known_root(g, &sel)?, instance_id.to_string())
    };
    validate_name(&name).map_err(AppError::invalid)?;
    let version_dir = root.join("versions").join(&name);
    if !version_dir.is_dir() {
        return Err(not_found());
    }
    let info = load_instance(&root, &name, g.game.isolation);
    if let Some(e) = &info.error {
        return Err(AppError::new("INSTANCE_INVALID", format!("实例「{name}」无法使用：{e}"))
            .with(serde_json::json!({ "instanceId": instance_id })));
    }
    let settings = InstanceSettings::load(&version_dir);
    let game_dir = PathBuf::from(&info.game_dir);
    Ok(ResolvedInstance { root, name, version_dir, game_dir, info, settings })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::McFolderEntry;

    fn settings_for(root: &Path) -> GlobalSettings {
        let mut g = GlobalSettings::default();
        g.game.folders = vec![McFolderEntry { name: "t".into(), path: root.to_string_lossy().into_owned() }];
        g.game.selected_folder = Some(root.to_string_lossy().into_owned());
        g
    }

    fn write_version(root: &Path, name: &str) {
        let d = root.join("versions").join(name);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(
            d.join(format!("{name}.json")),
            r#"{"id":"1.20.1","type":"release","mainClass":"net.minecraft.client.main.Main","libraries":[]}"#,
        )
        .unwrap();
    }

    #[test]
    fn resolves_by_path_and_by_name() {
        let d = tempfile::tempdir().unwrap();
        write_version(d.path(), "1.20.1");
        let g = settings_for(d.path());
        let by_path = resolve(&g, &d.path().join("versions/1.20.1").to_string_lossy()).unwrap();
        assert_eq!(by_path.name, "1.20.1");
        assert!(by_path.game_dir.starts_with(d.path()));
        let by_name = resolve(&g, "1.20.1").unwrap();
        assert_eq!(by_name.version_dir, by_path.version_dir);
    }

    #[test]
    fn rejects_unknown_roots_and_missing_instances() {
        let d = tempfile::tempdir().unwrap();
        write_version(d.path(), "1.20.1");
        let other = tempfile::tempdir().unwrap();
        write_version(other.path(), "x");
        let g = settings_for(d.path());
        let e = resolve(&g, &other.path().join("versions/x").to_string_lossy()).unwrap_err();
        assert_eq!(e.code, "FORBIDDEN_PATH");
        assert_eq!(resolve(&g, "nope").unwrap_err().code, "INSTANCE_NOT_FOUND");
        assert_eq!(resolve(&g, "../evil").unwrap_err().code, "INVALID_ARGUMENT", "relative paths are refused");
        assert_eq!(resolve(&g, "a b/versions/x").unwrap_err().code, "INVALID_ARGUMENT");
        assert_eq!(resolve(&g, "..").unwrap_err().code, "INVALID_ARGUMENT");
        assert_eq!(resolve(&g, &d.path().join("mods/x").to_string_lossy()).unwrap_err().code, "FORBIDDEN_PATH");
    }
}
