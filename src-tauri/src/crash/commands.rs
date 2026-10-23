use super::collect::{candidate_paths, collect};
use super::{CrashAnalysis, analyze_to_result};
use crate::instances::scan::load_instance;
use crate::settings::commands::global;
use std::fs;
use std::path::{Path, PathBuf};

fn finish(c: super::collect::Collected, manual: bool) -> CrashAnalysis {
    let mut r = analyze_to_result(&c.logs, manual);
    r.files_used = c.files_used;
    r.primary_file = c.primary_file;
    r
}

/// Analyze the logs found under `game_dir` / `root`. Only files written within the last three
/// minutes count (PCL's rule), plus the captured `live_output`.
pub fn analyze_game(game_dir: &Path, root: &Path, live_output: Option<&[String]>) -> CrashAnalysis {
    let paths = candidate_paths(game_dir, root);
    finish(collect(&paths, true, live_output), false)
}

/// Analyze the logs of an instance after a crash. `live_output` is the tail of
/// the game's stdout/stderr captured by the launcher.
#[tauri::command]
pub fn crash_analyze_instance(root: String, name: String, live_output: Option<Vec<String>>) -> Result<CrashAnalysis, String> {
    let g = global().get();
    if !g.game.folders.iter().any(|f| f.path == root) {
        return Err(format!("未登记的 Minecraft 文件夹：{root}"));
    }
    crate::instances::detect::validate_name(&name)?;
    let info = load_instance(Path::new(&root), &name, g.game.isolation);
    Ok(analyze_game(Path::new(&info.game_dir), Path::new(&root), live_output.as_deref()))
}

/// Analyze a log file the user picked (`.log`, `.txt`, hs_err, crash report).
/// Zip archives are not unpacked in this port yet.
#[tauri::command]
pub fn crash_analyze_file(path: String) -> Result<CrashAnalysis, String> {
    let p = PathBuf::from(&path);
    if !p.is_file() {
        return Err(format!("文件不存在：{path}"));
    }
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
    // An arbitrary user-picked file: feed it in under a name the classifier understands.
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    if ext != "log" && ext != "txt" {
        return Err("仅支持 .log 与 .txt 文件".into());
    }
    // Same limit as the automatic collector; a multi-gigabyte "log" must not be read into memory.
    let len = fs::metadata(&p).map_err(|e| e.to_string())?.len();
    if len > super::collect::MAX_FILE_BYTES {
        return Err(format!("文件过大（{} MB），最大支持 {} MB", len >> 20, super::collect::MAX_FILE_BYTES >> 20));
    }
    let bytes = fs::read(&p).map_err(|e| e.to_string())?;
    let lines: Vec<String> = String::from_utf8_lossy(&bytes).lines().map(String::from).collect();
    let c = if name.starts_with("hs_err") || name.starts_with("crash-") || name == "latest.log" || name == "debug.log" {
        collect(std::slice::from_ref(&p), false, None)
    } else {
        collect(&[], false, Some(&lines))
    };
    let mut r = finish(c, true);
    if r.primary_file.is_none() {
        r.primary_file = Some(path);
    }
    Ok(r)
}

/// Analyze pasted text directly.
#[tauri::command]
pub fn crash_analyze_text(text: String) -> CrashAnalysis {
    let lines: Vec<String> = text.lines().map(String::from).collect();
    finish(collect(&[], false, Some(&lines)), true)
}
