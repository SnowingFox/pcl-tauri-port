//! Forge / NeoForge installer flow (client side), run natively on macOS:
//! download installer jar -> read install_profile.json -> write version json ->
//! fetch libraries (extracting the bundled ones) -> run processors with a JVM.
//! Flow follows PCL.Mac's ForgeInstaller (Swift reference); code is original.

use super::vanilla::*;
use crate::download::engine::FileSpec;
use crate::download::error::{DlError, DlResult};
use crate::download::manifest::{maven_path, host_os, VersionJson};
use crate::download::{Manager, TaskCtx};
use serde_json::Value;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flavor {
    Forge,
    NeoForge,
}

pub fn installer_url(f: Flavor, mc: &str, ver: &str) -> String {
    match f {
        Flavor::Forge => format!("https://maven.minecraftforge.net/net/minecraftforge/forge/{mc}-{ver}/forge-{mc}-{ver}-installer.jar"),
        Flavor::NeoForge => format!("https://maven.neoforged.net/releases/net/neoforged/neoforge/{ver}/neoforge-{ver}-installer.jar"),
    }
}

/// Parsed install_profile.json (modern layout). Old layout is detected by the `install` key.
#[derive(Debug)]
pub struct Profile {
    pub version_id: Option<String>,
    pub json_entry: String,
    pub data: HashMap<String, String>, // client-side values
    pub processors: Vec<Processor>,
    pub libraries: Vec<Value>,
}

#[derive(Debug, Clone)]
pub struct Processor {
    pub jar: String,
    pub classpath: Vec<String>,
    pub args: Vec<String>,
    pub outputs: Vec<(String, String)>,
}

pub fn parse_profile(v: &Value) -> DlResult<Profile> {
    let data = v["data"]
        .as_object()
        .map(|o| o.iter().filter_map(|(k, e)| Some((k.clone(), e["client"].as_str()?.to_string()))).collect())
        .unwrap_or_default();
    let strs = |x: &Value| -> Vec<String> { x.as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect()).unwrap_or_default() };
    let processors = v["processors"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|p| p["sides"].as_array().map(|s| s.iter().any(|x| x == "client")).unwrap_or(true))
                .filter_map(|p| {
                    Some(Processor {
                        jar: p["jar"].as_str()?.to_string(),
                        classpath: strs(&p["classpath"]),
                        args: strs(&p["args"]),
                        outputs: p["outputs"].as_object().map(|o| o.iter().filter_map(|(k, x)| Some((k.clone(), x.as_str()?.to_string()))).collect()).unwrap_or_default(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Profile {
        version_id: v["version"].as_str().map(String::from),
        json_entry: v["json"].as_str().unwrap_or("/version.json").trim_start_matches('/').to_string(),
        data,
        processors,
        libraries: v["libraries"].as_array().cloned().unwrap_or_default(),
    })
}

/// Resolve a `data`/argument token: `[maven:coord]` -> library path, `'lit'` -> lit, else unchanged.
pub fn resolve_token(tok: &str, libs: &Path) -> String {
    if tok.len() >= 2 && tok.starts_with('[') && tok.ends_with(']') {
        if let Some(p) = maven_path(&tok[1..tok.len() - 1]) {
            return libs.join(p).to_string_lossy().into_owned();
        }
    } else if tok.len() >= 2 && tok.starts_with('\'') && tok.ends_with('\'') {
        return tok[1..tok.len() - 1].to_string();
    }
    tok.to_string()
}

/// Substitute every `{KEY}` in `s` using `vars`, then resolve bracket/quote tokens.
pub fn substitute(s: &str, vars: &HashMap<String, String>, libs: &Path) -> String {
    let mut out = s.to_string();
    for (k, v) in vars {
        out = out.replace(&format!("{{{k}}}"), v);
    }
    resolve_token(&out, libs)
}

pub fn find_java(explicit: Option<&str>) -> String {
    if let Some(j) = explicit.filter(|s| !s.is_empty()) {
        return j.to_string();
    }
    if let Ok(j) = std::env::var("JAVA_HOME") {
        let p = Path::new(&j).join("bin/java");
        if p.is_file() {
            return p.to_string_lossy().into_owned();
        }
    }
    if let Ok(o) = std::process::Command::new("/usr/libexec/java_home").output()
        && o.status.success() {
            let p = Path::new(String::from_utf8_lossy(&o.stdout).trim()).join("bin/java");
            if p.is_file() {
                return p.to_string_lossy().into_owned();
            }
        }
    "java".into()
}

fn read_entry(jar: &Path, name: &str) -> DlResult<Vec<u8>> {
    let mut z = zip::ZipArchive::new(std::fs::File::open(jar)?)?;
    let mut f = z.by_name(name)?;
    let mut b = Vec::new();
    f.read_to_end(&mut b)?;
    Ok(b)
}

fn extract_entry(jar: &Path, name: &str, dest: &Path) -> DlResult<()> {
    let b = read_entry(jar, name)?;
    if let Some(p) = dest.parent() {
        std::fs::create_dir_all(p)?;
    }
    std::fs::write(dest, b)?;
    Ok(())
}

pub fn jar_main_class(jar: &Path) -> DlResult<String> {
    let m = String::from_utf8_lossy(&read_entry(jar, "META-INF/MANIFEST.MF")?).into_owned();
    m.lines()
        .find_map(|l| l.strip_prefix("Main-Class:").map(|s| s.trim().to_string()))
        .ok_or_else(|| DlError::Other(format!("{} has no Main-Class", jar.display())))
}

/// Download libraries, extracting those without a URL from the installer's `maven/` folder.
async fn fetch_profile_libs(ctx: &TaskCtx, libs_raw: &[Value], installer: &Path, start: f64, span: f64) -> DlResult<()> {
    let (os, arch) = host_os();
    let libdir = ctx.cfg.game_dir.join("libraries");
    let mut files = vec![];
    let mut bundled = vec![];
    for l in libs_raw {
        let lib: crate::download::manifest::Library = serde_json::from_value(l.clone())?;
        if let Some(a) = lib.downloads.as_ref().and_then(|d| d.artifact.as_ref())
            && a.url.is_empty() {
                if let Some(p) = a.path.clone().or_else(|| maven_path(&lib.name)).filter(|p| crate::settings::guard::is_safe_rel(p)) {
                    bundled.push(p);
                }
                continue;
            }
        for f in lib.files(os, arch) {
            files.push(FileSpec::new(f.url, libdir.join(&f.path)).with_sha1(f.sha1).with_size(f.size));
        }
    }
    let inst = installer.to_path_buf();
    let ld = libdir.clone();
    tokio::task::spawn_blocking(move || -> DlResult<()> {
        for p in bundled {
            let dest = ld.join(&p);
            if !dest.exists() {
                // Missing entry is tolerated: a processor may generate it later.
                let _ = extract_entry(&inst, &format!("maven/{p}"), &dest);
            }
        }
        Ok(())
    })
    .await
    .map_err(|e| DlError::Other(e.to_string()))??;
    ctx.tracker.begin_stage("loader-libraries", start, span, files.len() as u64, files.iter().filter_map(|f| f.size).sum());
    ctx.batch(files).await
}

async fn outputs_valid(outs: &[(String, String)], vars: &HashMap<String, String>, libs: &Path) -> bool {
    if outs.is_empty() {
        return false;
    }
    for (k, v) in outs {
        let path = substitute(k, vars, libs);
        let want = substitute(v, vars, libs).trim_matches('\'').to_string();
        match crate::download::hash::file_hash(crate::download::hash::HashKind::Sha1, PathBuf::from(&path)).await {
            Ok(h) if h.eq_ignore_ascii_case(&want) => {}
            _ => return false,
        }
    }
    true
}

/// Install Forge or NeoForge `ver` for `mc`. Returns the new version id.
pub async fn install_forge_like(ctx: &TaskCtx, mgr: &Arc<Manager>, fl: Flavor, mc: &str, ver: &str, java: Option<String>) -> DlResult<String> {
    let gd = ctx.cfg.game_dir.clone();
    let libs = gd.join("libraries");
    install_vanilla(ctx, mgr, mc, 0.0, 0.6).await?;

    // installer jar
    let tmp = gd.join(".pcl-tmp").join(format!("{}-{mc}-{ver}", if fl == Flavor::Forge { "forge" } else { "neoforge" }));
    tokio::fs::create_dir_all(&tmp).await?;
    let installer = tmp.join("installer.jar");
    ctx.tracker.begin_stage("installer", 0.6, 0.05, 1, 0);
    let mut spec = FileSpec::new(installer_url(fl, mc, ver), &installer);
    if fl == Flavor::NeoForge {
        spec.urls.push(format!("https://bmclapi2.bangbang93.com/neoforge/version/{ver}/download/installer.jar"));
    } else {
        spec.urls.push(format!("https://bmclapi2.bangbang93.com/forge/download?mcversion={mc}&version={ver}&category=installer&format=jar"));
    }
    ctx.file(spec).await?;

    // profile
    let prof_raw: Value = {
        let i = installer.clone();
        serde_json::from_slice(&tokio::task::spawn_blocking(move || read_entry(&i, "install_profile.json")).await.map_err(|e| DlError::Other(e.to_string()))??)?
    };
    let old = prof_raw.get("install").is_some();
    let (version_raw, vid, libs_raw, profile) = if old {
        let vi = prof_raw["versionInfo"].clone();
        let id = vi["id"].as_str().ok_or("old profile: versionInfo.id missing")?.to_string();
        // Old installers ship the loader jar inside the installer.
        let coord = prof_raw["install"]["path"].as_str().ok_or("old profile: install.path")?;
        let entry = prof_raw["install"]["filePath"].as_str().ok_or("old profile: install.filePath")?.to_string();
        let dest = libs.join(maven_path(coord).ok_or("bad maven coord")?);
        if !crate::settings::guard::is_safe_rel(&entry) {
            return Err(DlError::Other(format!("unsafe filePath in installer profile: {entry:?}")));
        }
        let i = installer.clone();
        tokio::task::spawn_blocking(move || extract_entry(&i, &entry, &dest)).await.map_err(|e| DlError::Other(e.to_string()))??;
        let l = vi["libraries"].as_array().cloned().unwrap_or_default();
        (vi, id, l, None)
    } else {
        let p = parse_profile(&prof_raw)?;
        let i = installer.clone();
        let entry = p.json_entry.clone();
        let vraw: Value = serde_json::from_slice(&tokio::task::spawn_blocking(move || read_entry(&i, &entry)).await.map_err(|e| DlError::Other(e.to_string()))??)?;
        let id = vraw["id"].as_str().map(String::from).or_else(|| p.version_id.clone()).ok_or("no version id")?;
        let mut l = p.libraries.clone();
        l.extend(vraw["libraries"].as_array().cloned().unwrap_or_default());
        (vraw, id, l, Some(p))
    };

    // version json (inheritsFrom vanilla is already present in the loader json)
    let jpath = version_json_path(&gd, &vid);
    tokio::fs::create_dir_all(jpath.parent().unwrap()).await?;
    tokio::fs::write(&jpath, serde_json::to_vec_pretty(&version_raw)?).await?;
    let _: VersionJson = serde_json::from_value(version_raw.clone())?;

    fetch_profile_libs(ctx, &libs_raw, &installer, 0.65, 0.2).await?;

    if let Some(p) = profile {
        // variables
        let mc_jar = version_jar_path(&gd, mc);
        let mut vars: HashMap<String, String> = HashMap::new();
        vars.insert("SIDE".into(), "client".into());
        vars.insert("INSTALLER".into(), installer.to_string_lossy().into());
        vars.insert("MINECRAFT_JAR".into(), mc_jar.to_string_lossy().into());
        vars.insert("MINECRAFT_VERSION".into(), mc.into());
        vars.insert("ROOT".into(), gd.to_string_lossy().into());
        vars.insert("LIBRARY_DIR".into(), libs.to_string_lossy().into());
        for (k, v) in &p.data {
            if let Some(entry) = v.strip_prefix('/') {
                if !crate::settings::guard::is_safe_rel(entry) {
                    return Err(DlError::Other(format!("unsafe data entry in installer profile: {entry:?}")));
                }
                let dest = tmp.join("data").join(entry);
                let (i, e, d) = (installer.clone(), entry.to_string(), dest.clone());
                tokio::task::spawn_blocking(move || extract_entry(&i, &e, &d)).await.map_err(|e| DlError::Other(e.to_string()))??;
                vars.insert(k.clone(), dest.to_string_lossy().into());
            } else {
                vars.insert(k.clone(), resolve_token(v, &libs));
            }
        }

        let java = find_java(java.as_deref());
        let n = p.processors.len().max(1) as f64;
        ctx.tracker.begin_stage("processors", 0.85, 0.15, p.processors.len() as u64, 0);
        for proc_ in &p.processors {
            ctx.ctl.checkpoint().await?;
            if outputs_valid(&proc_.outputs, &vars, &libs).await {
                ctx.tracker.file_done();
                continue;
            }
            let jar = libs.join(maven_path(&proc_.jar).ok_or("bad processor coord")?);
            let main = { let j = jar.clone(); tokio::task::spawn_blocking(move || jar_main_class(&j)).await.map_err(|e| DlError::Other(e.to_string()))?? };
            let mut cp: Vec<String> = proc_.classpath.iter().filter_map(|c| maven_path(c)).map(|p| libs.join(p).to_string_lossy().into_owned()).collect();
            cp.push(jar.to_string_lossy().into_owned());
            let args: Vec<String> = proc_.args.iter().map(|a| substitute(a, &vars, &libs)).collect();
            let out = tokio::process::Command::new(&java)
                .current_dir(&tmp)
                .arg("-cp")
                .arg(cp.join(":"))
                .arg(&main)
                .args(&args)
                .output()
                .await
                .map_err(|e| DlError::Other(format!("cannot run java ({java}): {e}")))?;
            if !out.status.success() {
                return Err(DlError::Other(format!(
                    "processor {} failed ({}): {}",
                    proc_.jar,
                    out.status,
                    String::from_utf8_lossy(&out.stderr).lines().rev().take(5).collect::<Vec<_>>().join(" | ")
                )));
            }
            let _ = n;
            ctx.tracker.file_done();
        }
    }
    let _ = tokio::fs::remove_dir_all(&tmp).await;
    Ok(vid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens() {
        let libs = Path::new("/g/libraries");
        assert_eq!(resolve_token("[net.minecraftforge:forge:1.0:client]", libs), "/g/libraries/net/minecraftforge/forge/1.0/forge-1.0-client.jar");
        assert_eq!(resolve_token("'1.20.1'", libs), "1.20.1");
        assert_eq!(resolve_token("--task", libs), "--task");
        let mut v = HashMap::new();
        v.insert("MINECRAFT_JAR".into(), "/m.jar".into());
        assert_eq!(substitute("{MINECRAFT_JAR}", &v, libs), "/m.jar");
    }

    #[test]
    fn profile_parse_filters_sides() {
        let j: Value = serde_json::from_str(
            r#"{"version":"1.20.1-forge-47.2.0","json":"/version.json","data":{"MAPPINGS":{"client":"[a:b:1:m@txt]","server":"x"}},
            "processors":[{"sides":["server"],"jar":"a:b:1","args":[]},{"jar":"c:d:2","classpath":["e:f:3"],"args":["--x","{MAPPINGS}"],"outputs":{"{X}":"'abc'"}}],
            "libraries":[{"name":"a:b:1"}]}"#,
        )
        .unwrap();
        let p = parse_profile(&j).unwrap();
        assert_eq!(p.processors.len(), 1);
        assert_eq!(p.processors[0].classpath, vec!["e:f:3"]);
        assert_eq!(p.data["MAPPINGS"], "[a:b:1:m@txt]");
        assert_eq!(p.json_entry, "version.json");
        assert_eq!(p.libraries.len(), 1);
    }

    #[test]
    fn installer_urls() {
        assert_eq!(
            installer_url(Flavor::Forge, "1.20.1", "47.2.0"),
            "https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.1-47.2.0/forge-1.20.1-47.2.0-installer.jar"
        );
        assert!(installer_url(Flavor::NeoForge, "1.21", "21.0.5").ends_with("neoforge/21.0.5/neoforge-21.0.5-installer.jar"));
    }

    #[test]
    fn main_class_from_jar() {
        use std::io::Write;
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("a.jar");
        let mut z = zip::ZipWriter::new(std::fs::File::create(&p).unwrap());
        z.start_file("META-INF/MANIFEST.MF", zip::write::SimpleFileOptions::default()).unwrap();
        z.write_all(b"Manifest-Version: 1.0\r\nMain-Class: a.b.Main\r\n").unwrap();
        z.finish().unwrap();
        assert_eq!(jar_main_class(&p).unwrap(), "a.b.Main");
    }
}
