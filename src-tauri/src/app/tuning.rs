//! Merge global + per-instance settings into concrete launch parameters.

use crate::instances::detect::{InstanceInfo, LoaderKind};
use crate::java::{self, JavaInfo};
use crate::settings::{GameWindowMode, GlobalSettings, InstanceSettings, MemoryMode};
use std::path::Path;

/// Shell-like split: whitespace separated, single or double quotes group.
pub fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    for c in s.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => cur.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    started = false;
                }
            }
            (None, c) => cur.push(c),
        }
    }
    if started || !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Physical memory in MiB (best effort, 8 GiB when unknown).
pub fn physical_memory_mb() -> u64 {
    #[cfg(target_os = "macos")]
    if let Ok(o) = std::process::Command::new("sysctl").args(["-n", "hw.memsize"]).output()
        && let Ok(b) = String::from_utf8_lossy(&o.stdout).trim().parse::<u64>()
    {
        return b / (1024 * 1024);
    }
    #[cfg(target_os = "linux")]
    if let Ok(s) = std::fs::read_to_string("/proc/meminfo") {
        if let Some(kb) = s.lines().find_map(|l| l.strip_prefix("MemTotal:")).and_then(|v| v.trim().trim_end_matches("kB").trim().parse::<u64>().ok()) {
            return kb / 1024;
        }
    }
    8192
}

/// Automatic heap size: vanilla 2 GiB, modded 4-8 GiB depending on mod count, never above
/// 60% of physical memory and never below 1 GiB.
pub fn auto_memory_mb(total_mb: u64, modded: bool, mod_count: usize) -> u32 {
    let want: u64 = match (modded, mod_count) {
        (false, _) => 2048,
        (true, 0..=50) => 4096,
        (true, 51..=150) => 6144,
        (true, _) => 8192,
    };
    want.min(total_mb * 60 / 100).max(1024) as u32
}

pub fn count_mod_files(mods_dir: &Path) -> usize {
    std::fs::read_dir(mods_dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| {
                    let n = e.file_name().to_string_lossy().to_ascii_lowercase();
                    n.ends_with(".jar") || n.ends_with(".litemod")
                })
                .count()
        })
        .unwrap_or(0)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tuning {
    pub max_memory_mb: u32,
    pub jvm_args: Vec<String>,
    pub game_args: Vec<String>,
    pub resolution: Option<(u32, u32)>,
    /// Java executables to try first: instance setting, then global selection.
    pub java_preferred: Vec<String>,
}

pub fn merge(g: &GlobalSettings, inst: &InstanceSettings, info: &InstanceInfo, mod_count: usize, total_mb: u64) -> Tuning {
    let max_memory_mb = match (inst.memory_mb, g.memory.mode) {
        (Some(mb), _) => mb,
        (None, MemoryMode::Custom) => g.memory.custom_mb,
        (None, MemoryMode::Auto) => auto_memory_mb(total_mb, info.modable, mod_count),
    };
    // A non-empty per-instance value replaces the global one.
    let jvm = if inst.jvm_args.trim().is_empty() { &g.game.jvm_args } else { &inst.jvm_args };
    let game = if inst.game_args.trim().is_empty() { &g.game.game_args } else { &inst.game_args };
    let mut game_args = split_args(game);
    let resolution = match g.game.window_mode {
        GameWindowMode::Default => None,
        GameWindowMode::Custom => Some((g.game.window_width, g.game.window_height)),
        GameWindowMode::SameAsLauncher => Some((g.window.width, g.window.height)),
        GameWindowMode::Fullscreen => {
            game_args.push("--fullscreen".into());
            None
        }
    };
    let java_preferred = [inst.java_path.clone(), g.java.selected.clone()]
        .into_iter()
        .flatten()
        .filter(|p| !p.trim().is_empty())
        .collect();
    Tuning { max_memory_mb, jvm_args: split_args(jvm), game_args, resolution, java_preferred }
}

/// Upper bound for the Java major, when newer versions are known to break the game:
/// LaunchWrapper based setups (Forge/LiteLoader/OptiFine on <= 1.16) cast the system class loader
/// to `URLClassLoader`, which Java 9+ no longer allows.
pub fn max_java_major(required: u32, main_class: &str, info: &InstanceInfo) -> Option<u32> {
    let legacy_loader = info.loaders.iter().any(|l| matches!(l.kind, LoaderKind::Forge | LoaderKind::LiteLoader | LoaderKind::OptiFine));
    (required <= 8 && (main_class.contains("launchwrapper") || legacy_loader)).then_some(8)
}

fn acceptable(j: &JavaInfo, required: u32, max: Option<u32>) -> bool {
    j.major >= required && max.is_none_or(|m| j.major <= m)
}

/// Choose the Java: an explicitly preferred one when it fits, else the best discovered match.
/// `Err` carries the majors that were found (for the error details).
pub fn pick_java(preferred: &[String], discovered: Vec<JavaInfo>, required: u32, max: Option<u32>) -> Result<(JavaInfo, Option<String>), Vec<u32>> {
    let mut note = None;
    let mut seen_majors: Vec<u32> = Vec::new();
    for p in preferred {
        match java::inspect(Path::new(p)) {
            Some(j) if acceptable(&j, required, max) => return Ok((j, None)),
            Some(j) => {
                seen_majors.push(j.major);
                note = Some(format!("所选 Java {} 不符合要求（需要 Java {required}），已自动选择其他版本", j.major));
            }
            None => note = Some(format!("所选 Java 不可用：{p}，已自动选择其他版本")),
        }
    }
    let mut majors: Vec<u32> = discovered.iter().map(|j| j.major).chain(seen_majors).collect();
    majors.sort_unstable();
    majors.dedup();
    let fitting: Vec<JavaInfo> = discovered.into_iter().filter(|j| acceptable(j, required, max)).collect();
    match java::select_for(&fitting, required) {
        Some(j) => Ok((j.clone(), note)),
        None => Err(majors),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instances::detect::{Category, InstanceKind, Loader};

    fn info(modable: bool, loaders: Vec<Loader>) -> InstanceInfo {
        InstanceInfo {
            name: "x".into(), path: "/p".into(), root: "/r".into(), kind: InstanceKind::Release, mc_version: Some("1.20.1".into()),
            loaders, release_time: None, version_type: None, description: String::new(), category: Category::Error, icon: String::new(),
            custom_logo_path: None, starred: false, hidden: false, modable, game_dir: "/g".into(), isolated: true, inherits_from: None, error: None,
        }
    }

    fn j(major: u32, arch: &str) -> JavaInfo {
        JavaInfo { path: format!("/j{major}/bin/java"), home: format!("/j{major}"), version: major.to_string(), major, arch: arch.into(), vendor: String::new(), is_jdk: true }
    }

    #[test]
    fn splits_quoted_args() {
        assert_eq!(split_args("-Xss1M  -Dfoo=\"a b\" 'c d' \"\""), vec!["-Xss1M", "-Dfoo=a b", "c d", ""]);
        assert!(split_args("   ").is_empty());
    }

    #[test]
    fn auto_memory_scales_and_caps() {
        assert_eq!(auto_memory_mb(16384, false, 0), 2048);
        assert_eq!(auto_memory_mb(16384, true, 10), 4096);
        assert_eq!(auto_memory_mb(16384, true, 100), 6144);
        assert_eq!(auto_memory_mb(32768, true, 300), 8192);
        assert_eq!(auto_memory_mb(8192, true, 300), 4915, "60% of 8 GiB");
        assert_eq!(auto_memory_mb(1024, false, 0), 1024);
    }

    #[test]
    fn merges_instance_over_global() {
        let mut g = GlobalSettings::default();
        g.memory.mode = MemoryMode::Custom;
        g.memory.custom_mb = 3000;
        g.game.game_args = "--g".into();
        g.java.selected = Some("/global/java".into());
        let mut i = InstanceSettings::default();
        let t = merge(&g, &i, &info(false, vec![]), 0, 16384);
        assert_eq!(t.max_memory_mb, 3000);
        assert_eq!(t.game_args, vec!["--g"]);
        assert_eq!(t.java_preferred, vec!["/global/java"]);
        assert_eq!(t.jvm_args, split_args(&g.game.jvm_args));
        i.memory_mb = Some(5000);
        i.game_args = "--i x".into();
        i.jvm_args = "-Xss2M".into();
        i.java_path = Some("/inst/java".into());
        let t = merge(&g, &i, &info(false, vec![]), 0, 16384);
        assert_eq!((t.max_memory_mb, t.game_args, t.jvm_args), (5000, vec!["--i".into(), "x".into()], vec!["-Xss2M".to_string()]));
        assert_eq!(t.java_preferred, vec!["/inst/java", "/global/java"]);
    }

    #[test]
    fn window_modes() {
        let mut g = GlobalSettings::default();
        let i = InstanceSettings::default();
        let inf = info(false, vec![]);
        g.game.window_mode = GameWindowMode::Custom;
        g.game.window_width = 1280;
        g.game.window_height = 720;
        assert_eq!(merge(&g, &i, &inf, 0, 8192).resolution, Some((1280, 720)));
        g.game.window_mode = GameWindowMode::Fullscreen;
        let t = merge(&g, &i, &inf, 0, 8192);
        assert_eq!(t.resolution, None);
        assert!(t.game_args.contains(&"--fullscreen".to_string()));
        g.game.window_mode = GameWindowMode::Default;
        assert_eq!(merge(&g, &i, &inf, 0, 8192).resolution, None);
    }

    #[test]
    fn java_choice_and_limits() {
        let native = if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" };
        let all = vec![j(8, native), j(17, native), j(21, native)];
        assert_eq!(pick_java(&[], all.clone(), 17, None).unwrap().0.major, 17);
        assert_eq!(pick_java(&[], all.clone(), 8, Some(8)).unwrap().0.major, 8);
        assert_eq!(pick_java(&[], all.clone(), 21, None).unwrap().0.major, 21);
        assert_eq!(pick_java(&[], vec![j(8, native)], 17, None).unwrap_err(), vec![8]);
        assert_eq!(pick_java(&[], all.clone(), 8, Some(8)).unwrap().1, None);
        let (_, note) = pick_java(&["/no/such/java".into()], all, 17, None).unwrap();
        assert!(note.unwrap().contains("不可用"));
    }

    #[test]
    fn legacy_loader_java_cap() {
        let forge = info(true, vec![Loader { kind: LoaderKind::Forge, version: None }]);
        assert_eq!(max_java_major(8, "net.minecraft.launchwrapper.Launch", &forge), Some(8));
        assert_eq!(max_java_major(8, "net.minecraft.client.main.Main", &info(false, vec![])), None);
        assert_eq!(max_java_major(17, "cpw.mods.bootstraplauncher.BootstrapLauncher", &forge), None);
    }
}
