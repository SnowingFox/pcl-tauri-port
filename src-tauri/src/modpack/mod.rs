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
        z.by_name(&format!("{base}{n}")).map_err(|e| e.to_string())?.read_to_string(&mut s).map_err(|e| e.to_string())?;
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

/// Extract the plan's override directories into `instance_dir` with zip-slip protection.
pub fn extract_overrides(archive: &Path, plan: &ModpackInstallPlan, instance_dir: &Path) -> Result<usize, String> {
    let f = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut z = zip::ZipArchive::new(f).map_err(|e| e.to_string())?;
    let mut count = 0;
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
        if let Some(p) = target.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let mut out = std::fs::File::create(&target).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
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
    fn detect() {
        let n = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(detect_format(&n(&["modrinth.index.json"])), Some((PackFormat::Modrinth, "".into())));
        assert_eq!(detect_format(&n(&["Pack/manifest.json"])), Some((PackFormat::CurseForge, "Pack/".into())));
        assert_eq!(detect_format(&n(&["x.txt"])), None);
    }
}
