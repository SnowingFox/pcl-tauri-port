//! Local mod scanning, metadata parsing, enable/disable and update checks.
use super::hash::{curseforge_fingerprint, sha1_hex};
use super::types::*;
use super::{modrinth, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ModMeta {
    pub mod_id: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub authors: Vec<String>,
    pub loaders: Vec<ModLoader>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalMod {
    pub path: String,
    pub file_name: String,
    pub enabled: bool,
    pub size: u64,
    pub sha1: String,
    pub curseforge_fingerprint: u32,
    pub meta: ModMeta,
}

fn read_entry<R: Read + Seek>(z: &mut zip::ZipArchive<R>, name: &str) -> Option<Vec<u8>> {
    let mut f = z.by_name(name).ok()?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;
    Some(buf)
}

fn jstr(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_string).filter(|x| !x.is_empty())
}

fn authors_from(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string).or_else(|| jstr(x, "name")))
                .collect()
        })
        .unwrap_or_default()
}

pub fn parse_fabric_json(text: &str) -> Option<ModMeta> {
    let v: Value = serde_json::from_str(text).ok()?;
    Some(ModMeta {
        mod_id: jstr(&v, "id"),
        name: jstr(&v, "name"),
        version: jstr(&v, "version"),
        description: jstr(&v, "description"),
        authors: authors_from(&v["authors"]),
        loaders: vec![ModLoader::Fabric],
    })
}

pub fn parse_quilt_json(text: &str) -> Option<ModMeta> {
    let v: Value = serde_json::from_str(text).ok()?;
    let q = &v["quilt_loader"];
    let m = &q["metadata"];
    let authors = match &m["contributors"] {
        Value::Object(o) => o.keys().cloned().collect(),
        other => authors_from(other),
    };
    Some(ModMeta {
        mod_id: jstr(q, "id"),
        name: jstr(m, "name"),
        version: jstr(q, "version"),
        description: jstr(m, "description"),
        authors,
        loaders: vec![ModLoader::Quilt],
    })
}

/// `manifest_version` is used to resolve `${file.jarVersion}` placeholders.
pub fn parse_mods_toml(text: &str, loader: ModLoader, manifest_version: Option<&str>) -> Option<ModMeta> {
    let t: toml::Table = toml::from_str(text).ok()?;
    let m = t.get("mods")?.as_array()?.first()?.as_table()?;
    let get = |k: &str| m.get(k).and_then(|x| x.as_str()).map(|x| x.trim().to_string()).filter(|x| !x.is_empty());
    let version = get("version").map(|v| {
        if v.contains("${file.jarVersion}") {
            manifest_version.unwrap_or("").to_string()
        } else {
            v
        }
    });
    let authors = get("authors")
        .or_else(|| t.get("authors").and_then(|x| x.as_str()).map(str::to_string))
        .map(|a| a.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect())
        .unwrap_or_default();
    Some(ModMeta {
        mod_id: get("modId"),
        name: get("displayName"),
        version: version.filter(|v| !v.is_empty()),
        description: get("description"),
        authors,
        loaders: vec![loader],
    })
}

pub fn parse_mcmod_info(text: &str) -> Option<ModMeta> {
    let v: Value = serde_json::from_str(text).ok()?;
    let first = match &v {
        Value::Array(a) => a.first()?,
        Value::Object(_) => v.get("modList").and_then(|l| l.get(0)).unwrap_or(&v),
        _ => return None,
    };
    let authors = if first.get("authorList").is_some() {
        authors_from(&first["authorList"])
    } else {
        authors_from(&first["authors"])
    };
    Some(ModMeta {
        mod_id: jstr(first, "modid"),
        name: jstr(first, "name"),
        version: jstr(first, "version"),
        description: jstr(first, "description"),
        authors,
        loaders: vec![ModLoader::Forge],
    })
}

fn manifest_version(text: &str) -> Option<String> {
    text.lines().find_map(|l| l.strip_prefix("Implementation-Version:").map(|v| v.trim().to_string()))
}

/// Parse metadata from jar bytes. Tries Fabric, Quilt, NeoForge, Forge, then legacy mcmod.info.
pub fn parse_jar_meta(bytes: &[u8]) -> ModMeta {
    let mut z = match zip::ZipArchive::new(std::io::Cursor::new(bytes)) {
        Ok(z) => z,
        Err(_) => return ModMeta::default(),
    };
    let text = |z: &mut zip::ZipArchive<_>, n: &str| read_entry(z, n).map(|b| String::from_utf8_lossy(&b).trim_start_matches('\u{feff}').to_string());
    if let Some(t) = text(&mut z, "fabric.mod.json")
        && let Some(m) = parse_fabric_json(&t) {
            return m;
        }
    if let Some(t) = text(&mut z, "quilt.mod.json")
        && let Some(m) = parse_quilt_json(&t) {
            return m;
        }
    let mf = text(&mut z, "META-INF/MANIFEST.MF").and_then(|t| manifest_version(&t));
    for (path, loader) in [
        ("META-INF/neoforge.mods.toml", ModLoader::NeoForge),
        ("META-INF/mods.toml", ModLoader::Forge),
    ] {
        if let Some(t) = text(&mut z, path)
            && let Some(m) = parse_mods_toml(&t, loader, mf.as_deref()) {
                return m;
            }
    }
    if let Some(t) = text(&mut z, "mcmod.info")
        && let Some(m) = parse_mcmod_info(&t) {
            return m;
        }
    ModMeta::default()
}

pub fn is_disabled(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".disabled")
}

fn is_mod_file(name: &str) -> bool {
    let l = name.to_ascii_lowercase();
    let l = l.strip_suffix(".disabled").unwrap_or(&l);
    l.ends_with(".jar") || l.ends_with(".litemod")
}

pub fn scan_mods_dir(dir: &Path) -> Result<Vec<LocalMod>> {
    let mut out = Vec::new();
    let rd = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(e.to_string()),
    };
    for entry in rd.flatten() {
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_string();
        if !path.is_file() || !is_mod_file(&file_name) {
            continue;
        }
        let bytes = std::fs::read(&path).map_err(|e| format!("{file_name}: {e}"))?;
        out.push(LocalMod {
            path: path.to_string_lossy().to_string(),
            enabled: !is_disabled(&file_name),
            size: bytes.len() as u64,
            sha1: sha1_hex(&bytes),
            curseforge_fingerprint: curseforge_fingerprint(&bytes),
            meta: parse_jar_meta(&bytes),
            file_name,
        });
    }
    out.sort_by_key(|a| a.file_name.to_lowercase());
    Ok(out)
}

/// Rename `x.jar` <-> `x.jar.disabled`. Returns the new path.
pub fn set_mod_enabled(path: &Path, enabled: bool) -> Result<PathBuf> {
    let name = path.file_name().and_then(|n| n.to_str()).ok_or("invalid path")?;
    // Only mod files may be renamed (never `~/.zshrc` -> `~/.zshrc.disabled`), and never through a symlink.
    if !is_mod_file(name) {
        return Err(format!("{name} is not a mod file"));
    }
    if std::fs::symlink_metadata(path).map(|m| !m.file_type().is_file()).unwrap_or(true) {
        return Err(format!("{name} is not a regular file"));
    }
    let new_name = match (enabled, is_disabled(name)) {
        (true, true) => name[..name.len() - ".disabled".len()].to_string(),
        (false, false) => format!("{name}.disabled"),
        _ => return Ok(path.to_path_buf()),
    };
    let target = path.with_file_name(new_name);
    if target.exists() {
        return Err(format!("{} already exists", target.display()));
    }
    std::fs::rename(path, &target).map_err(|e| e.to_string())?;
    Ok(target)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModUpdate {
    pub path: String,
    pub current_sha1: String,
    pub project_id: String,
    pub latest: ResourceFile,
}

/// Check Modrinth for newer versions of the given mods (matched by sha1) for a game version.
pub async fn check_updates(mods: &[LocalMod], mc_version: &str, loader: ModLoader) -> Result<Vec<ModUpdate>> {
    if mods.is_empty() {
        return Ok(vec![]);
    }
    let hashes: Vec<String> = mods.iter().map(|m| m.sha1.clone()).collect();
    let current = modrinth::versions_by_hashes(&hashes).await?;
    let mut loaders = vec![loader];
    if loader == ModLoader::Quilt {
        loaders.push(ModLoader::Fabric);
    }
    let latest = modrinth::latest_by_hashes(&hashes, &loaders, &[mc_version.to_string()]).await?;
    Ok(compute_updates(mods, &current, &latest))
}

/// An update exists when the latest version has a different file hash and a newer publish date.
pub fn compute_updates(
    mods: &[LocalMod],
    current: &std::collections::HashMap<String, ResourceFile>,
    latest: &std::collections::HashMap<String, ResourceFile>,
) -> Vec<ModUpdate> {
    mods.iter()
        .filter_map(|m| {
            let cur = current.get(&m.sha1)?;
            let new = latest.get(&m.sha1)?;
            if new.sha1.as_deref() == Some(m.sha1.as_str()) || new.download_urls.is_empty() {
                return None;
            }
            // ISO-8601 UTC strings compare lexicographically
            if new.date_published <= cur.date_published {
                return None;
            }
            Some(ModUpdate {
                path: m.path.clone(),
                current_sha1: m.sha1.clone(),
                project_id: new.project_id.clone(),
                latest: new.clone(),
            })
        })
        .collect()
}

#[cfg(test)]
pub(crate) fn build_jar(files: &[(&str, &str)]) -> Vec<u8> {
    use std::io::Write;
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    for (n, c) in files {
        w.start_file(*n, opts).unwrap();
        w.write_all(c.as_bytes()).unwrap();
    }
    w.finish().unwrap().into_inner()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fabric() {
        let jar = build_jar(&[("fabric.mod.json", r#"{"id":"sodium","name":"Sodium","version":"0.5.0","authors":["jelly",{"name":"x"}],"description":"fast"}"#)]);
        let m = parse_jar_meta(&jar);
        assert_eq!(m.mod_id.as_deref(), Some("sodium"));
        assert_eq!(m.authors, vec!["jelly", "x"]);
        assert_eq!(m.loaders, vec![ModLoader::Fabric]);
    }

    #[test]
    fn quilt() {
        let jar = build_jar(&[("quilt.mod.json", r#"{"quilt_loader":{"id":"qm","version":"1.0","metadata":{"name":"QM","contributors":{"Alice":"Owner"}}}}"#)]);
        let m = parse_jar_meta(&jar);
        assert_eq!((m.mod_id.as_deref(), m.authors.as_slice()), (Some("qm"), &["Alice".to_string()][..]));
        assert_eq!(m.loaders, vec![ModLoader::Quilt]);
    }

    #[test]
    fn forge_toml_with_manifest_version() {
        let jar = build_jar(&[
            ("META-INF/mods.toml", "modLoader=\"javafml\"\n[[mods]]\nmodId=\"jei\"\nversion=\"${file.jarVersion}\"\ndisplayName=\"JEI\"\nauthors=\"mezz, other\"\n"),
            ("META-INF/MANIFEST.MF", "Manifest-Version: 1.0\nImplementation-Version: 15.2.0\n"),
        ]);
        let m = parse_jar_meta(&jar);
        assert_eq!(m.version.as_deref(), Some("15.2.0"));
        assert_eq!(m.authors, vec!["mezz", "other"]);
        assert_eq!(m.loaders, vec![ModLoader::Forge]);
    }

    #[test]
    fn neoforge_toml() {
        let jar = build_jar(&[("META-INF/neoforge.mods.toml", "[[mods]]\nmodId=\"nm\"\nversion=\"2.0\"\n")]);
        assert_eq!(parse_jar_meta(&jar).loaders, vec![ModLoader::NeoForge]);
    }

    #[test]
    fn mcmod_info_both_shapes() {
        let a = parse_mcmod_info(r#"[{"modid":"old","name":"Old","version":"1","authorList":["a"]}]"#).unwrap();
        assert_eq!(a.mod_id.as_deref(), Some("old"));
        let b = parse_mcmod_info(r#"{"modListVersion":2,"modList":[{"modid":"new","name":"N"}]}"#).unwrap();
        assert_eq!(b.mod_id.as_deref(), Some("new"));
    }

    #[test]
    fn garbage_jar_gives_default() {
        assert_eq!(parse_jar_meta(b"not a zip"), ModMeta::default());
    }

    #[test]
    fn scan_and_toggle() {
        let dir = std::env::temp_dir().join(format!("pcl-mods-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.jar"), build_jar(&[("fabric.mod.json", r#"{"id":"a"}"#)])).unwrap();
        std::fs::write(dir.join("b.jar.disabled"), build_jar(&[])).unwrap();
        std::fs::write(dir.join("readme.txt"), "x").unwrap();
        let mods = scan_mods_dir(&dir).unwrap();
        assert_eq!(mods.len(), 2);
        assert!(mods[0].enabled && !mods[1].enabled);
        let p = set_mod_enabled(Path::new(&mods[0].path), false).unwrap();
        assert!(p.ends_with("a.jar.disabled") && p.exists());
        let p = set_mod_enabled(&p, true).unwrap();
        assert!(p.ends_with("a.jar"));
        // Only mod files can be toggled: arbitrary files must not be renamed.
        std::fs::write(dir.join("notes.txt"), "x").unwrap();
        assert!(set_mod_enabled(&dir.join("notes.txt"), false).is_err());
        assert!(dir.join("notes.txt").exists() && !dir.join("notes.txt.disabled").exists());
        std::os::unix::fs::symlink("/etc/hosts", dir.join("link.jar")).unwrap();
        assert!(set_mod_enabled(&dir.join("link.jar"), false).is_err());
        assert!(set_mod_enabled(&dir.join("missing.jar"), false).is_err());
        std::fs::remove_file(dir.join("link.jar")).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn update_computation() {
        let mk = |sha: &str, date: &str, url: bool| ResourceFile {
            platform: Platform::Modrinth, id: "v".into(), project_id: "p".into(), name: "".into(), file_name: "".into(),
            version_number: None, release_type: "release".into(), game_versions: vec![], loaders: vec![],
            sha1: Some(sha.into()), size: 0, download_urls: if url { vec!["u".into()] } else { vec![] },
            date_published: date.into(), dependencies: vec![], fingerprint: None,
        };
        let lm = LocalMod { path: "p".into(), file_name: "a.jar".into(), enabled: true, size: 0, sha1: "old".into(), curseforge_fingerprint: 0, meta: ModMeta::default() };
        let cur = [("old".to_string(), mk("old", "2024-01-01", true))].into();
        let newer = [("old".to_string(), mk("new", "2024-06-01", true))].into();
        assert_eq!(compute_updates(std::slice::from_ref(&lm), &cur, &newer).len(), 1);
        let same = [("old".to_string(), mk("old", "2024-06-01", true))].into();
        assert!(compute_updates(&[lm], &cur, &same).is_empty());
    }
}
