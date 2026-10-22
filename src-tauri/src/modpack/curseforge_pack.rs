use super::*;
use crate::resources::{curseforge, types::ResourceFile};
use serde_json::Value;

pub fn parse(text: &str, base: &str) -> Result<ModpackInstallPlan, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("invalid manifest.json: {e}"))?;
    let mc = v["minecraft"]["version"].as_str().ok_or("CurseForge pack missing Minecraft version")?.to_string();
    let mut loaders = vec![];
    let mut warnings = vec![];
    for l in v["minecraft"]["modLoaders"].as_array().map(Vec::as_slice).unwrap_or_default() {
        let id = l["id"].as_str().unwrap_or_default().to_ascii_lowercase();
        if id.contains("recommended") {
            return Err("this modpack is too old (uses 'recommended' loader versions) and is unsupported".into());
        }
        let parsed = [
            ("forge-", ModLoader::Forge),
            ("neoforge-", ModLoader::NeoForge),
            ("fabric-", ModLoader::Fabric),
            ("quilt-", ModLoader::Quilt),
        ]
        .into_iter()
        .find_map(|(p, ml)| id.strip_prefix(p).map(|ver| LoaderSpec { loader: ml, version: ver.to_string() }));
        match parsed {
            Some(s) => loaders.push(s),
            None => warnings.push(format!("unsupported loader: {id}")),
        }
    }
    let mut files = vec![];
    for f in v["files"].as_array().map(Vec::as_slice).unwrap_or_default() {
        let (Some(p), Some(fid)) = (f["projectID"].as_u64(), f["fileID"].as_u64()) else {
            warnings.push(format!("skipped file entry missing ids: {f}"));
            continue;
        };
        files.push(PlanFile {
            path: String::new(), // determined at resolve time (mods/, resourcepacks/, ...)
            urls: vec![],
            sha1: None,
            size: None,
            optional: f["required"].as_bool() == Some(false),
            curseforge: Some(CurseForgeRef { project_id: p, file_id: fid }),
        });
    }
    let ov = v["overrides"].as_str().unwrap_or("overrides");
    let ov = if ov == "." || ov == "./" { "" } else { ov.trim_start_matches("./") };
    safe_join(Path::new("/"), ov)?;
    Ok(ModpackInstallPlan {
        format: PackFormat::CurseForge,
        name: v["name"].as_str().unwrap_or("CurseForge Pack").to_string(),
        version: v["version"].as_str().map(str::to_string),
        mc_version: mc,
        loaders,
        files,
        override_dirs: vec![format!("{base}{ov}")].into_iter().map(|d| d.trim_end_matches('/').to_string()).collect(),
        warnings,
    })
}

/// Target folder by file name (CurseForge's `modules` heuristic from PCL is approximated by
/// extension: .jar -> mods, .zip -> resourcepacks unless class says otherwise).
pub fn target_folder(file_name: &str) -> &'static str {
    let l = file_name.to_ascii_lowercase();
    if l.ends_with(".jar") { "mods" } else { "resourcepacks" }
}

/// Fill in paths/URLs for CurseForge refs via the API. Files deleted upstream are dropped and
/// listed in the returned `missing` vector so the UI can warn (PCL asks "continue?").
pub async fn resolve(plan: &mut ModpackInstallPlan) -> Result<Vec<CurseForgeRef>, String> {
    let ids: Vec<u64> = plan.files.iter().filter_map(|f| f.curseforge.as_ref().map(|c| c.file_id)).collect();
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let mut found: Vec<ResourceFile> = vec![];
    for chunk in ids.chunks(200) {
        found.extend(curseforge::files_by_ids(chunk).await?);
    }
    Ok(apply_resolved(plan, found))
}

pub fn apply_resolved(plan: &mut ModpackInstallPlan, found: Vec<ResourceFile>) -> Vec<CurseForgeRef> {
    let by_id: std::collections::HashMap<u64, &ResourceFile> = found.iter().filter_map(|f| f.id.parse().ok().map(|i| (i, f))).collect();
    let mut missing = vec![];
    let mut seen = std::collections::HashSet::new();
    let mut kept = vec![];
    for mut pf in std::mem::take(&mut plan.files) {
        let Some(r) = pf.curseforge.clone() else {
            kept.push(pf);
            continue;
        };
        if !seen.insert(r.file_id) {
            continue; // CurseForge sometimes lists duplicates
        }
        // `fileName` comes from the API: it becomes a path below the instance, so it must be a
        // single plain component. Anything else is reported like a file that no longer exists.
        let Some(rf) = by_id.get(&r.file_id).filter(|rf| crate::settings::guard::is_safe_component(&rf.file_name)) else {
            missing.push(r);
            continue;
        };
        pf.path = format!("{}/{}", target_folder(&rf.file_name), rf.file_name);
        pf.sha1 = rf.sha1.clone();
        pf.size = Some(rf.size);
        pf.urls = if rf.download_urls.is_empty() {
            vec![curseforge::fallback_download_url(r.file_id, &rf.file_name)]
        } else {
            rf.download_urls.clone()
        };
        kept.push(pf);
    }
    plan.files = kept;
    missing
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::types::Platform;
    const MF: &str = r#"{"minecraft":{"version":"1.20.1","modLoaders":[{"id":"forge-47.2.0","primary":true}]},"manifestType":"minecraftModpack",
      "name":"CF Pack","version":"3","overrides":"overrides","files":[{"projectID":1,"fileID":10,"required":true},{"projectID":2,"fileID":20,"required":false},{"projectID":3,"fileID":30},{"projectID":9}]}"#;

    fn rf(id: u64, name: &str) -> ResourceFile {
        ResourceFile {
            platform: Platform::CurseForge,
            id: id.to_string(),
            project_id: "1".into(),
            name: name.into(),
            file_name: name.into(),
            version_number: None,
            release_type: "release".into(),
            game_versions: vec![],
            loaders: vec![],
            sha1: Some("h".into()),
            size: 5,
            download_urls: vec![],
            date_published: String::new(),
            dependencies: vec![],
            fingerprint: None,
        }
    }

    #[test]
    fn parse_and_resolve() {
        let mut p = parse(MF, "").unwrap();
        assert_eq!(p.loaders, vec![LoaderSpec { loader: ModLoader::Forge, version: "47.2.0".into() }]);
        assert_eq!(p.files.len(), 3);
        assert!(p.files[1].optional);
        assert_eq!(p.warnings.len(), 1);
        assert_eq!(p.override_dirs, vec!["overrides"]);
        let missing = apply_resolved(&mut p, vec![rf(10, "a.jar"), rf(20, "pack.zip")]);
        assert_eq!(missing, vec![CurseForgeRef { project_id: 3, file_id: 30 }]);
        assert_eq!(p.files[0].path, "mods/a.jar");
        assert_eq!(p.files[1].path, "resourcepacks/pack.zip");
        assert!(p.files[0].urls[0].starts_with("https://edge.forgecdn.net/files/0/10/"));
    }

    #[test]
    fn hostile_api_file_names_are_not_installed() {
        let mut p = parse(MF, "").unwrap();
        let missing = apply_resolved(&mut p, vec![rf(10, "../../../../.zshrc"), rf(20, "ok.zip"), rf(30, "a/b.jar")]);
        // 10 and 30 carry path separators and are treated like files that no longer exist upstream.
        assert_eq!(missing.iter().map(|m| m.file_id).collect::<Vec<_>>(), vec![10, 30]);
        assert_eq!(p.files.len(), 1);
        assert_eq!(p.files[0].path, "resourcepacks/ok.zip");
    }

    #[test]
    fn old_recommended_rejected() {
        assert!(parse(&MF.replace("forge-47.2.0", "forge-recommended"), "").is_err());
    }

    #[test]
    fn dot_overrides_and_base() {
        let p = parse(&MF.replace("\"overrides\":\"overrides\"", "\"overrides\":\".\""), "Pack/").unwrap();
        assert_eq!(p.override_dirs, vec!["Pack"]);
    }
}
