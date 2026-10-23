use super::detect::*;
use crate::settings::InstanceSettings;
use crate::settings::IsolationMode;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

/// Find the version JSON of an instance: `<name>.json`, otherwise any `*.json`
/// that has `mainClass`, `type` and `id` (PCL `GetJsonPath`).
pub fn find_json(version_dir: &Path, name: &str) -> Result<PathBuf, String> {
    let direct = version_dir.join(format!("{name}.json"));
    if direct.is_file() {
        return Ok(direct);
    }
    let rd = fs::read_dir(version_dir).map_err(|e| format!("无法读取版本文件夹：{e}"))?;
    let mut candidates: Vec<PathBuf> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("json") && p.is_file())
        .collect();
    candidates.sort();
    for p in candidates {
        if let Ok(v) = read_json_value(&p)
            && v.get("mainClass").is_some()
            && v.get("type").is_some()
            && v.get("id").is_some()
        {
            return Ok(p);
        }
    }
    Err(format!("未找到版本 JSON 文件：{name}.json"))
}

fn read_json_value(p: &Path) -> Result<Value, String> {
    let text = fs::read_to_string(p).map_err(|e| e.to_string())?;
    serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())
}

/// Read the version JSON and merge its `inheritsFrom` chain, child entries first
/// in `libraries` (PCL `JsonObject`). Also converts the HMCL `patches` layout.
/// The direct parent id is stored in `_parentId`.
pub fn merge_json(root: &Path, name: &str) -> Result<(Value, Option<String>), String> {
    let mut visited = vec![name.to_string()];
    let dir = root.join("versions").join(name);
    let path = find_json(&dir, name)?;
    let mut cur = read_json_value(&path)?;
    if cur.get("patches").is_some() && cur.get("time").is_none() {
        cur = flatten_hmcl(cur, name);
    }
    let mut first_parent: Option<String> = None;
    loop {
        let parent = cur.get("inheritsFrom").and_then(Value::as_str).unwrap_or("").to_string();
        if parent.is_empty() || visited.contains(&parent) {
            break;
        }
        visited.push(parent.clone());
        if first_parent.is_none() {
            first_parent = Some(parent.clone());
        }
        let pdir = root.join("versions").join(&parent);
        let ppath = find_json(&pdir, &parent).map_err(|_| format!("需要安装 {parent} 作为前置版本"))?;
        let mut pjson = read_json_value(&ppath)?;
        let mut libs: Vec<Value> = cur.get("libraries").and_then(Value::as_array).cloned().unwrap_or_default();
        if let Some(pl) = pjson.get("libraries").and_then(Value::as_array) {
            libs.extend(pl.iter().cloned());
        }
        // child wins over parent for everything else
        if let (Some(c), Some(p)) = (cur.as_object(), pjson.as_object_mut()) {
            for (k, v) in c {
                if k != "inheritsFrom" {
                    p.insert(k.clone(), v.clone());
                }
            }
            p.insert("libraries".into(), Value::Array(libs));
            // keep walking up from the parent's own parent
            if let Some(pp) = pjson.get("inheritsFrom").cloned() {
                if let Some(o) = pjson.as_object_mut() {
                    o.insert("inheritsFrom".into(), pp);
                }
            } else if let Some(o) = pjson.as_object_mut() {
                o.remove("inheritsFrom");
            }
        }
        cur = pjson;
    }
    if let Some(p) = &first_parent {
        // Resolve the root vanilla id for MC version detection: the last visited id.
        let vanilla = visited.last().cloned().unwrap_or_else(|| p.clone());
        if let Some(o) = cur.as_object_mut() {
            o.insert("_parentId".into(), Value::String(vanilla));
        }
    }
    Ok((cur, first_parent))
}

fn flatten_hmcl(root: Value, name: &str) -> Value {
    let mut patches: Vec<Value> = root.get("patches").and_then(Value::as_array).cloned().unwrap_or_default();
    patches.sort_by_key(|p| p.get("priority").and_then(Value::as_i64).unwrap_or(0));
    let mut out: Option<Value> = None;
    for p in patches {
        if p.get("id").is_none() {
            continue;
        }
        match out.as_mut() {
            None => out = Some(p),
            Some(o) => {
                if let (Some(oo), Some(pp)) = (o.as_object_mut(), p.as_object()) {
                    let mut libs = oo.get("libraries").and_then(Value::as_array).cloned().unwrap_or_default();
                    if let Some(l) = pp.get("libraries").and_then(Value::as_array) {
                        libs.extend(l.iter().cloned());
                    }
                    for (k, v) in pp {
                        oo.insert(k.clone(), v.clone());
                    }
                    oo.insert("libraries".into(), Value::Array(libs));
                }
            }
        }
    }
    let mut v = out.unwrap_or(Value::Null);
    if let Some(o) = v.as_object_mut() {
        o.insert("id".into(), Value::String(name.into()));
        o.remove("inheritsFrom");
    }
    v
}

fn dir_has_files(p: &Path) -> bool {
    fs::read_dir(p).map(|mut r| r.any(|e| e.map(|e| e.path().is_file()).unwrap_or(false))).unwrap_or(false)
}
fn dir_has_subdirs(p: &Path) -> bool {
    fs::read_dir(p).map(|mut r| r.any(|e| e.map(|e| e.path().is_dir()).unwrap_or(false))).unwrap_or(false)
}

/// Decide version isolation (PCL `PathIndie` / `ShouldBeIndie`).
pub fn resolve_isolation(
    version_dir: &Path,
    settings: &InstanceSettings,
    global: IsolationMode,
    kind: InstanceKind,
    modable: bool,
) -> bool {
    if let Some(v) = settings.isolated {
        return v;
    }
    if dir_has_files(&version_dir.join("mods")) || dir_has_subdirs(&version_dir.join("saves")) {
        return true;
    }
    let is_release = !matches!(kind, InstanceKind::Fool | InstanceKind::Old | InstanceKind::Snapshot);
    match global {
        IsolationMode::Off => false,
        IsolationMode::ModdableOnly => modable,
        IsolationMode::NonReleaseOnly => !is_release,
        IsolationMode::ModdableAndNonRelease => !is_release || modable,
        IsolationMode::All => true,
    }
}

/// Load one instance. Never fails: problems are reported through `kind == Error`.
pub fn load_instance(root: &Path, name: &str, global_isolation: IsolationMode) -> InstanceInfo {
    let version_dir = root.join("versions").join(name);
    let settings = InstanceSettings::load(&version_dir);
    let mut info = InstanceInfo {
        name: name.to_string(),
        path: path_str(&version_dir),
        root: path_str(root),
        kind: InstanceKind::Error,
        mc_version: None,
        loaders: vec![],
        release_time: None,
        version_type: None,
        description: String::new(),
        category: Category::Error,
        icon: icon_for(InstanceKind::Error).into(),
        custom_logo_path: None,
        starred: settings.starred,
        hidden: settings.display_type == 1,
        modable: false,
        game_dir: path_str(root),
        isolated: false,
        inherits_from: None,
        error: None,
    };

    match merge_json(root, name) {
        Err(e) => info.error = Some(e),
        Ok((json, parent)) => {
            info.inherits_from = parent;
            if json.get("mainClass").and_then(Value::as_str).is_none() {
                info.error = Some("版本 JSON 中没有 mainClass 项！".into());
            } else {
                let loaders = detect_loaders(&json);
                let mc = detect_mc_version(&json, name);
                if mc.is_none() {
                    info.error = Some("无法获取版本号，结果为空".into());
                } else {
                    info.kind = classify_kind(&json, mc.as_deref(), &loaders, name);
                }
                info.release_time = json.get("releaseTime").and_then(Value::as_str).map(String::from);
                info.version_type = json.get("type").and_then(Value::as_str).map(String::from);
                info.modable = !loaders.is_empty() && !loaders.iter().all(|l| l.kind == LoaderKind::OptiFine);
                info.mc_version = mc;
                info.loaders = loaders;
            }
        }
    }

    info.description = if let Some(e) = &info.error {
        e.clone()
    } else if !settings.custom_info.is_empty() {
        settings.custom_info.clone()
    } else if info.kind == InstanceKind::Fool {
        fool_description(info.mc_version.as_deref().unwrap_or(name))
            .or_else(|| fool_description(name))
            .map(String::from)
            .unwrap_or_else(|| display_name(info.kind, info.mc_version.as_deref(), &info.loaders))
    } else {
        display_name(info.kind, info.mc_version.as_deref(), &info.loaders)
    };

    let custom_logo = version_dir.join("PCL").join("Logo.png");
    if settings.logo_custom && custom_logo.is_file() {
        info.icon = "custom".into();
        info.custom_logo_path = Some(path_str(&custom_logo));
    } else {
        info.icon = icon_for(info.kind).into();
    }
    info.category = category_for(info.kind, info.modable, settings.starred, settings.display_type);
    info.isolated = resolve_isolation(&version_dir, &settings, global_isolation, info.kind, info.modable);
    info.game_dir = if info.isolated { path_str(&version_dir) } else { path_str(root) };
    info
}

/// List all instances under `<root>/versions`. Sorted by name (case-insensitive).
/// A missing `versions` dir is an empty list, not an error.
pub fn scan_folder(root: &Path, global_isolation: IsolationMode) -> Vec<InstanceInfo> {
    let Ok(rd) = fs::read_dir(root.join("versions")) else { return vec![] };
    let mut names: Vec<String> = rd
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| !n.starts_with('.'))
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names.iter().map(|n| load_instance(root, n, global_isolation)).collect()
}

#[cfg(test)]
pub(crate) mod fixtures {
    use std::fs;
    use std::path::Path;

    pub fn write_version(root: &Path, name: &str, json: &str) {
        let d = root.join("versions").join(name);
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join(format!("{name}.json")), json).unwrap();
        fs::write(d.join(format!("{name}.jar")), b"jar").unwrap();
    }

    pub const VANILLA: &str = r#"{"id":"1.20.1","type":"release","mainClass":"net.minecraft.client.main.Main","releaseTime":"2023-06-12T13:25:51+00:00","libraries":[{"name":"com.mojang:brigadier:1.1.8"}]}"#;
    pub const SNAPSHOT: &str = r#"{"id":"23w31a","type":"snapshot","mainClass":"net.minecraft.client.main.Main","libraries":[]}"#;
    pub const FABRIC: &str = r#"{"id":"fabric-loader-0.15.0-1.20.1","inheritsFrom":"1.20.1","type":"release","mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient","libraries":[{"name":"net.fabricmc:fabric-loader:0.15.0"},{"name":"net.fabricmc:intermediary:1.20.1"}]}"#;
    pub const FORGE: &str = r#"{"id":"1.20.1-forge-47.2.0","inheritsFrom":"1.20.1","type":"release","mainClass":"cpw.mods.bootstraplauncher.BootstrapLauncher","arguments":{"game":["--launchTarget","forgeclient","--fml.forgeVersion","47.2.0","--fml.mcVersion","1.20.1"]},"libraries":[{"name":"net.minecraftforge:forge:1.20.1-47.2.0:universal"}]}"#;
    pub const OLD: &str = r#"{"id":"b1.7.3","type":"old_beta","mainClass":"net.minecraft.launchwrapper.Launch","libraries":[]}"#;
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;

    #[test]
    fn scans_and_classifies() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        write_version(r, "1.20.1", VANILLA);
        write_version(r, "23w31a", SNAPSHOT);
        write_version(r, "fab", FABRIC);
        write_version(r, "frg", FORGE);
        write_version(r, "b1.7.3", OLD);
        write_version(r, "15w14a", r#"{"id":"15w14a","type":"snapshot","mainClass":"x"}"#);
        fs::create_dir_all(r.join("versions/broken")).unwrap();
        fs::write(r.join("versions/broken/broken.json"), "{oops").unwrap();
        fs::create_dir_all(r.join("versions/empty")).unwrap();

        let list = scan_folder(r, IsolationMode::Off);
        let by = |n: &str| list.iter().find(|i| i.name == n).unwrap();

        assert_eq!(by("1.20.1").kind, InstanceKind::Release);
        assert_eq!(by("1.20.1").description, "正式版 1.20.1");
        assert_eq!(by("1.20.1").category, Category::Regular);
        assert_eq!(by("23w31a").kind, InstanceKind::Snapshot);
        assert_eq!(by("23w31a").category, Category::Rubbish);
        assert_eq!(by("b1.7.3").kind, InstanceKind::Old);
        assert_eq!(by("15w14a").kind, InstanceKind::Fool);
        assert_eq!(by("15w14a").category, Category::Fool);

        let fab = by("fab");
        assert_eq!(fab.kind, InstanceKind::Fabric);
        assert_eq!(fab.mc_version.as_deref(), Some("1.20.1"));
        assert_eq!(fab.loaders[0].version.as_deref(), Some("0.15.0"));
        assert_eq!(fab.category, Category::Modded);
        assert!(fab.modable);
        assert_eq!(fab.description, "正式版 1.20.1, Fabric 0.15.0");
        assert_eq!(fab.inherits_from.as_deref(), Some("1.20.1"));

        let frg = by("frg");
        assert_eq!(frg.kind, InstanceKind::Forge);
        assert_eq!(frg.loaders[0].version.as_deref(), Some("47.2.0"));

        assert_eq!(by("broken").kind, InstanceKind::Error);
        assert!(by("broken").error.is_some());
        assert_eq!(by("empty").kind, InstanceKind::Error);
    }

    #[test]
    fn missing_parent_is_reported() {
        let d = tempfile::tempdir().unwrap();
        write_version(d.path(), "fab", FABRIC);
        let i = load_instance(d.path(), "fab", IsolationMode::Off);
        assert_eq!(i.kind, InstanceKind::Error);
        assert_eq!(i.error.as_deref(), Some("需要安装 1.20.1 作为前置版本"));
    }

    #[test]
    fn isolation_rules() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        write_version(r, "1.20.1", VANILLA);
        write_version(r, "fab", FABRIC);
        assert!(!load_instance(r, "1.20.1", IsolationMode::ModdableOnly).isolated);
        assert!(load_instance(r, "fab", IsolationMode::ModdableOnly).isolated);
        assert!(load_instance(r, "1.20.1", IsolationMode::All).isolated);
        // mods folder with files forces isolation
        let mods = r.join("versions/1.20.1/mods");
        fs::create_dir_all(&mods).unwrap();
        fs::write(mods.join("a.jar"), b"x").unwrap();
        let i = load_instance(r, "1.20.1", IsolationMode::Off);
        assert!(i.isolated);
        assert!(i.game_dir.ends_with("versions/1.20.1"));
        // explicit override wins
        InstanceSettings { isolated: Some(false), ..Default::default() }.save(&r.join("versions/1.20.1")).unwrap();
        let i = load_instance(r, "1.20.1", IsolationMode::All);
        assert!(!i.isolated);
        assert_eq!(i.game_dir, r.to_string_lossy());
    }

    #[test]
    fn star_and_hidden_from_settings() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        write_version(r, "1.20.1", VANILLA);
        InstanceSettings { starred: true, ..Default::default() }.save(&r.join("versions/1.20.1")).unwrap();
        assert_eq!(load_instance(r, "1.20.1", IsolationMode::Off).category, Category::Star);
        InstanceSettings { display_type: 1, ..Default::default() }.save(&r.join("versions/1.20.1")).unwrap();
        let i = load_instance(r, "1.20.1", IsolationMode::Off);
        assert!(i.hidden);
        assert_eq!(i.category, Category::Hidden);
    }

    #[test]
    fn json_found_by_content_when_name_differs() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        let dir = r.join("versions/renamed");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("other.json"), VANILLA).unwrap();
        let i = load_instance(r, "renamed", IsolationMode::Off);
        assert_eq!(i.kind, InstanceKind::Release);
    }

    #[test]
    fn name_validation() {
        assert!(validate_name("ok name-1.20").is_ok());
        for bad in ["", " a", "a/b", "..", "a:b", "a."] {
            assert!(validate_name(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn self_inheritance_does_not_loop() {
        let d = tempfile::tempdir().unwrap();
        write_version(d.path(), "loop", r#"{"id":"loop","inheritsFrom":"loop","type":"release","mainClass":"x","libraries":[]}"#);
        let i = load_instance(d.path(), "loop", IsolationMode::Off);
        assert!(i.error.is_none() || i.mc_version.is_none());
    }
}
