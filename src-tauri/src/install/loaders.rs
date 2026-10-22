//! Fabric, Quilt (profile-json loaders), OptiFine (mod download), and loader version lists.

use super::vanilla::*;
use crate::download::engine::FileSpec;
use crate::download::error::{DlError, DlResult};
use crate::download::manifest::VersionJson;
use crate::download::mirror::Source;
use crate::download::{fetch_bytes, Manager, TaskCtx};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, Copy, Debug)]
pub enum ProfileLoader {
    Fabric,
    Quilt,
}

impl ProfileLoader {
    fn meta(&self) -> &'static str {
        match self {
            Self::Fabric => "https://meta.fabricmc.net/v2",
            Self::Quilt => "https://meta.quiltmc.org/v3",
        }
    }
    #[allow(dead_code)]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Fabric => "fabric",
            Self::Quilt => "quilt",
        }
    }
}

pub fn profile_url(l: ProfileLoader, mc: &str, loader: &str) -> String {
    format!("{}/versions/loader/{}/{}/profile/json", l.meta(), mc, loader)
}

/// Fabric/Quilt: ensure vanilla, write the loader profile json (inheritsFrom = mc), fetch libraries.
/// Returns the new version id.
pub async fn install_profile_loader(ctx: &TaskCtx, mgr: &Arc<Manager>, l: ProfileLoader, mc: &str, loader: &str) -> DlResult<String> {
    let gd = ctx.cfg.game_dir.clone();
    install_vanilla(ctx, mgr, mc, 0.0, 0.85).await?;
    ctx.tracker.begin_stage("loader-profile", 0.85, 0.02, 1, 0);
    let bytes = ctx.get_bytes(&profile_url(l, mc, loader)).await?;
    let raw: serde_json::Value = serde_json::from_slice(&bytes)?;
    let id = raw["id"].as_str().ok_or("loader profile has no id")?.to_string();
    if raw["inheritsFrom"].as_str().is_none() {
        return Err("loader profile lacks inheritsFrom".into());
    }
    let path = version_json_path(&gd, &id);
    tokio::fs::create_dir_all(path.parent().unwrap()).await?;
    tokio::fs::write(&path, serde_json::to_vec_pretty(&raw)?).await?;
    let vj: VersionJson = serde_json::from_value(raw)?;
    install_libraries(ctx, &vj, 0.87, 0.13, "loader-libraries").await?;
    Ok(id)
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct LoaderVersion {
    pub version: String,
    pub stable: Option<bool>,
}

/// Parse Fabric/Quilt `versions/loader/{mc}` response.
pub fn parse_profile_loader_list(bytes: &[u8]) -> DlResult<Vec<LoaderVersion>> {
    let v: serde_json::Value = serde_json::from_slice(bytes)?;
    Ok(v.as_array()
        .ok_or("expected array")?
        .iter()
        .filter_map(|e| {
            Some(LoaderVersion { version: e["loader"]["version"].as_str()?.to_string(), stable: e["loader"]["stable"].as_bool() })
        })
        .collect())
}

/// Parse BMCLAPI `/forge/minecraft/{mc}` (newest first after sort).
pub fn parse_forge_list(bytes: &[u8]) -> DlResult<Vec<LoaderVersion>> {
    let v: serde_json::Value = serde_json::from_slice(bytes)?;
    let mut out: Vec<(i64, String)> = v
        .as_array()
        .ok_or("expected array")?
        .iter()
        .filter_map(|e| Some((e["build"].as_i64().unwrap_or(0), e["version"].as_str()?.to_string())))
        .collect();
    out.sort_by_key(|e| std::cmp::Reverse(e.0));
    Ok(out.into_iter().map(|(_, version)| LoaderVersion { version, stable: None }).collect())
}

/// NeoForge versions are `<minor>.<patch>.<build>[-beta]` for MC `1.<minor>.<patch>`.
pub fn neoforge_prefix(mc: &str) -> Option<String> {
    let mut it = mc.split('.');
    if it.next()? != "1" {
        return None;
    }
    let minor: u32 = it.next()?.parse().ok()?;
    let patch: u32 = it.next().map(|p| p.parse().ok()).unwrap_or(Some(0))?;
    Some(format!("{minor}.{patch}."))
}

pub fn parse_neoforge_list(bytes: &[u8], mc: &str) -> DlResult<Vec<LoaderVersion>> {
    let v: serde_json::Value = serde_json::from_slice(bytes)?;
    let pre = neoforge_prefix(mc).ok_or("unsupported Minecraft version for NeoForge")?;
    let mut out: Vec<LoaderVersion> = v["versions"]
        .as_array()
        .ok_or("expected versions array")?
        .iter()
        .filter_map(|s| s.as_str())
        .filter(|s| s.starts_with(&pre))
        .map(|s| LoaderVersion { version: s.to_string(), stable: Some(!s.contains('-')) })
        .collect();
    out.reverse();
    Ok(out)
}

pub async fn list_loader_versions(m: &Manager, loader: &str, mc: &str) -> DlResult<Vec<LoaderVersion>> {
    let src = m.cfg().source;
    match loader {
        "fabric" => parse_profile_loader_list(&fetch_bytes(&m.client, src, &format!("{}/versions/loader/{mc}", ProfileLoader::Fabric.meta())).await?),
        "quilt" => parse_profile_loader_list(&fetch_bytes(&m.client, src, &format!("{}/versions/loader/{mc}", ProfileLoader::Quilt.meta())).await?),
        // Forge has no official list API; BMCLAPI is the only JSON source, used regardless of the mirror setting.
        "forge" => parse_forge_list(&fetch_bytes(&m.client, Source::Official, &format!("https://bmclapi2.bangbang93.com/forge/minecraft/{mc}")).await?),
        "neoforge" => parse_neoforge_list(
            &fetch_bytes(&m.client, Source::Official, "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge").await?,
            mc,
        ),
        other => Err(DlError::Other(format!("unknown loader {other}"))),
    }
}

#[derive(Clone, Debug, Serialize, serde::Deserialize)]
pub struct OptifineVersion {
    pub mcversion: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub patch: String,
    pub filename: String,
}

pub async fn list_optifine(m: &Manager, mc: &str) -> DlResult<Vec<OptifineVersion>> {
    let b = fetch_bytes(&m.client, Source::Official, &format!("https://bmclapi2.bangbang93.com/optifine/{mc}")).await?;
    Ok(serde_json::from_slice(&b)?)
}

/// OptiFine as a Forge/standalone-mod jar dropped into `mods_dir`.
/// INCOMPLETE: the standalone launchwrapper-profile install (running OptiFine's patcher) is not implemented.
pub async fn install_optifine_mod(ctx: &TaskCtx, mc: &str, kind: &str, patch: &str, mods_dir: PathBuf) -> DlResult<PathBuf> {
    let dest = mods_dir.join(format!("OptiFine_{mc}_{kind}_{patch}.jar"));
    ctx.tracker.begin_stage("optifine", 0.0, 1.0, 1, 0);
    // BMCLAPI is the only scriptable source; use it directly (no official URL exists).
    ctx.file(FileSpec::new(format!("https://bmclapi2.bangbang93.com/optifine/{mc}/{kind}/{patch}"), &dest)).await?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls() {
        assert_eq!(
            profile_url(ProfileLoader::Fabric, "1.20.1", "0.15.0"),
            "https://meta.fabricmc.net/v2/versions/loader/1.20.1/0.15.0/profile/json"
        );
        assert!(profile_url(ProfileLoader::Quilt, "1.20.1", "0.2").starts_with("https://meta.quiltmc.org/v3/"));
    }

    #[test]
    fn lists() {
        let l = parse_profile_loader_list(br#"[{"loader":{"version":"0.15.1","stable":true}},{"loader":{"version":"0.15.0-beta","stable":false}}]"#).unwrap();
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].version, "0.15.1");
        let f = parse_forge_list(br#"[{"version":"47.1.0","build":10},{"version":"47.2.0","build":20}]"#).unwrap();
        assert_eq!(f[0].version, "47.2.0");
    }

    #[test]
    fn neoforge() {
        assert_eq!(neoforge_prefix("1.20.4").unwrap(), "20.4.");
        assert_eq!(neoforge_prefix("1.21").unwrap(), "21.0.");
        assert!(neoforge_prefix("24w01a").is_none());
        let l = parse_neoforge_list(br#"{"versions":["20.4.1","20.4.2-beta","21.0.5","20.40.1"]}"#, "1.20.4").unwrap();
        assert_eq!(l.iter().map(|v| v.version.as_str()).collect::<Vec<_>>(), vec!["20.4.2-beta", "20.4.1"]);
        assert_eq!(l[0].stable, Some(false));
    }
}
