//! Tauri commands for instance management.

use super::detect::InstanceInfo;
use super::ops::{self, ExportedInfo};
use super::scan;
use crate::settings::commands::global;
use crate::settings::{IsolationMode, McFolderEntry};
use std::path::{Path, PathBuf};

fn iso() -> IsolationMode {
    global().get().game.isolation
}

/// Only folders the user registered in settings are accepted as roots, so the
/// frontend cannot point these commands at arbitrary directories.
fn known_root(root: &str) -> Result<PathBuf, String> {
    let g = global().get();
    let p = Path::new(root);
    if g.game.folders.iter().any(|f| Path::new(&f.path) == p) {
        Ok(p.to_path_buf())
    } else {
        Err(format!("未登记的 Minecraft 文件夹：{root}"))
    }
}

#[tauri::command]
pub fn mc_folders_list() -> Vec<McFolderEntry> {
    global().get().game.folders
}

#[tauri::command]
pub fn mc_folder_add(name: String, path: String) -> Result<Vec<McFolderEntry>, String> {
    let p = Path::new(&path);
    if !p.is_absolute() {
        return Err("需要绝对路径".into());
    }
    std::fs::create_dir_all(p.join("versions")).map_err(|e| format!("无法访问该文件夹：{e}"))?;
    let name = if name.trim().is_empty() {
        p.file_name().and_then(|n| n.to_str()).unwrap_or("Minecraft").to_string()
    } else {
        name
    };
    global()
        .update(|g| {
            if let Some(f) = g.game.folders.iter_mut().find(|f| f.path == path) {
                f.name = name.clone();
            } else {
                g.game.folders.push(McFolderEntry { name: name.clone(), path: path.clone() });
            }
        })
        .map(|g| g.game.folders)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn mc_folder_remove(path: String) -> Result<Vec<McFolderEntry>, String> {
    // Only unregisters; files on disk are never touched.
    global()
        .update(|g| g.game.folders.retain(|f| f.path != path))
        .map(|g| g.game.folders)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn mc_folder_select(path: String) -> Result<(), String> {
    known_root(&path)?;
    global()
        .update(|g| {
            g.game.selected_folder = Some(path);
            g.game.selected_instance = None;
        })
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn instances_scan(root: String) -> Result<Vec<InstanceInfo>, String> {
    let root = known_root(&root)?;
    Ok(scan::scan_folder(&root, iso()))
}

#[tauri::command]
pub fn instance_get(root: String, name: String) -> Result<InstanceInfo, String> {
    let root = known_root(&root)?;
    super::detect::validate_name(&name)?;
    Ok(scan::load_instance(&root, &name, iso()))
}

#[tauri::command]
pub fn instance_select(root: String, name: String) -> Result<(), String> {
    known_root(&root)?;
    global()
        .update(|g| {
            g.game.selected_folder = Some(root);
            g.game.selected_instance = Some(name);
        })
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn instance_rename(root: String, name: String, new_name: String) -> Result<InstanceInfo, String> {
    let root = known_root(&root)?;
    let info = ops::rename_instance(&root, &name, &new_name, iso())?;
    let _ = global().update(|g| {
        if g.game.selected_instance.as_deref() == Some(name.as_str()) {
            g.game.selected_instance = Some(new_name.clone());
        }
    });
    Ok(info)
}

#[tauri::command]
pub fn instance_duplicate(root: String, name: String, new_name: String) -> Result<InstanceInfo, String> {
    let root = known_root(&root)?;
    ops::duplicate_instance(&root, &name, &new_name, iso())
}

/// Moves to the Trash unless `permanent` is true. The frontend must confirm first.
#[tauri::command]
pub fn instance_delete(root: String, name: String, permanent: bool) -> Result<String, String> {
    let root = known_root(&root)?;
    let out = ops::delete_instance(&root, &name, permanent)?;
    let _ = global().update(|g| {
        if g.game.selected_instance.as_deref() == Some(name.as_str()) {
            g.game.selected_instance = None;
        }
    });
    Ok(out)
}

/// `target`: "instance" (version folder, selected in Finder), "game" (game dir), "mods", "saves", "logs".
#[tauri::command]
pub fn instance_open_in_finder(root: String, name: String, target: String) -> Result<(), String> {
    let root = known_root(&root)?;
    let info = scan::load_instance(&root, &name, iso());
    super::detect::validate_name(&name)?;
    let game = PathBuf::from(&info.game_dir);
    let (path, select) = match target.as_str() {
        "instance" => (PathBuf::from(&info.path), true),
        "game" => (game, false),
        "mods" => (game.join("mods"), false),
        "saves" => (game.join("saves"), false),
        "logs" => (game.join("logs"), false),
        other => return Err(format!("未知的目标：{other}")),
    };
    if !select {
        let _ = std::fs::create_dir_all(&path);
    }
    ops::reveal(&path, select)
}

#[tauri::command]
pub fn instance_export_info(root: String, name: String) -> Result<ExportedInfo, String> {
    let root = known_root(&root)?;
    ops::export_info(&root, &name, iso())
}
