//! Version JSON model, `inheritsFrom` merging and rule evaluation.
//!
//! Logic follows the Mojang launcher-meta format as consumed by PCL
//! (ModMinecraft.vb / ModLaunch.vb); this is a Rust re-implementation.

use super::LaunchError;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct VersionJson {
    pub id: String,
    #[serde(rename = "inheritsFrom")]
    pub inherits_from: Option<String>,
    #[serde(rename = "mainClass")]
    pub main_class: Option<String>,
    #[serde(rename = "minecraftArguments")]
    pub minecraft_arguments: Option<String>,
    pub arguments: Option<Arguments>,
    pub libraries: Vec<Library>,
    #[serde(rename = "assetIndex")]
    pub asset_index: Option<AssetIndexRef>,
    pub assets: Option<String>,
    #[serde(rename = "javaVersion")]
    pub java_version: Option<JavaVersionReq>,
    #[serde(rename = "type")]
    pub version_type: Option<String>,
    pub jar: Option<String>,
    pub downloads: Option<VersionDownloads>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct VersionDownloads {
    pub client: Option<Artifact>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct JavaVersionReq {
    pub component: Option<String>,
    #[serde(rename = "majorVersion")]
    pub major_version: u32,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct AssetIndexRef {
    pub id: String,
    pub url: Option<String>,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct Arguments {
    pub game: Vec<ArgItem>,
    pub jvm: Vec<ArgItem>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ArgItem {
    Plain(String),
    Conditional {
        #[serde(default)]
        rules: Vec<Rule>,
        value: ArgValue,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ArgValue {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct Artifact {
    pub path: Option<String>,
    pub url: Option<String>,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct LibraryDownloads {
    pub artifact: Option<Artifact>,
    pub classifiers: HashMap<String, Artifact>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct ExtractRule {
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct Library {
    pub name: String,
    pub downloads: Option<LibraryDownloads>,
    pub natives: Option<HashMap<String, String>>,
    pub rules: Vec<Rule>,
    pub extract: Option<ExtractRule>,
    /// Maven repo base for downloads-less (Forge style) libraries.
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct Rule {
    pub action: String,
    pub os: Option<OsRule>,
    pub features: Option<HashMap<String, bool>>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct OsRule {
    pub name: Option<String>,
    pub arch: Option<String>,
    /// Regex on the OS version. Not evaluated (only used for Windows 10 rules).
    pub version: Option<String>,
}

/// Environment used to evaluate rules.
#[derive(Debug, Clone)]
pub struct RuleContext {
    /// Mojang OS name: "osx", "windows", "linux".
    pub os_name: String,
    /// "arm64", "x64" or "x86".
    pub arch: String,
    pub features: HashMap<String, bool>,
}

impl RuleContext {
    pub fn current(features: HashMap<String, bool>) -> Self {
        let os_name = if cfg!(target_os = "macos") {
            "osx"
        } else if cfg!(target_os = "windows") {
            "windows"
        } else {
            "linux"
        };
        let arch = if cfg!(target_arch = "aarch64") {
            "arm64"
        } else if cfg!(target_arch = "x86") {
            "x86"
        } else {
            "x64"
        };
        RuleContext { os_name: os_name.into(), arch: arch.into(), features }
    }
}

impl OsRule {
    fn matches(&self, ctx: &RuleContext) -> bool {
        if let Some(name) = &self.name {
            // Some manifests use "osx-arm64"/"osx-x86" style names.
            let (base, arch_suffix) = match name.split_once('-') {
                Some((b, a)) => (b, Some(a)),
                None => (name.as_str(), None),
            };
            let base = if base == "macos" { "osx" } else { base };
            if base != ctx.os_name {
                return false;
            }
            if let Some(a) = arch_suffix {
                if !arch_eq(a, &ctx.arch) {
                    return false;
                }
            }
        }
        if let Some(arch) = &self.arch {
            if !arch_eq(arch, &ctx.arch) {
                return false;
            }
        }
        true
    }
}

fn arch_eq(rule_arch: &str, ctx_arch: &str) -> bool {
    match rule_arch {
        "arm64" | "aarch64" => ctx_arch == "arm64",
        "x86_64" | "amd64" | "x64" => ctx_arch == "x64",
        "x86" | "i386" => ctx_arch == "x86",
        _ => false,
    }
}

impl Rule {
    fn matches(&self, ctx: &RuleContext) -> bool {
        if let Some(os) = &self.os {
            if !os.matches(ctx) {
                return false;
            }
        }
        if let Some(features) = &self.features {
            for (k, want) in features {
                if ctx.features.get(k).copied().unwrap_or(false) != *want {
                    return false;
                }
            }
        }
        true
    }
}

/// Mojang semantics: no rules => allowed; otherwise start disallowed and the
/// last matching rule decides.
pub fn rules_allow(rules: &[Rule], ctx: &RuleContext) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for r in rules {
        if r.matches(ctx) {
            allowed = r.action == "allow";
        }
    }
    allowed
}

impl Library {
    /// (group, artifact, version, classifier, ext)
    pub fn maven_parts(&self) -> Option<(String, String, String, Option<String>, String)> {
        let (coord, ext) = match self.name.split_once('@') {
            Some((c, e)) => (c, e.to_string()),
            None => (self.name.as_str(), "jar".to_string()),
        };
        let p: Vec<&str> = coord.split(':').collect();
        if p.len() < 3 {
            return None;
        }
        Some((p[0].into(), p[1].into(), p[2].into(), p.get(3).map(|s| s.to_string()), ext))
    }

    /// Dedup key: group:artifact[:classifier]
    pub fn key(&self) -> String {
        match self.maven_parts() {
            Some((g, a, _, Some(c), _)) => format!("{g}:{a}:{c}"),
            Some((g, a, _, None, _)) => format!("{g}:{a}"),
            None => self.name.clone(),
        }
    }

    fn maven_rel_path(&self, classifier_override: Option<&str>) -> Option<String> {
        let (g, a, v, c, ext) = self.maven_parts()?;
        let c = classifier_override.map(|s| s.to_string()).or(c);
        let file = match c {
            Some(c) => format!("{a}-{v}-{c}.{ext}"),
            None => format!("{a}-{v}.{ext}"),
        };
        Some(format!("{}/{a}/{v}/{file}", g.replace('.', "/")))
    }

    pub fn allowed(&self, ctx: &RuleContext) -> bool {
        rules_allow(&self.rules, ctx)
    }

    pub fn artifact(&self) -> Option<&Artifact> {
        self.downloads.as_ref()?.artifact.as_ref()
    }

    /// Old-style natives: library has a `natives` entry for this OS.
    pub fn native_classifier(&self, ctx: &RuleContext) -> Option<String> {
        let c = self.natives.as_ref()?.get(&ctx.os_name)?;
        let bits = if ctx.arch == "x86" { "32" } else { "64" };
        Some(c.replace("${arch}", bits))
    }

    /// Relative path (under libraries/) of the jar placed on the classpath,
    /// or None if this library is natives-only.
    pub fn classpath_rel_path(&self) -> Option<String> {
        if self.natives.is_some() && self.artifact().is_none() {
            return None;
        }
        if let Some(p) = self.artifact().and_then(|a| a.path.clone()) {
            return Some(p);
        }
        self.maven_rel_path(None)
    }

    /// Download info for the classpath jar (if known); for the download module.
    #[allow(dead_code)]
    pub fn classpath_artifact(&self) -> Option<Artifact> {
        let rel = self.classpath_rel_path()?;
        let mut art = self.artifact().cloned().unwrap_or_default();
        if art.url.is_none() {
            if let Some(base) = &self.url {
                art.url = Some(format!("{}/{}", base.trim_end_matches('/'), rel));
            }
        }
        art.path = Some(rel);
        Some(art)
    }

    /// (relative path, artifact info) of the old-style natives jar.
    pub fn native_artifact(&self, ctx: &RuleContext) -> Option<(String, Artifact)> {
        let cls = self.native_classifier(ctx)?;
        let art = self.downloads.as_ref().and_then(|d| d.classifiers.get(&cls)).cloned();
        let rel = art
            .as_ref()
            .and_then(|a| a.path.clone())
            .or_else(|| self.maven_rel_path(Some(&cls)))?;
        let mut art = art.unwrap_or_default();
        art.path = Some(rel.clone());
        Some((rel, art))
    }

    /// New-style LWJGL natives jar (`...:natives-macos[-arm64]` classifier on the classpath).
    pub fn is_new_style_native(&self) -> bool {
        matches!(self.maven_parts(), Some((_, _, _, Some(c), _)) if c.starts_with("natives"))
    }
}

/// Fully merged version (child over `inheritsFrom` chain).
#[derive(Debug, Clone)]
pub struct ResolvedVersion {
    pub json: VersionJson,
    /// Id used for the launch.
    pub id: String,
    /// Id of the version whose `<id>.jar` is the client jar.
    pub jar_id: String,
    /// Id at the end of the inheritance chain (usually the vanilla MC version).
    pub root_id: String,
}

pub fn version_json_path(minecraft_dir: &Path, id: &str) -> PathBuf {
    minecraft_dir.join("versions").join(id).join(format!("{id}.json"))
}

pub fn version_jar_path(minecraft_dir: &Path, jar_id: &str) -> PathBuf {
    minecraft_dir.join("versions").join(jar_id).join(format!("{jar_id}.jar"))
}

pub fn parse_version_json(text: &str) -> Result<VersionJson, LaunchError> {
    serde_json::from_str(text).map_err(|e| LaunchError::VersionJson(e.to_string()))
}

pub fn resolve_version(minecraft_dir: &Path, id: &str) -> Result<ResolvedVersion, LaunchError> {
    resolve_with(id, &|vid| {
        let p = version_json_path(minecraft_dir, vid);
        let text = std::fs::read_to_string(&p)
            .map_err(|e| LaunchError::VersionJson(format!("{}: {e}", p.display())))?;
        parse_version_json(&text)
    })
}

/// Loader-injected variant used by tests.
pub fn resolve_with(
    id: &str,
    load: &dyn Fn(&str) -> Result<VersionJson, LaunchError>,
) -> Result<ResolvedVersion, LaunchError> {
    let mut chain: Vec<VersionJson> = Vec::new();
    let mut seen = HashSet::new();
    let mut cur = id.to_string();
    loop {
        if !seen.insert(cur.clone()) || chain.len() > 10 {
            return Err(LaunchError::VersionJson(format!("inheritsFrom cycle at {cur}")));
        }
        let mut v = load(&cur)?;
        if v.id.is_empty() {
            v.id = cur.clone();
        }
        let next = v.inherits_from.clone().filter(|s| !s.is_empty());
        chain.push(v);
        match next {
            Some(n) => cur = n,
            None => break,
        }
    }
    let root_id = chain.last().map(|v| v.id.clone()).unwrap_or_default();
    // jar: nearest explicit `jar` field, else root id.
    let jar_id = chain.iter().find_map(|v| v.jar.clone()).unwrap_or_else(|| root_id.clone());
    let mut it = chain.into_iter().rev();
    let mut merged = it.next().unwrap();
    for child in it {
        merged = merge(merged, child);
    }
    merged.id = id.to_string();
    Ok(ResolvedVersion { json: merged, id: id.to_string(), jar_id, root_id })
}

fn merge(parent: VersionJson, child: VersionJson) -> VersionJson {
    let arguments = match (parent.arguments, child.arguments) {
        (Some(mut p), Some(c)) => {
            p.game.extend(c.game);
            p.jvm.extend(c.jvm);
            Some(p)
        }
        (p, c) => c.or(p),
    };
    let mut libraries = child.libraries;
    libraries.extend(parent.libraries);
    VersionJson {
        id: child.id,
        inherits_from: None,
        main_class: child.main_class.or(parent.main_class),
        minecraft_arguments: child.minecraft_arguments.or(parent.minecraft_arguments),
        arguments,
        libraries,
        asset_index: child.asset_index.or(parent.asset_index),
        assets: child.assets.or(parent.assets),
        java_version: child.java_version.or(parent.java_version),
        version_type: child.version_type.or(parent.version_type),
        jar: child.jar.or(parent.jar),
        downloads: child.downloads.or(parent.downloads),
    }
}

#[cfg(test)]
pub mod fixtures {
    pub const VANILLA_MODERN: &str = r#"{
      "id": "1.20.1", "type": "release", "mainClass": "net.minecraft.client.main.Main",
      "javaVersion": {"component": "java-runtime-gamma", "majorVersion": 17},
      "assetIndex": {"id": "5", "url": "https://example.invalid/5.json"},
      "assets": "5",
      "downloads": {"client": {"url": "https://example.invalid/client.jar", "sha1": "abc", "size": 1}},
      "arguments": {
        "game": ["--username", "${auth_player_name}", "--version", "${version_name}",
          {"rules": [{"action": "allow", "features": {"is_demo_user": true}}], "value": "--demo"},
          {"rules": [{"action": "allow", "features": {"has_custom_resolution": true}}],
           "value": ["--width", "${resolution_width}", "--height", "${resolution_height}"]}],
        "jvm": [
          {"rules": [{"action": "allow", "os": {"name": "osx"}}], "value": ["-XstartOnFirstThread"]},
          {"rules": [{"action": "allow", "os": {"name": "windows"}}], "value": "-Dwin=1"},
          "-Djava.library.path=${natives_directory}", "-cp", "${classpath}"]
      },
      "libraries": [
        {"name": "com.mojang:logging:1.1.1", "downloads": {"artifact": {"path": "com/mojang/logging/1.1.1/logging-1.1.1.jar", "url": "u"}}},
        {"name": "org.lwjgl:lwjgl:3.3.1", "downloads": {"artifact": {"path": "org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1.jar"}}},
        {"name": "org.lwjgl:lwjgl:3.3.1:natives-macos-arm64",
         "rules": [{"action": "allow", "os": {"name": "osx", "arch": "arm64"}}],
         "downloads": {"artifact": {"path": "org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1-natives-macos-arm64.jar"}}},
        {"name": "org.lwjgl:lwjgl:3.3.1:natives-macos",
         "rules": [{"action": "allow", "os": {"name": "osx"}}],
         "downloads": {"artifact": {"path": "org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1-natives-macos.jar"}}},
        {"name": "org.lwjgl:lwjgl:3.3.1:natives-windows",
         "rules": [{"action": "allow", "os": {"name": "windows"}}],
         "downloads": {"artifact": {"path": "org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1-natives-windows.jar"}}}
      ]}"#;

    pub const FORGE_CHILD: &str = r#"{
      "id": "1.20.1-forge", "inheritsFrom": "1.20.1", "type": "release",
      "mainClass": "cpw.mods.bootstraplauncher.BootstrapLauncher",
      "arguments": {"game": ["--launchTarget", "forgeclient"], "jvm": ["-DforgeVer=1"]},
      "libraries": [
        {"name": "com.mojang:logging:1.2.0", "url": "https://maven.example.invalid/"},
        {"name": "net.minecraftforge:fmlloader:1.20.1-47.2.0"}
      ]}"#;

    pub const LEGACY: &str = r#"{
      "id": "1.7.10", "type": "release", "mainClass": "net.minecraft.client.main.Main",
      "assets": "1.7.10",
      "minecraftArguments": "--username ${auth_player_name} --session ${auth_session} --gameDir ${game_directory} --assetsDir ${assets_root} --version ${version_name}",
      "assetIndex": {"id": "1.7.10"},
      "libraries": [
        {"name": "org.lwjgl.lwjgl:lwjgl-platform:2.9.1",
         "natives": {"linux": "natives-linux", "osx": "natives-osx", "windows": "natives-windows"},
         "extract": {"exclude": ["META-INF/"]},
         "downloads": {"classifiers": {"natives-osx": {"path": "org/lwjgl/lwjgl/lwjgl-platform/2.9.1/lwjgl-platform-2.9.1-natives-osx.jar", "url": "u"}}}},
        {"name": "com.google.guava:guava:15.0", "downloads": {"artifact": {"path": "com/google/guava/guava/15.0/guava-15.0.jar"}}}
      ]}"#;
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;

    fn ctx(os: &str, arch: &str, feats: &[(&str, bool)]) -> RuleContext {
        RuleContext {
            os_name: os.into(),
            arch: arch.into(),
            features: feats.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
        }
    }

    fn rule(json: &str) -> Vec<Rule> {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn rules_basic() {
        let c = ctx("osx", "arm64", &[]);
        assert!(rules_allow(&[], &c));
        assert!(rules_allow(&rule(r#"[{"action":"allow"}]"#), &c));
        assert!(!rules_allow(&rule(r#"[{"action":"allow","os":{"name":"windows"}}]"#), &c));
        // allow all, then disallow osx
        assert!(!rules_allow(
            &rule(r#"[{"action":"allow"},{"action":"disallow","os":{"name":"osx"}}]"#),
            &c
        ));
        assert!(rules_allow(&rule(r#"[{"action":"allow","os":{"name":"osx","arch":"arm64"}}]"#), &c));
        assert!(!rules_allow(
            &rule(r#"[{"action":"allow","os":{"name":"osx","arch":"arm64"}}]"#),
            &ctx("osx", "x64", &[])
        ));
        assert!(!rules_allow(&rule(r#"[{"action":"allow","os":{"arch":"x86"}}]"#), &c));
        assert!(rules_allow(&rule(r#"[{"action":"allow","os":{"name":"osx-arm64"}}]"#), &c));
    }

    #[test]
    fn rules_features() {
        let r = rule(r#"[{"action":"allow","features":{"is_demo_user":true}}]"#);
        assert!(!rules_allow(&r, &ctx("osx", "arm64", &[])));
        assert!(rules_allow(&r, &ctx("osx", "arm64", &[("is_demo_user", true)])));
    }

    #[test]
    fn inherits_merge() {
        let resolved = resolve_with("1.20.1-forge", &|id| match id {
            "1.20.1-forge" => parse_version_json(FORGE_CHILD),
            "1.20.1" => parse_version_json(VANILLA_MODERN),
            _ => Err(LaunchError::VersionJson("nf".into())),
        })
        .unwrap();
        assert_eq!(resolved.root_id, "1.20.1");
        assert_eq!(resolved.jar_id, "1.20.1");
        let j = &resolved.json;
        assert_eq!(j.main_class.as_deref(), Some("cpw.mods.bootstraplauncher.BootstrapLauncher"));
        assert_eq!(j.java_version.as_ref().unwrap().major_version, 17);
        assert_eq!(j.asset_index.as_ref().unwrap().id, "5");
        let a = j.arguments.as_ref().unwrap();
        assert_eq!(a.game.len(), 6 + 2); // 6 vanilla items + 2 forge
        assert!(j.libraries.iter().any(|l| l.name.contains("fmlloader")));
    }

    #[test]
    fn inherits_cycle_detected() {
        let r = resolve_with("a", &|id| {
            Ok(VersionJson { id: id.into(), inherits_from: Some(if id == "a" { "b".into() } else { "a".into() }), ..Default::default() })
        });
        assert!(r.is_err());
    }

    #[test]
    fn library_paths_and_natives() {
        let v = parse_version_json(LEGACY).unwrap();
        let c = ctx("osx", "x64", &[]);
        let lwjgl = &v.libraries[0];
        assert!(lwjgl.classpath_rel_path().is_none());
        let (rel, _) = lwjgl.native_artifact(&c).unwrap();
        assert!(rel.ends_with("natives-osx.jar"));
        assert_eq!(v.libraries[1].classpath_rel_path().unwrap(), "com/google/guava/guava/15.0/guava-15.0.jar");

        let l = Library { name: "net.minecraftforge:fmlloader:1.20.1-47.2.0".into(), ..Default::default() };
        assert_eq!(
            l.classpath_rel_path().unwrap(),
            "net/minecraftforge/fmlloader/1.20.1-47.2.0/fmlloader-1.20.1-47.2.0.jar"
        );
        let l = Library { name: "a.b:c:1.0:natives-macos@zip".into(), ..Default::default() };
        assert_eq!(l.classpath_rel_path().unwrap(), "a/b/c/1.0/c-1.0-natives-macos.zip");
        assert_eq!(l.key(), "a.b:c:natives-macos");
    }
}
