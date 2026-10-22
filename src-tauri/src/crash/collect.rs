//! Step 1 and 2 of PCL's analyzer: gather candidate log files and turn them into `Logs`.

use super::analyzer::Logs;
use super::text::head_tail_lines;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// A log file only counts for an automatic analysis if it was written this recently.
pub const RECENT_WINDOW: Duration = Duration::from_secs(3 * 60);
pub const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    HsErr,
    CrashReport,
    McLog,
    Extra,
}

#[derive(Debug)]
pub struct Collected {
    pub logs: Logs,
    pub files_used: Vec<String>,
    pub primary_file: Option<String>,
}

fn read_lines(p: &Path) -> Option<Vec<String>> {
    let meta = fs::metadata(p).ok()?;
    if !meta.is_file() || meta.len() == 0 || meta.len() > MAX_FILE_BYTES {
        return None;
    }
    let bytes = fs::read(p).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    Some(text.lines().map(|l| l.to_string()).collect())
}

fn classify(name: &str, lines: &[String]) -> Option<Kind> {
    let n = name.to_lowercase();
    if n.starts_with("hs_err") {
        Some(Kind::HsErr)
    } else if n.starts_with("crash-") {
        Some(Kind::CrashReport)
    } else if matches!(n.as_str(), "latest.log" | "latest log.txt" | "debug.log" | "debug log.txt" | "rawoutput.log") {
        Some(Kind::McLog)
    } else if matches!(n.as_str(), "启动器日志.txt" | "pcl2 启动器日志.txt" | "pcl 启动器日志.txt" | "log1.txt") {
        if lines.iter().any(|l| l.contains("以下为游戏输出的最后一段内容")) { Some(Kind::McLog) } else { Some(Kind::Extra) }
    } else if n.ends_with(".log") {
        Some(Kind::Extra)
    } else {
        None
    }
}

/// Candidate log paths for an instance. `game_dir` is the working dir (isolated
/// version folder or the `.minecraft` root).
pub fn candidate_paths(game_dir: &Path, root: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = vec![];
    let mut push_dir = |d: &Path, only_log: bool, recurse: bool| {
        let Ok(rd) = fs::read_dir(d) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() && recurse {
                if let Ok(r2) = fs::read_dir(&p) {
                    v.extend(r2.flatten().map(|e| e.path()).filter(|p| p.is_file()));
                }
            } else if p.is_file() && (!only_log || p.extension().and_then(|x| x.to_str()) == Some("log")) {
                v.push(p);
            }
        }
    };
    push_dir(&game_dir.join("crash-reports"), false, true);
    push_dir(root, true, false);
    push_dir(game_dir, true, false);
    // JVM crash dumps land in the working directory.
    if let Ok(rd) = fs::read_dir(game_dir) {
        v.extend(
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.is_file() && p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("hs_err"))),
        );
    }
    v.push(game_dir.join("logs").join("latest.log"));
    v.push(game_dir.join("logs").join("debug.log"));
    v.sort();
    v.dedup();
    v
}

/// Build `Logs` from files. `recent_only` applies the 3 minute rule used right
/// after a crash; imported logs pass `false`. `live_output` is the captured tail
/// of the game's stdout/stderr, if any.
pub fn collect(paths: &[PathBuf], recent_only: bool, live_output: Option<&[String]>) -> Collected {
    let now = SystemTime::now();
    let mut files: Vec<(PathBuf, Kind, Vec<String>, SystemTime)> = vec![];
    for p in paths {
        let Ok(meta) = fs::metadata(p) else { continue };
        let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        if recent_only {
            let age = now.duration_since(mtime).unwrap_or_else(|e| e.duration());
            if age > RECENT_WINDOW {
                continue;
            }
        }
        let Some(lines) = read_lines(p) else { continue };
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if let Some(k) = classify(name, &lines) {
            files.push((p.clone(), k, lines, mtime));
        }
    }
    let mut raw_output_used = false;
    if let Some(out) = live_output.filter(|o| !o.is_empty()) {
        files.push((PathBuf::from("rawoutput.log"), Kind::McLog, out.to_vec(), now));
        raw_output_used = true;
    }

    // Only extra logs -> treat them as game logs.
    if !files.is_empty() && files.iter().all(|f| f.1 == Kind::Extra) {
        for f in files.iter_mut() {
            f.1 = Kind::McLog;
        }
    }

    let mut logs = Logs::default();
    let mut used: Vec<String> = vec![];
    let mut primary: Option<String> = None;
    let newest = |k: Kind, files: &[(PathBuf, Kind, Vec<String>, SystemTime)]| {
        files.iter().filter(|f| f.1 == k).max_by_key(|f| f.3).map(|f| (f.0.clone(), f.2.clone()))
    };

    if let Some((p, lines)) = newest(Kind::HsErr, &files) {
        logs.hs = Some(head_tail_lines(&lines, 200, 100));
        used.push(p.to_string_lossy().into_owned());
        primary.get_or_insert(p.to_string_lossy().into_owned());
    }
    if let Some((p, lines)) = newest(Kind::CrashReport, &files) {
        logs.crash = Some(head_tail_lines(&lines, 300, 700));
        used.push(p.to_string_lossy().into_owned());
        primary.get_or_insert(p.to_string_lossy().into_owned());
    }

    let mc_files: Vec<&(PathBuf, Kind, Vec<String>, SystemTime)> = files.iter().filter(|f| f.1 == Kind::McLog).collect();
    if !mc_files.is_empty() {
        let by_name = |n: &str| {
            mc_files.iter().find(|f| f.0.file_name().and_then(|x| x.to_str()).map(|x| x.to_lowercase()).as_deref() == Some(n))
        };
        let mut mc = String::new();
        for n in ["rawoutput.log", "启动器日志.txt", "log1.txt", "游戏崩溃前的输出.txt", "pcl2 启动器日志.txt", "pcl 启动器日志.txt"] {
            if let Some(f) = by_name(n) {
                let mut after = false;
                let mut got = false;
                for l in &f.2 {
                    if after {
                        mc.push_str(l);
                        mc.push('\n');
                        got = true;
                    } else if l.contains("以下为游戏输出的最后一段内容") {
                        after = true;
                    }
                }
                if !got {
                    mc.push_str(&head_tail_lines(&f.2, 0, 500));
                }
                mc = mc.trim_end_matches(['\r', '\n']).to_string();
                break;
            }
        }
        for n in ["latest.log", "latest log.txt", "debug.log", "debug log.txt"] {
            if let Some(f) = by_name(n) {
                mc.push_str(&head_tail_lines(&f.2, 1500, 500));
                break;
            }
        }
        let mut debug: Option<String> = None;
        for n in ["debug.log", "debug log.txt"] {
            if let Some(f) = by_name(n) {
                debug = Some(head_tail_lines(&f.2, 1000, 0));
                break;
            }
        }
        if mc.is_empty() {
            if let Some(d) = &debug {
                mc = d.clone();
            } else if let Some(f) = mc_files.first() {
                mc = head_tail_lines(&f.2, 1500, 500);
            }
        }
        if !mc.is_empty() {
            logs.mc = Some(mc);
        }
        logs.mc_debug = debug;
        for f in &mc_files {
            if f.0 != Path::new("rawoutput.log") {
                used.push(f.0.to_string_lossy().into_owned());
            }
        }
        // PCL prefers the game log as the file to open when there is no hs_err/crash report.
        if primary.is_none() {
            primary = mc_files
                .iter()
                .find(|f| f.0 != Path::new("rawoutput.log"))
                .map(|f| f.0.to_string_lossy().into_owned());
        }
    }
    let _ = raw_output_used;
    Collected { logs, files_used: used, primary_file: primary }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_recent_logs_and_skips_old() {
        let d = tempfile::tempdir().unwrap();
        let g = d.path();
        fs::create_dir_all(g.join("logs")).unwrap();
        fs::create_dir_all(g.join("crash-reports")).unwrap();
        fs::write(g.join("logs/latest.log"), "[INFO] hello\njava.lang.OutOfMemoryError\n").unwrap();
        fs::write(g.join("crash-reports/crash-2024.txt"), "---- Minecraft Crash Report ----\n").unwrap();
        fs::write(g.join("hs_err_pid1.log"), "# A fatal error\n").unwrap();
        fs::write(g.join("empty.log"), "").unwrap();
        let paths = candidate_paths(g, g);
        let c = collect(&paths, true, None);
        assert!(c.logs.mc.as_deref().unwrap().contains("OutOfMemoryError"));
        assert!(c.logs.crash.is_some());
        assert!(c.logs.hs.is_some());
        assert!(c.primary_file.is_some());
        assert!(!c.files_used.iter().any(|f| f.ends_with("empty.log")));
    }

    #[test]
    fn live_output_is_used() {
        let out = vec!["Error: Could not create the Java Virtual Machine.".to_string(), "Unrecognized option: --x".to_string()];
        let c = collect(&[], true, Some(&out));
        assert!(c.logs.mc.unwrap().contains("Unrecognized option"));
    }

    #[test]
    fn nothing_found() {
        let c = collect(&[], true, None);
        assert!(c.logs.mc.is_none() && c.logs.hs.is_none() && c.logs.crash.is_none());
    }
}
