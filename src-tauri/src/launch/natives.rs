//! Natives extraction and library presence checks.

use super::version::*;
use super::{args::classpath_libraries, LaunchError};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

fn is_native_lib(name: &str) -> bool {
    name.ends_with(".dylib") || name.ends_with(".jnilib") || name.ends_with(".so") || name.ends_with(".dll")
}

fn extract_jar(jar: &Path, dest: &Path, exclude: &[String], only_native_libs: bool) -> Result<(), LaunchError> {
    let file = fs::File::open(jar).map_err(|e| LaunchError::Io(format!("{}: {e}", jar.display())))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| LaunchError::Io(format!("{}: {e}", jar.display())))?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| LaunchError::Io(e.to_string()))?;
        if entry.is_dir() {
            continue;
        }
        // enclosed_name rejects zip-slip paths.
        let Some(rel) = entry.enclosed_name() else { continue };
        let name = rel.to_string_lossy().replace('\\', "/");
        if exclude.iter().any(|e| name.starts_with(e.as_str())) || name.starts_with("META-INF/") {
            continue;
        }
        if only_native_libs && !is_native_lib(&name) {
            continue;
        }
        // Flatten for new-style natives (libs may live in subfolders like macos/arm64/org/lwjgl/).
        let out = if only_native_libs {
            dest.join(rel.file_name().unwrap_or_default())
        } else {
            dest.join(&rel)
        };
        if let Some(p) = out.parent() {
            fs::create_dir_all(p).map_err(|e| LaunchError::Io(e.to_string()))?;
        }
        let mut f = fs::File::create(&out).map_err(|e| LaunchError::Io(format!("{}: {e}", out.display())))?;
        io::copy(&mut entry, &mut f).map_err(|e| LaunchError::Io(e.to_string()))?;
    }
    Ok(())
}

/// Jars that must be unpacked into the natives dir: (path, exclude list, flatten-native-libs-only).
pub fn native_jars(v: &ResolvedVersion, libraries_dir: &Path, ctx: &RuleContext) -> Vec<(PathBuf, Vec<String>, bool)> {
    let mut out = Vec::new();
    for lib in &v.json.libraries {
        if !lib.allowed(ctx) {
            continue;
        }
        if let Some((rel, _)) = lib.native_artifact(ctx) {
            let excl = lib.extract.as_ref().map(|e| e.exclude.clone()).unwrap_or_default();
            out.push((libraries_dir.join(rel), excl, false));
        } else if lib.is_new_style_native()
            && let Some(rel) = lib.classpath_rel_path() {
                out.push((libraries_dir.join(rel), vec![], true));
            }
    }
    out
}

/// Re-creates the natives dir. Returns the number of jars extracted.
pub fn extract_natives(v: &ResolvedVersion, libraries_dir: &Path, natives_dir: &Path, ctx: &RuleContext) -> Result<usize, LaunchError> {
    if natives_dir.exists() {
        fs::remove_dir_all(natives_dir).map_err(|e| LaunchError::Io(e.to_string()))?;
    }
    fs::create_dir_all(natives_dir).map_err(|e| LaunchError::Io(e.to_string()))?;
    let jars = native_jars(v, libraries_dir, ctx);
    for (jar, excl, flat) in &jars {
        extract_jar(jar, natives_dir, excl, *flat)?;
    }
    Ok(jars.len())
}

/// Library files (classpath + natives) that are missing on disk.
pub fn missing_libraries(v: &ResolvedVersion, libraries_dir: &Path, ctx: &RuleContext) -> Vec<PathBuf> {
    let mut missing: Vec<PathBuf> = classpath_libraries(v, ctx)
        .iter()
        .filter_map(|l| l.classpath_rel_path())
        .map(|r| libraries_dir.join(r))
        .filter(|p| !p.is_file())
        .collect();
    for (p, _, _) in native_jars(v, libraries_dir, ctx) {
        if !p.is_file() && !missing.contains(&p) {
            missing.push(p);
        }
    }
    missing
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::io::Write;

    #[test]
    fn extracts_and_flattens() {
        let tmp = std::env::temp_dir().join(format!("pcl-natives-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let libs = tmp.join("libraries");
        let rel = "org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1-natives-macos-arm64.jar";
        fs::create_dir_all(libs.join(rel).parent().unwrap()).unwrap();
        {
            let f = fs::File::create(libs.join(rel)).unwrap();
            let mut z = zip::ZipWriter::new(f);
            let o = zip::write::SimpleFileOptions::default();
            z.start_file("macos/arm64/org/lwjgl/liblwjgl.dylib", o).unwrap();
            z.write_all(b"x").unwrap();
            z.start_file("META-INF/MANIFEST.MF", o).unwrap();
            z.write_all(b"m").unwrap();
            z.finish().unwrap();
        }
        let json = format!(
            r#"{{"id":"t","mainClass":"M","libraries":[{{"name":"org.lwjgl:lwjgl:3.3.1:natives-macos-arm64","downloads":{{"artifact":{{"path":"{rel}"}}}}}}]}}"#
        );
        let j = parse_version_json(&json).unwrap();
        let v = ResolvedVersion { id: "t".into(), jar_id: "t".into(), root_id: "t".into(), json: j };
        let ctx = RuleContext { os_name: "osx".into(), arch: "arm64".into(), features: HashMap::new() };
        assert!(missing_libraries(&v, &libs, &ctx).is_empty());
        let nd = tmp.join("natives");
        assert_eq!(extract_natives(&v, &libs, &nd, &ctx).unwrap(), 1);
        assert!(nd.join("liblwjgl.dylib").is_file());
        assert!(!nd.join("META-INF").exists());
        let _ = fs::remove_dir_all(&tmp);
    }
}
