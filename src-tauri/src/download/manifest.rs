//! Mojang version manifest + version JSON models and library rule evaluation.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VersionManifest {
    pub latest: Latest,
    pub versions: Vec<VersionEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Latest {
    pub release: String,
    pub snapshot: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VersionEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default, rename = "releaseTime")]
    pub release_time: Option<String>,
}

impl VersionManifest {
    pub fn parse(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
    pub fn find(&self, id: &str) -> Option<&VersionEntry> {
        self.versions.iter().find(|v| v.id == id)
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct DownloadInfo {
    #[serde(default)]
    pub path: Option<String>,
    pub url: String,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct OsRule {
    pub name: Option<String>,
    pub arch: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Rule {
    pub action: String,
    #[serde(default)]
    pub os: Option<OsRule>,
    #[serde(default)]
    pub features: Option<Value>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LibDownloads {
    pub artifact: Option<DownloadInfo>,
    #[serde(default)]
    pub classifiers: Option<std::collections::HashMap<String, DownloadInfo>>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Library {
    pub name: String,
    #[serde(default)]
    pub downloads: Option<LibDownloads>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub rules: Option<Vec<Rule>>,
    #[serde(default)]
    pub natives: Option<std::collections::HashMap<String, String>>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AssetIndexRef {
    pub id: String,
    pub url: String,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct VersionJson {
    pub id: String,
    #[serde(default, rename = "inheritsFrom")]
    pub inherits_from: Option<String>,
    #[serde(default)]
    pub libraries: Vec<Library>,
    #[serde(default, rename = "assetIndex")]
    pub asset_index: Option<AssetIndexRef>,
    #[serde(default)]
    pub downloads: Option<std::collections::HashMap<String, DownloadInfo>>,
    #[serde(default)]
    pub logging: Option<Value>,
}

/// Current OS/arch names in Mojang's vocabulary.
pub fn host_os() -> (&'static str, &'static str) {
    let os = if cfg!(target_os = "macos") {
        "osx"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    };
    let arch = if cfg!(target_arch = "aarch64") { "arm64" } else if cfg!(target_arch = "x86") { "x86" } else { "x86_64" };
    (os, arch)
}

/// Mojang rule semantics: no rules = allowed; last matching rule wins; feature rules never match here.
pub fn rules_allow(rules: &Option<Vec<Rule>>, os: &str, arch: &str) -> bool {
    let Some(rules) = rules else { return true };
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for r in rules {
        if r.features.is_some() {
            continue;
        }
        let matches = match &r.os {
            None => true,
            Some(o) => {
                o.name.as_deref().map(|n| n == os).unwrap_or(true)
                    && o.arch.as_deref().map(|a| a == arch || (a == "x86" && arch == "x86")).unwrap_or(true)
            }
        };
        if matches {
            allowed = r.action == "allow";
        }
    }
    allowed
}

/// Maven coordinate `g:a:v[:classifier][@ext]` to repository-relative path.
pub fn maven_path(coord: &str) -> Option<String> {
    let (coord, ext) = match coord.split_once('@') {
        Some((c, e)) => (c, e),
        None => (coord, "jar"),
    };
    let p: Vec<&str> = coord.split(':').collect();
    if p.len() < 3 {
        return None;
    }
    let (g, a, v) = (p[0].replace('.', "/"), p[1], p[2]);
    let cls = p.get(3).map(|c| format!("-{c}")).unwrap_or_default();
    let out = format!("{g}/{a}/{v}/{a}-{v}{cls}.{ext}");
    // Coordinates come from remote JSON and installer jars: refuse anything that would leave
    // the libraries folder (`g:../..:1`, absolute groups, empty segments).
    crate::settings::guard::is_safe_rel(&out).then_some(out)
}

pub struct LibFile {
    pub path: String,
    pub url: String,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

impl Library {
    /// Files to fetch for this library on the host platform (main artifact and/or native classifier).
    pub fn files(&self, os: &str, arch: &str) -> Vec<LibFile> {
        let mut out = self.files_unchecked(os, arch);
        // Library paths come from remote version JSON: drop anything that is not a plain
        // relative path (`../..`, absolute, backslashes) so it can never leave `libraries/`.
        out.retain(|f| crate::settings::guard::is_safe_rel(&f.path));
        out
    }

    fn files_unchecked(&self, os: &str, arch: &str) -> Vec<LibFile> {
        if !rules_allow(&self.rules, os, arch) {
            return vec![];
        }
        let mut out = vec![];
        if let Some(d) = &self.downloads {
            if let Some(a) = &d.artifact {
                if !a.url.is_empty() {
                    let path = a.path.clone().or_else(|| maven_path(&self.name));
                    if let Some(path) = path {
                        out.push(LibFile { path, url: a.url.clone(), sha1: a.sha1.clone(), size: a.size });
                    }
                }
            }
            if let (Some(n), Some(c)) = (&self.natives, &d.classifiers) {
                if let Some(key) = n.get(os) {
                    let key = key.replace("${arch}", if arch == "x86" { "32" } else { "64" });
                    if let Some(i) = c.get(&key) {
                        if let Some(path) = i.path.clone().or_else(|| maven_path(&format!("{}:{}", self.name, key))) {
                            out.push(LibFile { path, url: i.url.clone(), sha1: i.sha1.clone(), size: i.size });
                        }
                    }
                }
            }
            if !out.is_empty() || d.artifact.is_some() {
                return out;
            }
        }
        // Loader style (no downloads block): maven base url + coordinate.
        if let Some(path) = maven_path(&self.name) {
            let base = self.url.clone().unwrap_or_else(|| "https://libraries.minecraft.net/".into());
            let base = if base.ends_with('/') { base } else { format!("{base}/") };
            out.push(LibFile { url: format!("{base}{path}"), path, sha1: None, size: None });
        }
        out
    }
}

#[derive(Debug, Deserialize)]
pub struct AssetIndex {
    pub objects: std::collections::BTreeMap<String, AssetObject>,
    #[serde(default, rename = "map_to_resources")]
    pub map_to_resources: bool,
    #[serde(default)]
    pub r#virtual: bool,
}

#[derive(Debug, Deserialize)]
pub struct AssetObject {
    pub hash: String,
    pub size: u64,
}

impl AssetObject {
    /// Asset hashes come from a downloaded index: only hex digits may become a path.
    pub fn is_valid(&self) -> bool {
        crate::settings::guard::is_hex_hash(&self.hash)
    }
    pub fn rel_path(&self) -> String {
        format!("{}/{}", self.hash.get(..2).unwrap_or("00"), self.hash)
    }
    pub fn url(&self) -> String {
        format!("https://resources.download.minecraft.net/{}", self.rel_path())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_manifest() {
        let j = r#"{"latest":{"release":"1.21","snapshot":"24w01a"},"versions":[
          {"id":"1.21","type":"release","url":"https://x/1.21.json","sha1":"ab","releaseTime":"2024"}]}"#;
        let m = VersionManifest::parse(j.as_bytes()).unwrap();
        assert_eq!(m.latest.release, "1.21");
        assert_eq!(m.find("1.21").unwrap().kind, "release");
        assert!(m.find("nope").is_none());
    }

    #[test]
    fn rules() {
        let r: Option<Vec<Rule>> = serde_json::from_str(r#"[{"action":"allow"},{"action":"disallow","os":{"name":"osx"}}]"#).unwrap();
        assert!(!rules_allow(&r, "osx", "arm64"));
        assert!(rules_allow(&r, "linux", "x86_64"));
        let r: Option<Vec<Rule>> = serde_json::from_str(r#"[{"action":"allow","os":{"name":"osx","arch":"arm64"}}]"#).unwrap();
        assert!(rules_allow(&r, "osx", "arm64"));
        assert!(!rules_allow(&r, "osx", "x86_64"));
        assert!(rules_allow(&None, "osx", "arm64"));
    }

    #[test]
    fn maven() {
        assert_eq!(maven_path("net.fabricmc:fabric-loader:0.15.0").unwrap(), "net/fabricmc/fabric-loader/0.15.0/fabric-loader-0.15.0.jar");
        assert_eq!(
            maven_path("org.lwjgl:lwjgl:3.3.3:natives-macos-arm64").unwrap(),
            "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-macos-arm64.jar"
        );
        assert_eq!(maven_path("a.b:c:1.0@zip").unwrap(), "a/b/c/1.0/c-1.0.zip");
        assert!(maven_path("bad").is_none());
    }

    #[test]
    fn library_files() {
        let l: Library = serde_json::from_str(
            r#"{"name":"org.lwjgl:lwjgl:3.3.3","downloads":{"artifact":{"path":"p/a.jar","url":"https://u/a.jar","sha1":"s","size":3}},
            "rules":[{"action":"allow","os":{"name":"osx"}}]}"#,
        )
        .unwrap();
        assert_eq!(l.files("osx", "arm64").len(), 1);
        assert!(l.files("linux", "x86_64").is_empty());
        let l: Library = serde_json::from_str(r#"{"name":"net.fabricmc:intermediary:1.20","url":"https://maven.fabricmc.net/"}"#).unwrap();
        let f = l.files("osx", "arm64");
        assert_eq!(f[0].url, "https://maven.fabricmc.net/net/fabricmc/intermediary/1.20/intermediary-1.20.jar");
        let l: Library = serde_json::from_str(
            r#"{"name":"o:n:1","downloads":{"classifiers":{"natives-osx":{"path":"o/n/1/n-1-natives-osx.jar","url":"https://u/n","sha1":"x","size":1}}},"natives":{"osx":"natives-osx"}}"#,
        )
        .unwrap();
        assert_eq!(l.files("osx", "arm64")[0].path, "o/n/1/n-1-natives-osx.jar");
    }

    #[test]
    fn asset_paths() {
        let o = AssetObject { hash: "abcdef0123".into(), size: 1 };
        assert_eq!(o.rel_path(), "ab/abcdef0123");
        assert!(o.url().ends_with("/ab/abcdef0123"));
        assert!(o.is_valid());
        // Hostile index entries: traversal, short and multi-byte hashes must not panic or be valid.
        for h in ["../../../../etc/cron.d/x", "a", "é", "日本語日本語", "ab/cd", ""] {
            let o = AssetObject { hash: h.into(), size: 1 };
            assert!(!o.is_valid(), "{h:?}");
            let _ = o.rel_path();
        }
    }

    #[test]
    fn maven_path_rejects_traversal() {
        assert!(maven_path("g:../../../../tmp:1").is_none());
        assert!(maven_path("g:a:../../..").is_none());
        assert!(maven_path("g:a:1:../../x").is_none());
        assert!(maven_path("g:a:1@../../x").is_none());
        assert!(maven_path("/abs:a:1").is_none());
        // A slash inside a segment only nests deeper; it cannot leave the libraries folder.
        assert!(maven_path("g:a/b:1").is_some_and(|p| !p.contains("..") && !p.starts_with('/')));
        assert!(maven_path("net.minecraftforge:forge:1.20.1-47.2.0:installer").is_some());
    }

    #[test]
    fn library_files_ignore_unsafe_paths() {
        let l: Library = serde_json::from_str(
            r#"{"name":"o:n:1","downloads":{"artifact":{"path":"../../../.zshrc","url":"https://u/a.jar","sha1":"s","size":3}}}"#,
        )
        .unwrap();
        assert!(l.files("osx", "arm64").is_empty());
        let l: Library = serde_json::from_str(
            r#"{"name":"o:n:1","downloads":{"classifiers":{"natives-osx":{"path":"/etc/x","url":"https://u/n","sha1":"x","size":1}}},"natives":{"osx":"natives-osx"}}"#,
        )
        .unwrap();
        assert!(l.files("osx", "arm64").is_empty());
    }
}
