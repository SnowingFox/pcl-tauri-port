use super::*;
use serde_json::Value;

pub fn parse(text: &str, _base: &str) -> Result<ModpackInstallPlan, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("invalid modrinth.index.json: {e}"))?;
    let deps = v["dependencies"].as_object().ok_or("Modrinth pack has no dependencies")?;
    let mc = deps.get("minecraft").and_then(Value::as_str).ok_or("Modrinth pack missing Minecraft version")?.to_string();
    let mut loaders = vec![];
    let mut warnings = vec![];
    for (k, val) in deps {
        let ver = val.as_str().unwrap_or_default().to_string();
        let l = match k.to_ascii_lowercase().as_str() {
            "minecraft" => continue,
            "forge" => ModLoader::Forge,
            "neoforge" | "neo-forge" => ModLoader::NeoForge,
            "fabric-loader" => ModLoader::Fabric,
            "quilt-loader" => ModLoader::Quilt,
            other => {
                warnings.push(format!("unsupported loader/dependency: {other}"));
                continue;
            }
        };
        loaders.push(LoaderSpec { loader: l, version: ver });
    }
    let mut files = vec![];
    for f in v["files"].as_array().map(Vec::as_slice).unwrap_or_default() {
        let client = f["env"]["client"].as_str().unwrap_or("required");
        if client == "unsupported" {
            continue;
        }
        let path = f["path"].as_str().ok_or("file entry without path")?.to_string();
        // validate now so a malicious pack fails at planning time
        safe_join(Path::new("/"), &path)?;
        let urls: Vec<String> = f["downloads"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
        for u in &urls {
            crate::settings::guard::check_download_url(u).map_err(|e| format!("{path}: {e}"))?;
        }
        files.push(PlanFile {
            path,
            urls,
            sha1: f["hashes"]["sha1"].as_str().map(str::to_string),
            // PCL: Modrinth packs should not be size-checked
            size: None,
            optional: client == "optional",
            curseforge: None,
        });
    }
    Ok(ModpackInstallPlan {
        format: PackFormat::Modrinth,
        name: v["name"].as_str().unwrap_or("Modrinth Pack").to_string(),
        version: v["versionId"].as_str().map(str::to_string),
        mc_version: mc,
        loaders,
        files,
        override_dirs: vec!["overrides".into(), "client-overrides".into()],
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const IDX: &str = r#"{"formatVersion":1,"game":"minecraft","versionId":"1.2","name":"My Pack",
      "files":[{"path":"mods/a.jar","hashes":{"sha1":"aa","sha512":"x"},"downloads":["https://cdn.modrinth.com/a.jar"],"fileSize":3,"env":{"client":"required","server":"required"}},
               {"path":"mods/b.jar","hashes":{"sha1":"bb"},"downloads":["https://cdn.modrinth.com/data/x/b.jar"],"env":{"client":"optional"}},
               {"path":"mods/c.jar","hashes":{"sha1":"cc"},"downloads":["https://cdn.modrinth.com/data/x/c.jar"],"env":{"client":"unsupported"}}],
      "dependencies":{"minecraft":"1.20.1","fabric-loader":"0.15.0","weird":"1"}}"#;

    #[test]
    fn parses_plan() {
        let p = parse(IDX, "").unwrap();
        assert_eq!(p.mc_version, "1.20.1");
        assert_eq!(p.loaders, vec![LoaderSpec { loader: ModLoader::Fabric, version: "0.15.0".into() }]);
        assert_eq!(p.files.len(), 2);
        assert!(p.files[1].optional);
        assert_eq!(p.warnings.len(), 1);
    }

    #[test]
    fn rejects_hostile_download_urls() {
        for bad in ["file:///etc/passwd", "http://cdn.modrinth.com/a.jar", "https://127.0.0.1/a.jar", "https://169.254.169.254/latest/meta-data", "https://user:pw@evil.example/a.jar"] {
            let idx = IDX.replacen("https://cdn.modrinth.com/a.jar", bad, 1);
            assert!(parse(&idx, "").is_err(), "{bad}");
        }
    }

    #[test]
    fn rejects_traversal() {
        let bad = IDX.replace("mods/a.jar", "../../a.jar");
        assert!(parse(&bad, "").is_err());
    }

    #[test]
    fn end_to_end_zip_and_overrides() {
        let zip = build_zip(&[("modrinth.index.json", IDX), ("overrides/config/x.toml", "k=1"), ("client-overrides/options.txt", "o"), ("overrides/../evil.txt", "e")]);
        let dir = std::env::temp_dir().join(format!("pcl-mrpack-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let pack = dir.join("p.mrpack");
        std::fs::write(&pack, zip).unwrap();
        let plan = parse_pack(&pack).unwrap();
        let inst = dir.join("inst");
        // the traversal entry must abort extraction
        assert!(extract_overrides(&pack, &plan, &inst).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
