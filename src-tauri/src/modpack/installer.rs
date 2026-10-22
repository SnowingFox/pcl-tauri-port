use super::*;

/// Run an install: game + loaders, overrides, then files. The caller supplies the engine.
/// Optional files are included only if their path is in `accepted_optional`.
pub async fn install(
    archive: &Path,
    plan: &ModpackInstallPlan,
    instance_dir: &Path,
    accepted_optional: &[String],
    engine: &dyn ModpackInstaller,
) -> Result<(), String> {
    std::fs::create_dir_all(instance_dir).map_err(|e| e.to_string())?;
    let files: Vec<PlanFile> = plan
        .files
        .iter()
        .filter(|f| !f.optional || accepted_optional.contains(&f.path))
        .cloned()
        .collect();
    if let Some(unresolved) = files.iter().find(|f| f.urls.is_empty()) {
        return Err(format!("file has no download URL (resolve the plan first): {}", unresolved.path));
    }
    engine.install_game(instance_dir, &plan.mc_version, &plan.loaders).await?;
    extract_overrides(archive, plan, instance_dir)?;
    engine.download_files(instance_dir, &files).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Stub(Mutex<Vec<String>>);
    #[async_trait::async_trait]
    impl ModpackInstaller for Stub {
        async fn download_files(&self, _: &Path, files: &[PlanFile]) -> Result<(), String> {
            self.0.lock().unwrap().push(format!("dl:{}", files.len()));
            Ok(())
        }
        async fn install_game(&self, _: &Path, mc: &str, l: &[LoaderSpec]) -> Result<(), String> {
            self.0.lock().unwrap().push(format!("game:{mc}:{}", l.len()));
            Ok(())
        }
    }

    #[test]
    fn orchestrates_in_order() {
        let idx = r#"{"name":"P","dependencies":{"minecraft":"1.20.1","forge":"47.1.0"},
          "files":[{"path":"mods/a.jar","hashes":{"sha1":"a"},"downloads":["https://cdn.modrinth.com/data/x/a"]},
                   {"path":"mods/o.jar","hashes":{"sha1":"o"},"downloads":["https://cdn.modrinth.com/data/x/o"],"env":{"client":"optional"}}]}"#;
        let dir = std::env::temp_dir().join(format!("pcl-inst-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let pack = dir.join("p.mrpack");
        std::fs::write(&pack, build_zip(&[("modrinth.index.json", idx), ("overrides/options.txt", "x")])).unwrap();
        let plan = parse_pack(&pack).unwrap();
        let stub = Stub::default();
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        rt.block_on(install(&pack, &plan, &dir.join("inst"), &[], &stub)).unwrap();
        assert_eq!(*stub.0.lock().unwrap(), vec!["game:1.20.1:1", "dl:1"]);
        assert!(dir.join("inst/options.txt").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
