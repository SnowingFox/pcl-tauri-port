//! MultiMC / Prism instance exports (`mmc-pack.json` + `.minecraft/`).
use super::*;
use serde_json::Value;

pub fn parse(pack_json: &str, instance_cfg: Option<&str>, base: &str) -> Result<ModpackInstallPlan, String> {
    let v: Value = serde_json::from_str(pack_json).map_err(|e| format!("invalid mmc-pack.json: {e}"))?;
    let mut mc = None;
    let mut loaders = vec![];
    let mut warnings = vec![];
    for c in v["components"].as_array().map(Vec::as_slice).unwrap_or_default() {
        let uid = c["uid"].as_str().unwrap_or_default();
        let ver = c["version"].as_str().unwrap_or_default().to_string();
        let l = match uid {
            "net.minecraft" => { mc = Some(ver); continue }
            "net.minecraftforge" => ModLoader::Forge,
            "net.neoforged" => ModLoader::NeoForge,
            "net.fabricmc.fabric-loader" => ModLoader::Fabric,
            "org.quiltmc.quilt-loader" => ModLoader::Quilt,
            // intermediary mappings / LWJGL are implied by the above
            "net.fabricmc.intermediary" | "org.quiltmc.hashed" | "org.lwjgl" | "org.lwjgl3" => continue,
            other => { warnings.push(format!("ignored component: {other}")); continue }
        };
        loaders.push(LoaderSpec { loader: l, version: ver });
    }
    let name = instance_cfg
        .and_then(|c| c.lines().find_map(|l| l.strip_prefix("name=")))
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "MultiMC Pack".into());
    Ok(ModpackInstallPlan {
        format: PackFormat::MultiMc,
        name,
        version: None,
        mc_version: mc.ok_or("MultiMC pack missing net.minecraft component")?,
        loaders,
        files: vec![],
        override_dirs: vec![format!("{base}.minecraft"), format!("{base}minecraft")],
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses() {
        let p = parse(
            r#"{"formatVersion":1,"components":[{"uid":"net.minecraft","version":"1.20.1"},{"uid":"net.fabricmc.fabric-loader","version":"0.15.0"},{"uid":"net.fabricmc.intermediary","version":"1.20.1"},{"uid":"x.y"}]}"#,
            Some("InstanceType=OneSix\nname=Cool\n"), "inst/")
            .unwrap();
        assert_eq!(p.name, "Cool");
        assert_eq!(p.mc_version, "1.20.1");
        assert_eq!(p.loaders[0].loader, ModLoader::Fabric);
        assert_eq!(p.override_dirs[0], "inst/.minecraft");
        assert_eq!(p.warnings.len(), 1);
    }
}
