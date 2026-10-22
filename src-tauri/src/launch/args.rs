//! Classpath assembly and JVM/game argument building.

use super::version::*;
use super::{AuthData, LaunchError, LaunchRequest};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const LAUNCHER_NAME: &str = "PCL-Tauri";
pub const LAUNCHER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Replace `${key}` placeholders. Unknown placeholders are left untouched.
pub fn replace_templates(input: &str, values: &HashMap<String, String>) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find('}') {
            Some(end) => {
                let key = &after[..end];
                match values.get(key) {
                    Some(v) => out.push_str(v),
                    None => {
                        out.push_str("${");
                        out.push_str(key);
                        out.push('}');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// Flatten argument items by evaluating rules.
pub fn filter_args(items: &[ArgItem], ctx: &RuleContext) -> Vec<String> {
    let mut out = Vec::new();
    for it in items {
        match it {
            ArgItem::Plain(s) => out.push(s.clone()),
            ArgItem::Conditional { rules, value } => {
                if rules_allow(rules, ctx) {
                    match value {
                        ArgValue::One(s) => out.push(s.clone()),
                        ArgValue::Many(v) => out.extend(v.iter().cloned()),
                    }
                }
            }
        }
    }
    out
}

/// Library jars on the classpath, deduplicated by group:artifact[:classifier]
/// (first occurrence wins, i.e. the child/loader version overrides vanilla).
pub fn classpath_libraries<'a>(v: &'a ResolvedVersion, ctx: &RuleContext) -> Vec<&'a Library> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for lib in &v.json.libraries {
        if !lib.allowed(ctx) || lib.classpath_rel_path().is_none() {
            continue;
        }
        if seen.insert(lib.key()) {
            out.push(lib);
        }
    }
    out
}

pub fn build_classpath(v: &ResolvedVersion, libraries_dir: &Path, client_jar: &Path, ctx: &RuleContext) -> Vec<PathBuf> {
    let mut cp: Vec<PathBuf> =
        classpath_libraries(v, ctx).iter().filter_map(|l| l.classpath_rel_path()).map(|rel| libraries_dir.join(rel)).collect();
    cp.push(client_jar.to_path_buf());
    cp
}

/// Forge / NeoForge 1.17 - 1.20.x start through `cpw.mods.bootstraplauncher`, whose
/// `-DignoreList=...,${version_name}.jar` makes it skip the client jar on the classpath (Forge loads
/// the real Minecraft module itself). If the classpath entry has any other name - e.g. the inherited
/// `1.20.1.jar` of a differently named instance - the vanilla jar is loaded a second time and the
/// game dies with "Module minecraft contains package ..., module _1._20._1 exports package ...".
/// PCL keeps such an instance's own `<name>.jar`; HMCL copies the vanilla jar to it. So do we.
pub fn client_jar_is_version_named(v: &ResolvedVersion) -> bool {
    let Some(a) = &v.json.arguments else { return false };
    a.jvm.iter().any(|item| match item {
        ArgItem::Plain(s) => s.contains("${version_name}.jar"),
        ArgItem::Conditional { value: ArgValue::One(s), .. } => s.contains("${version_name}.jar"),
        ArgItem::Conditional { value: ArgValue::Many(v), .. } => v.iter().any(|s| s.contains("${version_name}.jar")),
    })
}

/// Id of the version folder whose `<id>.jar` goes on the classpath.
pub fn client_jar_id(v: &ResolvedVersion) -> &str {
    if client_jar_is_version_named(v) { &v.id } else { &v.jar_id }
}

pub struct BuiltCommand {
    pub java: PathBuf,
    pub args: Vec<String>,
    pub working_dir: PathBuf,
    pub natives_dir: PathBuf,
}

impl BuiltCommand {
    /// Printable command line with the access token masked.
    pub fn display(&self, auth: &AuthData) -> String {
        let mut s = self.java.display().to_string();
        for a in &self.args {
            s.push(' ');
            let a = if !auth.access_token.is_empty() { a.replace(&auth.access_token, "***") } else { a.clone() };
            if a.contains(' ') { s.push_str(&format!("\"{a}\"")) } else { s.push_str(&a) }
        }
        s
    }
}

pub fn build_command(req: &LaunchRequest, v: &ResolvedVersion, java: &Path, ctx: &RuleContext) -> Result<BuiltCommand, LaunchError> {
    let mc = Path::new(&req.minecraft_dir);
    let libraries_dir = mc.join("libraries");
    let assets_root = mc.join("assets");
    let jar = version_jar_path(mc, client_jar_id(v));
    let game_dir = req.game_dir.as_ref().map(PathBuf::from).unwrap_or_else(|| mc.join("versions").join(&v.id));
    let natives_dir = mc.join("versions").join(&v.id).join(format!("{}-natives", v.id));
    let sep = if cfg!(windows) { ";" } else { ":" };
    let cp = build_classpath(v, &libraries_dir, &jar, ctx).iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(sep);

    let j = &v.json;
    let main_class = j.main_class.clone().ok_or_else(|| LaunchError::VersionJson("missing mainClass".into()))?;
    let asset_id = j.asset_index.as_ref().map(|a| a.id.clone()).or_else(|| j.assets.clone()).unwrap_or_else(|| "legacy".into());

    let mut vals: HashMap<String, String> = HashMap::new();
    let mut put = |k: &str, val: String| {
        vals.insert(k.to_string(), val);
    };
    put("auth_player_name", req.auth.name.clone());
    put("auth_uuid", req.auth.uuid.replace('-', "").to_lowercase());
    put("auth_access_token", req.auth.access_token.clone());
    put("auth_session", format!("token:{}:{}", req.auth.access_token, req.auth.uuid.replace('-', "")));
    put("user_type", if req.auth.user_type.is_empty() { "msa".into() } else { req.auth.user_type.clone() });
    put("user_properties", "{}".into());
    put("clientid", "".into());
    put("auth_xuid", "".into());
    put("version_name", v.id.clone());
    put("version_type", j.version_type.clone().unwrap_or_else(|| "release".into()));
    put("game_directory", game_dir.display().to_string());
    put("assets_root", assets_root.display().to_string());
    put("game_assets", assets_root.join("virtual").join("legacy").display().to_string());
    put("assets_index_name", asset_id);
    put("natives_directory", natives_dir.display().to_string());
    put("library_directory", libraries_dir.display().to_string());
    put("classpath_separator", sep.into());
    put("classpath", cp.clone());
    put("launcher_name", LAUNCHER_NAME.into());
    put("launcher_version", LAUNCHER_VERSION.into());
    if let Some((w, h)) = req.resolution {
        put("resolution_width", w.to_string());
        put("resolution_height", h.to_string());
    }

    let mut jvm: Vec<String> = Vec::new();
    let mut game: Vec<String> = Vec::new();
    match &j.arguments {
        Some(a) => {
            jvm = filter_args(&a.jvm, ctx);
            game = filter_args(&a.game, ctx);
            if let Some(legacy) = &j.minecraft_arguments {
                game.extend(legacy.split_whitespace().map(String::from));
            }
        }
        None => {
            if let Some(legacy) = &j.minecraft_arguments {
                game = legacy.split_whitespace().map(String::from).collect();
            }
        }
    }
    // Old format has no JVM args: add the mandatory ones.
    if j.arguments.is_none() || !jvm.iter().any(|a| a.contains("java.library.path")) {
        jvm.push("-Djava.library.path=${natives_directory}".into());
    }
    if !jvm.iter().any(|a| a == "-cp" || a == "-classpath") {
        jvm.push("-cp".into());
        jvm.push("${classpath}".into());
    }
    // LWJGL3 on macOS requires the first thread for GLFW.
    if ctx.os_name == "osx" && !jvm.iter().any(|a| a == "-XstartOnFirstThread") && needs_start_on_first_thread(v) {
        jvm.insert(0, "-XstartOnFirstThread".into());
    }

    let mut args: Vec<String> = Vec::new();
    if let Some(m) = req.max_memory_mb {
        args.push(format!("-Xmx{m}m"));
    }
    if let Some(m) = req.min_memory_mb {
        args.push(format!("-Xms{m}m"));
    }
    args.push("-Dfile.encoding=UTF-8".into());
    args.push("-Dstdout.encoding=UTF-8".into());
    args.push("-Djna.tmpdir=${natives_directory}".into());
    args.push("-Dorg.lwjgl.system.SharedLibraryExtractPath=${natives_directory}".into());
    args.extend(req.extra_jvm_args.iter().cloned());
    args.extend(jvm);
    args.push(main_class);
    args.extend(game);
    args.extend(req.extra_game_args.iter().cloned());
    if req.demo && !args.iter().any(|a| a == "--demo") {
        args.push("--demo".into());
    }
    if req.resolution.is_some() && !args.iter().any(|a| a == "--width") {
        args.extend(["--width".into(), "${resolution_width}".into(), "--height".into(), "${resolution_height}".into()]);
    }

    // Drop duplicate classpath flag pairs introduced by loaders is not attempted.
    let args = args.iter().map(|a| replace_templates(a, &vals)).collect();
    Ok(BuiltCommand { java: java.to_path_buf(), args, working_dir: game_dir, natives_dir })
}

/// LWJGL 3 (any `org.lwjgl:*:3.x` library) needs -XstartOnFirstThread on macOS.
/// Versions with the modern `arguments.jvm` already carry it via an osx rule,
/// but loader-merged/trimmed manifests sometimes drop it.
fn needs_start_on_first_thread(v: &ResolvedVersion) -> bool {
    v.json.libraries.iter().any(|l| matches!(l.maven_parts(), Some((g, _, ver, _, _)) if g == "org.lwjgl" && ver.starts_with('3')))
}

#[cfg(test)]
mod tests {
    use super::super::version::fixtures::*;
    use super::*;

    fn ctx(os: &str, arch: &str) -> RuleContext {
        RuleContext { os_name: os.into(), arch: arch.into(), features: HashMap::new() }
    }

    fn req() -> LaunchRequest {
        LaunchRequest {
            minecraft_dir: "/mc".into(),
            version_id: "1.20.1".into(),
            auth: AuthData {
                name: "Steve".into(),
                uuid: "123e4567-e89b-12d3-a456-426614174000".into(),
                access_token: "SECRET".into(),
                user_type: "msa".into(),
            },
            java_path: None,
            game_dir: None,
            max_memory_mb: Some(2048),
            min_memory_mb: None,
            extra_jvm_args: vec![],
            extra_game_args: vec![],
            demo: false,
            resolution: None,
        }
    }

    fn resolved(text: &str) -> ResolvedVersion {
        let j = parse_version_json(text).unwrap();
        ResolvedVersion { id: j.id.clone(), jar_id: j.id.clone(), root_id: j.id.clone(), json: j }
    }

    #[test]
    fn templates() {
        let mut m = HashMap::new();
        m.insert("a".into(), "1".into());
        assert_eq!(replace_templates("x${a}y${b}z${a", &m), "x1y${b}z${a");
    }

    #[test]
    fn modern_args_macos_arm64() {
        let v = resolved(VANILLA_MODERN);
        let c = build_command(&req(), &v, Path::new("/j/bin/java"), &ctx("osx", "arm64")).unwrap();
        let a = &c.args;
        assert!(a.contains(&"-Xmx2048m".to_string()));
        assert_eq!(a.iter().filter(|x| *x == "-XstartOnFirstThread").count(), 1);
        assert!(!a.iter().any(|x| x.contains("-Dwin")));
        assert!(a.contains(&"-Djava.library.path=/mc/versions/1.20.1/1.20.1-natives".to_string()));
        let cp = &a[a.iter().position(|x| x == "-cp").unwrap() + 1];
        assert!(cp.contains("lwjgl-3.3.1-natives-macos-arm64.jar"));
        assert!(!cp.contains("natives-windows"));
        assert!(cp.ends_with("/mc/versions/1.20.1/1.20.1.jar"));
        let mi = a.iter().position(|x| x == "net.minecraft.client.main.Main").unwrap();
        assert_eq!(&a[mi + 1..mi + 5], ["--username", "Steve", "--version", "1.20.1"]);
        assert!(!a.contains(&"--demo".to_string()));
        assert!(a.iter().all(|x| !x.contains("${auth")));
    }

    #[test]
    fn x64_excludes_arm64_native() {
        let v = resolved(VANILLA_MODERN);
        let cp = build_classpath(&v, Path::new("/l"), Path::new("/c.jar"), &ctx("osx", "x64"));
        let s: Vec<String> = cp.iter().map(|p| p.display().to_string()).collect();
        assert!(!s.iter().any(|p| p.contains("arm64")));
        assert!(s.iter().any(|p| p.contains("natives-macos.jar")));
    }

    #[test]
    fn legacy_args() {
        let v = resolved(LEGACY);
        let mut r = req();
        r.version_id = "1.7.10".into();
        let c = build_command(&r, &v, Path::new("java"), &ctx("osx", "x64")).unwrap();
        let a = &c.args;
        assert!(a.contains(&"-cp".to_string()));
        assert!(a.iter().any(|x| x.starts_with("-Djava.library.path=")));
        // LWJGL2: no -XstartOnFirstThread
        assert!(!a.contains(&"-XstartOnFirstThread".to_string()));
        assert!(a.contains(&"token:SECRET:123e4567e89b12d3a456426614174000".to_string()));
        assert!(c.display(&r.auth).contains("***"));
        assert!(!c.display(&r.auth).contains("SECRET"));
    }

    /// Shape of the Forge 1.17-1.20.x loader json: BootstrapLauncher + ignoreList naming the client jar.
    const FORGE_BOOTSTRAP_CHILD: &str = r#"{
      "id": "My Forge", "inheritsFrom": "1.20.1", "type": "release",
      "mainClass": "cpw.mods.bootstraplauncher.BootstrapLauncher",
      "arguments": {"game": ["--launchTarget", "forgeclient"],
        "jvm": ["-DignoreList=bootstraplauncher,securejarhandler,forge-,${version_name}.jar", "-DlibraryDirectory=${library_directory}"]},
      "libraries": [{"name": "net.minecraftforge:fmlloader:1.20.1-47.4.28"}]}"#;

    fn resolved_forge(id: &str) -> ResolvedVersion {
        let child = parse_version_json(&FORGE_BOOTSTRAP_CHILD.replace("My Forge", id)).unwrap();
        let parent = parse_version_json(VANILLA_MODERN).unwrap();
        resolve_with(id, &|i| Ok(if i == "1.20.1" { parent.clone() } else { child.clone() })).unwrap()
    }

    #[test]
    fn bootstrap_launcher_gets_a_classpath_jar_named_after_the_version() {
        let v = resolved_forge("My Forge");
        assert_eq!(v.jar_id, "1.20.1", "the client jar is inherited from vanilla ...");
        assert!(client_jar_is_version_named(&v));
        let c = build_command(&req(), &v, Path::new("/j/bin/java"), &ctx("osx", "arm64")).unwrap();
        let cp = &c.args[c.args.iter().position(|x| x == "-cp").unwrap() + 1];
        assert!(cp.ends_with("/mc/versions/My Forge/My Forge.jar"), "... but the classpath entry must match -DignoreList: {cp}");
        assert!(c.args.iter().any(|a| a.starts_with("-DignoreList=") && a.ends_with(",My Forge.jar")));
        // a plain vanilla / Fabric style version keeps using the inherited jar
        let vanilla = resolved(VANILLA_MODERN);
        assert!(!client_jar_is_version_named(&vanilla));
        assert_eq!(client_jar_id(&vanilla), "1.20.1");
    }

    #[test]
    fn demo_and_resolution() {
        let v = resolved(VANILLA_MODERN);
        let mut r = req();
        r.demo = true;
        r.resolution = Some((854, 480));
        let c = build_command(&r, &v, Path::new("java"), &ctx("osx", "arm64")).unwrap();
        assert!(c.args.contains(&"--demo".to_string()));
        assert!(c.args.contains(&"854".to_string()));
    }

    #[test]
    fn library_dedup_prefers_first() {
        let child = parse_version_json(FORGE_CHILD).unwrap();
        let parent = parse_version_json(VANILLA_MODERN).unwrap();
        let r = resolve_with("1.20.1-forge", &|id| Ok(if id == "1.20.1" { parent.clone() } else { child.clone() })).unwrap();
        let libs = classpath_libraries(&r, &ctx("osx", "arm64"));
        let logging: Vec<_> = libs.iter().filter(|l| l.key() == "com.mojang:logging").collect();
        assert_eq!(logging.len(), 1);
        assert!(logging[0].name.ends_with("1.2.0"));
    }
}
