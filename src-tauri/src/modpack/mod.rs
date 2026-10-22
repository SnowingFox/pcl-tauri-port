#![allow(dead_code)] // library surface consumed by other agents/UI later
//! Modpack parsing and install planning (Modrinth .mrpack, CurseForge manifest zip, MultiMC/Prism).
//!
//! Flow ported from PCL `Modules/Minecraft/ModModpack.vb`. This module never downloads files
//! itself: it produces a [`ModpackInstallPlan`] and drives a [`ModpackInstaller`] supplied by the
//! download engine.
pub mod commands;
pub mod curseforge_pack;
pub mod installer;
pub mod mmc;
pub mod mrpack;

use crate::resources::types::ModLoader;
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackFormat {
    Modrinth,
    CurseForge,
    MultiMc,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoaderSpec {
    pub loader: ModLoader,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanFile {
    /// Path relative to the instance directory (validated, never escapes it).
    pub path: String,
    /// Candidate URLs. Empty for unresolved CurseForge files.
    pub urls: Vec<String>,
    pub sha1: Option<String>,
    pub size: Option<u64>,
    /// Optional file: the UI should ask the user before including.
    pub optional: bool,
    /// CurseForge references that still need resolving through the API.
    pub curseforge: Option<CurseForgeRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CurseForgeRef {
    pub project_id: u64,
    pub file_id: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModpackInstallPlan {
    pub format: PackFormat,
    pub name: String,
    pub version: Option<String>,
    pub mc_version: String,
    pub loaders: Vec<LoaderSpec>,
    pub files: Vec<PlanFile>,
    /// Directories inside the archive whose contents are copied over the instance dir, in order.
    pub override_dirs: Vec<String>,
    /// Human-readable notes (e.g. unsupported loader).
    pub warnings: Vec<String>,
}

impl ModpackInstallPlan {
    /// A plan that went through the webview is untrusted again: re-check every path and URL
    /// the way `parse_pack` did. Unresolved CurseForge entries (no URLs yet) are allowed.
    pub fn validate(&self) -> Result<(), String> {
        crate::settings::guard::check_mc_version(&self.mc_version)?;
        for l in &self.loaders {
            crate::settings::guard::check_version_token("加载器版本", &l.version)?;
        }
        if self.files.len() > 20_000 || self.override_dirs.len() > 8 {
            return Err("modpack plan is too large".into());
        }
        for d in &self.override_dirs {
            if !d.is_empty() {
                safe_join(Path::new("/"), d)?;
            }
        }
        for f in &self.files {
            safe_join(Path::new("/"), &f.path)?;
            for u in &f.urls {
                crate::settings::guard::check_download_url(u).map_err(|e| format!("{}: {e}", f.path))?;
            }
            if let Some(h) = f.sha1.as_deref().filter(|h| !h.is_empty())
                && (h.len() != 40 || !h.bytes().all(|b| b.is_ascii_hexdigit())) {
                    return Err(format!("{}: invalid sha1", f.path));
                }
        }
        Ok(())
    }
}

/// Implemented by the download agent's engine.
#[async_trait::async_trait]
pub trait ModpackInstaller: Send + Sync {
    /// Download all files (with sha1 verification when given) into `instance_dir`.
    async fn download_files(&self, instance_dir: &Path, files: &[PlanFile]) -> Result<(), String>;
    /// Install vanilla + loaders into the instance.
    async fn install_game(&self, instance_dir: &Path, mc_version: &str, loaders: &[LoaderSpec]) -> Result<(), String>;
}

/// Reject absolute paths and `..` so entries cannot escape `base`. Returns the joined path.
pub fn safe_join(base: &Path, rel: &str) -> Result<PathBuf, String> {
    let rel = rel.replace('\\', "/");
    let mut out = base.to_path_buf();
    for c in Path::new(&rel).components() {
        match c {
            Component::Normal(p) => out.push(p),
            Component::CurDir => {}
            _ => return Err(format!("unsafe path in modpack: {rel}")),
        }
    }
    Ok(out)
}

/// Open the archive and detect the format; returns (format, base folder prefix inside the zip).
pub fn detect_format(names: &[String]) -> Option<(PackFormat, String)> {
    for (file, fmt) in [
        ("modrinth.index.json", PackFormat::Modrinth),
        ("manifest.json", PackFormat::CurseForge),
        ("mmc-pack.json", PackFormat::MultiMc),
    ] {
        // exact root, or one wrapper folder deep (PCL's ArchiveBaseFolder)
        if names.iter().any(|n| n == file) {
            return Some((fmt, String::new()));
        }
        if let Some(n) = names.iter().find(|n| n.matches('/').count() == 1 && n.ends_with(&format!("/{file}"))) {
            return Some((fmt, n[..n.len() - file.len()].to_string()));
        }
    }
    None
}

pub fn parse_pack(path: &Path) -> Result<ModpackInstallPlan, String> {
    let f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut z = zip::ZipArchive::new(f).map_err(|e| format!("not a valid modpack archive: {e}"))?;
    let names: Vec<String> = z.file_names().filter_map(|n| n.ok().map(|c| c.into_owned())).collect();
    let (fmt, base) = detect_format(&names).ok_or("unrecognized modpack format")?;
    let read = |z: &mut zip::ZipArchive<std::fs::File>, n: &str| -> Result<String, String> {
        use std::io::Read;
        let mut s = String::new();
        // Manifests are small JSON documents; a decompression bomb must not exhaust memory.
        z.by_name(&format!("{base}{n}")).map_err(|e| e.to_string())?.take(MAX_MANIFEST_BYTES + 1).read_to_string(&mut s).map_err(|e| e.to_string())?;
        if s.len() as u64 > MAX_MANIFEST_BYTES {
            return Err(format!("{n} is too large"));
        }
        Ok(s.trim_start_matches('\u{feff}').to_string())
    };
    match fmt {
        PackFormat::Modrinth => mrpack::parse(&read(&mut z, "modrinth.index.json")?, &base),
        PackFormat::CurseForge => curseforge_pack::parse(&read(&mut z, "manifest.json")?, &base),
        PackFormat::MultiMc => {
            let cfg = read(&mut z, "instance.cfg").ok();
            mmc::parse(&read(&mut z, "mmc-pack.json")?, cfg.as_deref(), &base)
        }
    }
}

/// Largest manifest (`modrinth.index.json`, `manifest.json`, `mmc-pack.json`) we read.
pub const MAX_MANIFEST_BYTES: u64 = 32 * 1024 * 1024;
/// Limits for extracted overrides: a modpack is untrusted input.
pub const MAX_OVERRIDE_ENTRIES: usize = 50_000;
pub const MAX_OVERRIDE_BYTES: u64 = 4 * 1024 * 1024 * 1024;

/// Launcher-owned files inside an instance folder that pack content must never replace:
/// the per-instance settings (`PCL/`: Java path, JVM arguments, pre-launch command) and the
/// version's own `<name>.json` / `<name>.jar` / `<name>-natives/`, which decide what is executed.
/// A pack that wants to ship these gets an error instead of a silent skip.
fn is_launcher_owned(instance_dir: &Path, rel: &str) -> bool {
    let rel = rel.replace('\\', "/");
    let first = rel.split('/').next().unwrap_or("");
    if first.eq_ignore_ascii_case("PCL") {
        return true;
    }
    let name = instance_dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if name.is_empty() {
        return false;
    }
    let l = |s: &str| s.to_ascii_lowercase();
    let (first, name) = (l(first), l(name));
    first == format!("{name}.json") || first == format!("{name}.jar") || first == format!("{name}-natives")
}

/// A write target below `base` must not pass through a symlink (an earlier entry or a
/// pre-existing link could redirect the write outside the instance).
fn ensure_no_symlink(base: &Path, target: &Path) -> Result<(), String> {
    let rel = target.strip_prefix(base).map_err(|_| format!("unsafe path in modpack: {}", target.display()))?;
    let mut cur = base.to_path_buf();
    for c in rel.components() {
        cur.push(c);
        match std::fs::symlink_metadata(&cur) {
            Ok(m) if m.file_type().is_symlink() => return Err(format!("refusing to write through a symlink: {}", cur.display())),
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => break,
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}

/// Extract the plan's override directories into `instance_dir` with zip-slip protection,
/// symlink refusal and entry-count / size limits (decompression bombs).
pub fn extract_overrides(archive: &Path, plan: &ModpackInstallPlan, instance_dir: &Path) -> Result<usize, String> {
    use std::io::Read;
    let f = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut z = zip::ZipArchive::new(f).map_err(|e| e.to_string())?;
    if z.len() > MAX_OVERRIDE_ENTRIES {
        return Err(format!("modpack has too many entries ({})", z.len()));
    }
    std::fs::create_dir_all(instance_dir).map_err(|e| e.to_string())?;
    let mut count = 0;
    let mut budget = MAX_OVERRIDE_BYTES;
    for i in 0..z.len() {
        let mut entry = z.by_index(i).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        let name = match entry.name() { Ok(n) => n.into_owned(), Err(_) => continue };
        let Some(rel) = plan.override_dirs.iter().find_map(|d| {
            let d = if d.is_empty() || d.ends_with('/') { d.clone() } else { format!("{d}/") };
            name.strip_prefix(&d).map(str::to_string)
        }) else {
            continue;
        };
        if rel.is_empty() {
            continue;
        }
        let target = safe_join(instance_dir, &rel)?;
        if is_launcher_owned(instance_dir, &rel) {
            return Err(format!("modpack tries to overwrite launcher-owned file: {rel}"));
        }
        ensure_no_symlink(instance_dir, &target)?;
        if let Some(p) = target.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
            ensure_no_symlink(instance_dir, p)?;
        }
        let mut out = std::fs::File::create(&target).map_err(|e| e.to_string())?;
        // Count what is actually written, not what the header claims.
        let written = std::io::copy(&mut (&mut entry).take(budget + 1), &mut out).map_err(|e| e.to_string())?;
        if written > budget {
            drop(out);
            let _ = std::fs::remove_file(&target);
            return Err("modpack overrides are too large".into());
        }
        budget -= written;
        count += 1;
    }
    Ok(count)
}

#[cfg(test)]
pub(crate) fn build_zip(files: &[(&str, &str)]) -> Vec<u8> {
    crate::resources::local_mods::build_jar(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_join_rules() {
        let b = Path::new("/tmp/inst");
        assert!(safe_join(b, "mods/a.jar").is_ok());
        assert!(safe_join(b, "../evil").is_err());
        assert!(safe_join(b, "/etc/passwd").is_err());
        assert!(safe_join(b, "a\\..\\b").is_err());
    }

    #[test]
    fn plans_from_the_webview_are_revalidated() {
        let idx = r#"{"name":"P","dependencies":{"minecraft":"1.20.1"},"files":[{"path":"mods/a.jar","hashes":{"sha1":"da39a3ee5e6b4b0d3255bfef95601890afd80709"},"downloads":["https://cdn.modrinth.com/data/a.jar"]}]}"#;
        let d = tempfile::tempdir().unwrap();
        let pack = d.path().join("p.mrpack");
        std::fs::write(&pack, build_zip(&[("modrinth.index.json", idx)])).unwrap();
        let good = parse_pack(&pack).unwrap();
        assert!(good.validate().is_ok());

        let mut p = good.clone();
        p.files[0].path = "../../.zshrc".into();
        assert!(p.validate().is_err());
        let mut p = good.clone();
        p.files[0].urls = vec!["file:///etc/passwd".into()];
        assert!(p.validate().is_err());
        let mut p = good.clone();
        p.files[0].urls = vec!["https://169.254.169.254/latest".into()];
        assert!(p.validate().is_err());
        let mut p = good.clone();
        p.files[0].sha1 = Some("nothex".into());
        assert!(p.validate().is_err());
        let mut p = good.clone();
        p.override_dirs = vec!["../../etc".into()];
        assert!(p.validate().is_err());
        let mut p = good.clone();
        p.mc_version = "../1.20".into();
        assert!(p.validate().is_err());
    }

    #[test]
    fn extract_refuses_launcher_owned_files() {
        let d = tempfile::tempdir().unwrap();
        let idx = r#"{"name":"P","dependencies":{"minecraft":"1.20.1"},"files":[]}"#;
        for evil in ["overrides/PCL/config.json", "overrides/pcl/config.json", "overrides/Pack.json", "overrides/pack.JAR", "overrides/Pack-natives/x.dylib"] {
            let inst = d.path().join("Pack");
            let _ = std::fs::remove_dir_all(&inst);
            let pack = d.path().join("p.mrpack");
            std::fs::write(&pack, build_zip(&[("modrinth.index.json", idx), ("overrides/options.txt", "ok"), (evil, "x")])).unwrap();
            let plan = parse_pack(&pack).unwrap();
            assert!(extract_overrides(&pack, &plan, &inst).is_err(), "{evil}");
            assert!(!inst.join("PCL").exists() && !inst.join("Pack.json").exists(), "{evil}");
        }
        // Ordinary content, including a config folder with a json file, is still installed.
        let inst = d.path().join("Pack");
        let _ = std::fs::remove_dir_all(&inst);
        let pack = d.path().join("ok.mrpack");
        std::fs::write(&pack, build_zip(&[("modrinth.index.json", idx), ("overrides/config/a.json", "{}"), ("overrides/options.txt", "o")])).unwrap();
        let plan = parse_pack(&pack).unwrap();
        assert_eq!(extract_overrides(&pack, &plan, &inst).unwrap(), 2);
    }

    #[test]
    fn extract_refuses_symlinks_and_oversized_manifests() {
        let d = tempfile::tempdir().unwrap();
        let inst = d.path().join("inst");
        let outside = d.path().join("outside");
        std::fs::create_dir_all(&inst).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        // `config` already exists inside the instance as a symlink to a folder outside it.
        std::os::unix::fs::symlink(&outside, inst.join("config")).unwrap();
        let idx = r#"{"name":"P","dependencies":{"minecraft":"1.20.1"},"files":[]}"#;
        let pack = d.path().join("p.mrpack");
        std::fs::write(&pack, build_zip(&[("modrinth.index.json", idx), ("overrides/config/evil.toml", "x")])).unwrap();
        let plan = parse_pack(&pack).unwrap();
        assert!(extract_overrides(&pack, &plan, &inst).is_err());
        assert!(!outside.join("evil.toml").exists(), "a write went through the symlink");

        // A manifest bigger than the limit is refused instead of being buffered.
        let big = format!(r#"{{"name":"{}","dependencies":{{"minecraft":"1"}},"files":[]}}"#, "A".repeat(MAX_MANIFEST_BYTES as usize));
        let pack2 = d.path().join("big.mrpack");
        std::fs::write(&pack2, build_zip(&[("modrinth.index.json", &big)])).unwrap();
        assert!(parse_pack(&pack2).is_err());
    }

    #[test]
    fn detect() {
        let n = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(detect_format(&n(&["modrinth.index.json"])), Some((PackFormat::Modrinth, "".into())));
        assert_eq!(detect_format(&n(&["Pack/manifest.json"])), Some((PackFormat::CurseForge, "Pack/".into())));
        assert_eq!(detect_format(&n(&["x.txt"])), None);
    }
}
