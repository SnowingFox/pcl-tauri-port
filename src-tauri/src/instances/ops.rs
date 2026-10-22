use super::detect::{InstanceInfo, validate_name};
use super::scan::{find_json, load_instance};
use crate::settings::{InstanceSettings, IsolationMode};
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn version_dir(root: &Path, name: &str) -> Result<PathBuf, String> {
    validate_name(name)?;
    let d = root.join("versions").join(name);
    // `symlink_metadata`: a symlinked version folder is neither renamed, copied nor exported.
    match fs::symlink_metadata(&d) {
        Ok(m) if m.is_dir() => Ok(d),
        Ok(_) => Err(format!("{name} 不是普通的版本文件夹")),
        Err(_) => Err(format!("未找到版本 {name}")),
    }
}

fn rewrite_id(json_path: &Path, new_id: &str) -> Result<(), String> {
    let text = fs::read_to_string(json_path).map_err(|e| e.to_string())?;
    let mut v: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
    if let Some(o) = v.as_object_mut() {
        o.insert("id".into(), Value::String(new_id.into()));
    }
    fs::write(json_path, serde_json::to_vec_pretty(&v).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// Rename `versions/<old>` to `versions/<new>`: folder, jar, natives folder and JSON id
/// (PCL `McInstance.Rename`; no wildcard renames, to avoid hitting unrelated files).
pub fn rename_instance(root: &Path, old: &str, new: &str, iso: IsolationMode) -> Result<InstanceInfo, String> {
    let old_dir = version_dir(root, old)?;
    validate_name(new)?;
    if old == new {
        return Err("新旧名称相同".into());
    }
    let new_dir = root.join("versions").join(new);
    if new_dir.exists() {
        return Err(format!("已存在名为 {new} 的版本"));
    }
    let old_json = find_json(&old_dir, old).ok();
    fs::rename(&old_dir, &new_dir).map_err(|e| format!("重命名文件夹失败：{e}"))?;
    let natives_old = new_dir.join(format!("{old}-natives"));
    if natives_old.exists() {
        let _ = fs::rename(&natives_old, new_dir.join(format!("{new}-natives")));
    }
    let jar_old = new_dir.join(format!("{old}.jar"));
    if jar_old.is_file() {
        let _ = fs::rename(&jar_old, new_dir.join(format!("{new}.jar")));
    }
    if let Some(j) = old_json {
        let moved = new_dir.join(j.file_name().unwrap());
        let target = new_dir.join(format!("{new}.json"));
        if moved.is_file() {
            let _ = fs::rename(&moved, &target);
        }
        if target.is_file() {
            rewrite_id(&target, new)?;
        }
    }
    Ok(load_instance(root, new, iso))
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        let to = dst.join(entry.file_name());
        if ft.is_symlink() {
            continue; // never follow links out of the instance
        } else if ft.is_dir() {
            copy_dir(&entry.path(), &to)?;
        } else {
            fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}

/// Copy an instance to a new name. The copy is cleaned up if anything fails.
pub fn duplicate_instance(root: &Path, name: &str, new: &str, iso: IsolationMode) -> Result<InstanceInfo, String> {
    let src = version_dir(root, name)?;
    validate_name(new)?;
    let dst = root.join("versions").join(new);
    if dst.exists() {
        return Err(format!("已存在名为 {new} 的版本"));
    }
    let src_json = find_json(&src, name).ok();
    let result = (|| -> Result<(), String> {
        copy_dir(&src, &dst).map_err(|e| format!("复制失败：{e}"))?;
        let nat = dst.join(format!("{name}-natives"));
        if nat.exists() {
            fs::rename(&nat, dst.join(format!("{new}-natives"))).map_err(|e| e.to_string())?;
        }
        let jar = dst.join(format!("{name}.jar"));
        if jar.is_file() {
            fs::rename(&jar, dst.join(format!("{new}.jar"))).map_err(|e| e.to_string())?;
        }
        if let Some(j) = &src_json {
            let copied = dst.join(j.file_name().unwrap());
            let target = dst.join(format!("{new}.json"));
            if copied != target && copied.is_file() {
                fs::rename(&copied, &target).map_err(|e| e.to_string())?;
            }
            if target.is_file() {
                rewrite_id(&target, new)?;
            }
        }
        // The copy starts un-starred.
        let mut s = InstanceSettings::load(&dst);
        s.starred = false;
        s.save(&dst).map_err(|e| e.to_string())?;
        Ok(())
    })();
    if let Err(e) = result {
        let _ = fs::remove_dir_all(&dst);
        return Err(e);
    }
    Ok(load_instance(root, new, iso))
}

fn trash_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let t = PathBuf::from(home).join(".Trash");
    t.is_dir().then_some(t)
}

/// Delete an instance. By default it is moved to the macOS Trash so the user can
/// recover it; `permanent` removes it for good. Returns where it went.
pub fn delete_instance(root: &Path, name: &str, permanent: bool) -> Result<String, String> {
    let dir = version_dir(root, name)?;
    // Refuse anything that resolves outside <root>/versions (symlinked version folder).
    let canon_dir = dir.canonicalize().map_err(|e| e.to_string())?;
    let canon_versions = root.join("versions").canonicalize().map_err(|e| e.to_string())?;
    if canon_dir.parent() != Some(canon_versions.as_path()) {
        return Err("版本文件夹位置异常，已拒绝删除".into());
    }
    if !permanent {
        let trash = trash_dir().ok_or_else(|| "找不到废纸篓，请改用永久删除".to_string())?;
        let mut target = trash.join(name);
        let mut n = 1;
        while target.exists() {
            n += 1;
            target = trash.join(format!("{name} {n}"));
        }
        fs::rename(&dir, &target).map_err(|e| format!("移入废纸篓失败（可能在不同磁盘上）：{e}"))?;
        return Ok(target.to_string_lossy().into_owned());
    }
    fs::remove_dir_all(&dir).map_err(|e| format!("删除失败：{e}"))?;
    Ok(String::new())
}

/// Reveal a path in Finder (`open -R`) or open a folder (`open`). Args are passed
/// as an array, never through a shell.
pub fn reveal(path: &Path, select: bool) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("路径不存在：{}", path.display()));
    }
    let mut cmd = if cfg!(target_os = "macos") {
        let mut c = Command::new("open");
        if select {
            c.arg("-R");
        }
        c
    } else {
        Command::new("xdg-open")
    };
    cmd.arg(path).spawn().map(|_| ()).map_err(|e| format!("无法打开：{e}"))
}

#[derive(Debug, Serialize)]
pub struct ExportedInfo {
    pub instance: InstanceInfo,
    pub settings: InstanceSettings,
    pub mods: Vec<String>,
    pub resource_packs: Vec<String>,
    pub shader_packs: Vec<String>,
    pub saves: Vec<String>,
    pub total_size_bytes: u64,
    pub exported_at_unix: u64,
}

fn list_names(dir: &Path, only_ext: Option<&[&str]>) -> Vec<String> {
    let Ok(rd) = fs::read_dir(dir) else { return vec![] };
    let mut v: Vec<String> = rd
        .filter_map(|e| e.ok())
        .filter(|e| match only_ext {
            None => true,
            Some(exts) => e
                .path()
                .extension()
                .and_then(|x| x.to_str())
                .map(|x| exts.iter().any(|w| w.eq_ignore_ascii_case(x)))
                .unwrap_or(false),
        })
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| !n.starts_with('.'))
        .collect();
    v.sort();
    v
}

fn dir_size(p: &Path) -> u64 {
    let Ok(rd) = fs::read_dir(p) else { return 0 };
    rd.filter_map(|e| e.ok())
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => dir_size(&e.path()),
            Ok(t) if t.is_file() => e.metadata().map(|m| m.len()).unwrap_or(0),
            _ => 0,
        })
        .sum()
}

/// Structured summary of an instance for bug reports / sharing. No account data is included.
pub fn export_info(root: &Path, name: &str, iso: IsolationMode) -> Result<ExportedInfo, String> {
    let dir = version_dir(root, name)?;
    let info = load_instance(root, name, iso);
    let game = PathBuf::from(&info.game_dir);
    let mut settings = InstanceSettings::load(&dir);
    settings.server_address.clear();
    Ok(ExportedInfo {
        mods: list_names(&game.join("mods"), Some(&["jar", "disabled"])),
        resource_packs: list_names(&game.join("resourcepacks"), None),
        shader_packs: list_names(&game.join("shaderpacks"), None),
        saves: list_names(&game.join("saves"), None),
        total_size_bytes: dir_size(&dir),
        exported_at_unix: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        instance: info,
        settings,
    })
}

#[cfg(test)]
mod tests {
    use super::super::scan::fixtures::*;
    use super::*;

    fn setup() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        write_version(d.path(), "1.20.1", VANILLA);
        fs::create_dir_all(d.path().join("versions/1.20.1/1.20.1-natives")).unwrap();
        d
    }

    #[test]
    fn rename_moves_everything() {
        let d = setup();
        let r = d.path();
        let i = rename_instance(r, "1.20.1", "my-game", IsolationMode::Off).unwrap();
        assert_eq!(i.name, "my-game");
        assert!(r.join("versions/my-game/my-game.jar").is_file());
        assert!(r.join("versions/my-game/my-game-natives").is_dir());
        let json: Value = serde_json::from_str(&fs::read_to_string(r.join("versions/my-game/my-game.json")).unwrap()).unwrap();
        assert_eq!(json["id"], "my-game");
        assert!(!r.join("versions/1.20.1").exists());
    }

    #[test]
    fn rename_rejects_bad_and_existing() {
        let d = setup();
        let r = d.path();
        write_version(r, "other", SNAPSHOT);
        assert!(rename_instance(r, "1.20.1", "other", IsolationMode::Off).is_err());
        assert!(rename_instance(r, "1.20.1", "../evil", IsolationMode::Off).is_err());
        assert!(rename_instance(r, "../x", "y", IsolationMode::Off).is_err());
        assert!(r.join("versions/1.20.1").is_dir());
    }

    #[test]
    fn duplicate_copies_and_unstars() {
        let d = setup();
        let r = d.path();
        InstanceSettings { starred: true, ..Default::default() }.save(&r.join("versions/1.20.1")).unwrap();
        let i = duplicate_instance(r, "1.20.1", "copy", IsolationMode::Off).unwrap();
        assert_eq!(i.name, "copy");
        assert!(r.join("versions/1.20.1/1.20.1.jar").is_file(), "source untouched");
        assert!(r.join("versions/copy/copy.jar").is_file());
        assert!(r.join("versions/copy/copy-natives").is_dir());
        assert!(!i.starred);
        assert!(InstanceSettings::load(&r.join("versions/1.20.1")).starred);
        assert!(duplicate_instance(r, "1.20.1", "copy", IsolationMode::Off).is_err());
    }

    #[test]
    fn permanent_delete() {
        let d = setup();
        let r = d.path();
        delete_instance(r, "1.20.1", true).unwrap();
        assert!(!r.join("versions/1.20.1").exists());
        assert!(delete_instance(r, "1.20.1", true).is_err());
        assert!(delete_instance(r, "../..", true).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_versions_are_not_touched() {
        let d = setup();
        let r = d.path();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret.txt"), "s").unwrap();
        std::os::unix::fs::symlink(outside.path(), r.join("versions/link")).unwrap();
        assert!(rename_instance(r, "link", "moved", IsolationMode::Off).is_err());
        assert!(duplicate_instance(r, "link", "copy", IsolationMode::Off).is_err());
        assert!(export_info(r, "link", IsolationMode::Off).is_err());
        assert!(!r.join("versions/copy").exists() && !r.join("versions/moved").exists());
        assert!(outside.path().join("secret.txt").exists());
        // A symlink inside an instance is skipped by duplicate, not followed.
        std::os::unix::fs::symlink(outside.path(), r.join("versions/1.20.1/escape")).unwrap();
        duplicate_instance(r, "1.20.1", "copy", IsolationMode::Off).unwrap();
        assert!(!r.join("versions/copy/escape").exists());
    }

    #[cfg(unix)]
    #[test]
    fn delete_refuses_symlink_escape() {
        let d = setup();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("keep.txt"), "x").unwrap();
        std::os::unix::fs::symlink(outside.path(), d.path().join("versions/link")).unwrap();
        assert!(delete_instance(d.path(), "link", true).is_err());
        assert!(outside.path().join("keep.txt").exists());
    }

    #[test]
    fn export_lists_content() {
        let d = setup();
        let r = d.path();
        let mods = r.join("versions/1.20.1/mods");
        fs::create_dir_all(&mods).unwrap();
        fs::write(mods.join("a.jar"), b"x").unwrap();
        fs::write(mods.join("notes.txt"), b"x").unwrap();
        let e = export_info(r, "1.20.1", IsolationMode::Off).unwrap();
        assert_eq!(e.mods, vec!["a.jar".to_string()]);
        assert!(e.total_size_bytes > 0);
    }
}
