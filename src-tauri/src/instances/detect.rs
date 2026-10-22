use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::sync::OnceLock;

/// Mirrors PCL `McInstanceState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceKind {
    Error,
    Release,
    Snapshot,
    Fool,
    Old,
    Forge,
    NeoForge,
    Fabric,
    Quilt,
    OptiFine,
    LiteLoader,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoaderKind {
    Forge,
    NeoForge,
    Fabric,
    Quilt,
    OptiFine,
    LiteLoader,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Loader {
    pub kind: LoaderKind,
    /// `None` when the version could not be determined.
    pub version: Option<String>,
}

/// Mirrors PCL `McInstanceCardType`, used to group the instance list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Star,
    Hidden,
    /// Has a mod loader (PCL "API").
    Modded,
    Regular,
    /// Snapshots and ancient versions.
    Rubbish,
    Fool,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceInfo {
    pub name: String,
    /// Absolute path of `<root>/versions/<name>`.
    pub path: String,
    pub root: String,
    pub kind: InstanceKind,
    pub mc_version: Option<String>,
    pub loaders: Vec<Loader>,
    pub release_time: Option<String>,
    pub version_type: Option<String>,
    /// Human readable one-liner, e.g. "正式版 1.20.1, Fabric 0.15.0".
    pub description: String,
    pub category: Category,
    /// Icon id (`grass`, `anvil`, ...) or `custom` when `custom_logo_path` is set.
    pub icon: String,
    pub custom_logo_path: Option<String>,
    pub starred: bool,
    pub hidden: bool,
    pub modable: bool,
    /// Game working directory after applying version isolation.
    pub game_dir: String,
    pub isolated: bool,
    pub inherits_from: Option<String>,
    pub error: Option<String>,
}

fn re(cell: &'static OnceLock<Regex>, pat: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pat).unwrap())
}

pub fn is_snapshot_name(id: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    let l = id.to_lowercase();
    re(&R, r"^\d{2}w\d{2}[a-z~]").is_match(&l)
        || l.contains("-pre")
        || l.contains("-rc")
        || l.contains("snapshot")
        || l.contains("experimental")
        || l == "pending"
}

/// April-fools releases (PCL `GetMcFoolName`).
pub fn fool_description(id: &str) -> Option<&'static str> {
    let n = id.to_lowercase();
    Some(if n.starts_with("2.0") {
        "2013 | 这个秘密计划了两年的更新将游戏推向了一个新高度！"
    } else if n == "15w14a" {
        "2015 | 作为一款全年龄向的游戏，我们需要和平，需要爱与拥抱。"
    } else if n == "1.rv-pre1" {
        "2016 | 是时候将现代科技带入 Minecraft 了！"
    } else if n == "3d shareware v1.34" {
        "2019 | 我们从地下室的废墟里找到了这个开发于 1994 年的杰作！"
    } else if n.starts_with("20w14inf") || n == "20w14∞" {
        "2020 | 我们加入了 20 亿个新的维度，让无限的想象变成了现实！"
    } else if n == "22w13oneblockatatime" {
        "2022 | 一次一个方块更新！迎接全新的挖掘、合成与骑乘玩法吧！"
    } else if n == "23w13a_or_b" {
        "2023 | 研究表明：玩家喜欢作出选择——越多越好！"
    } else if n == "24w14potato" {
        "2024 | 毒马铃薯一直都被大家忽视和低估，于是我们超级加强了它！"
    } else if n == "25w14craftmine" {
        "2025 | 你可以合成任何东西——包括合成你的世界！"
    } else if n == "26w14a" {
        "2026 | 为什么需要物品栏？让方块们跟着你走吧！"
    } else {
        return None;
    })
}

/// Parse a Maven coordinate `group:artifact:version[:classifier]`.
fn coord(name: &str) -> Option<(&str, &str, &str)> {
    let mut it = name.split(':');
    Some((it.next()?, it.next()?, it.next()?))
}

/// Detect loaders from the (already inheritance-merged) version JSON.
fn add_loader(out: &mut Vec<Loader>, kind: LoaderKind, version: Option<String>) {
    if let Some(existing) = out.iter_mut().find(|l| l.kind == kind) {
        if existing.version.is_none() {
            existing.version = version;
        }
    } else {
        out.push(Loader { kind, version });
    }
}

pub fn detect_loaders(json: &Value) -> Vec<Loader> {
    let mut out: Vec<Loader> = Vec::new();
    if let Some(libs) = json.get("libraries").and_then(Value::as_array) {
        for lib in libs {
            let Some(name) = lib.get("name").and_then(Value::as_str) else { continue };
            let Some((group, artifact, version)) = coord(name) else { continue };
            match (group, artifact) {
                ("net.fabricmc", "fabric-loader") => add_loader(&mut out, LoaderKind::Fabric, Some(version.into())),
                ("org.quiltmc", "quilt-loader") => add_loader(&mut out, LoaderKind::Quilt, Some(version.into())),
                ("net.neoforged", "neoforge") | ("net.neoforged.fancymodloader", "loader") => {
                    add_loader(&mut out, LoaderKind::NeoForge, Some(version.into()))
                }
                ("net.minecraftforge", "forge") | ("net.minecraftforge", "fmlloader") => {
                    // forge versions look like "1.20.1-47.2.0"; keep the part after the first '-'
                    let v = version.split_once('-').map(|x| x.1).unwrap_or(version);
                    add_loader(&mut out, LoaderKind::Forge, Some(v.into()))
                }
                ("optifine", "OptiFine") => add_loader(&mut out, LoaderKind::OptiFine, Some(version.replace('_', " "))),
                ("com.mumfrey", "liteloader") => add_loader(&mut out, LoaderKind::LiteLoader, Some(version.into())),
                _ => {}
            }
        }
    }

    // Launch-argument hints (legacy and modern Forge).
    let mut arg_text = String::new();
    if let Some(s) = json.get("minecraftArguments").and_then(Value::as_str) {
        arg_text.push_str(s);
    }
    if let Some(args) = json.get("arguments").and_then(|a| a.get("game")).and_then(Value::as_array) {
        for a in args.iter().filter_map(Value::as_str) {
            arg_text.push(' ');
            arg_text.push_str(a);
        }
    }
    if arg_text.contains("--fml.forgeVersion") {
        let v = arg_text.split("--fml.forgeVersion").nth(1).and_then(|s| s.split_whitespace().next()).map(String::from);
        add_loader(&mut out, LoaderKind::Forge, v);
    }
    if arg_text.contains("--fml.neoForgeVersion") {
        let v = arg_text.split("--fml.neoForgeVersion").nth(1).and_then(|s| s.split_whitespace().next()).map(String::from);
        add_loader(&mut out, LoaderKind::NeoForge, v);
    }

    // Main-class fallbacks when libraries are stripped.
    let main = json.get("mainClass").and_then(Value::as_str).unwrap_or("");
    if main.contains("fabricmc") && !out.iter().any(|l| l.kind == LoaderKind::Fabric) {
        add_loader(&mut out, LoaderKind::Fabric, None);
    }
    if main.contains("quiltmc") && !out.iter().any(|l| l.kind == LoaderKind::Quilt) {
        add_loader(&mut out, LoaderKind::Quilt, None);
    }
    if (main.contains("cpw.mods.modlauncher") || main.contains("net.minecraftforge"))
        && !out.iter().any(|l| matches!(l.kind, LoaderKind::Forge | LoaderKind::NeoForge))
        && arg_text.contains("forge")
    {
        add_loader(&mut out, LoaderKind::Forge, None);
    }
    if main.contains("launchwrapper") && json.to_string().to_lowercase().contains("liteloader") {
        add_loader(&mut out, LoaderKind::LiteLoader, None);
    }
    out
}

/// Best effort Minecraft (vanilla) version of a merged version JSON.
pub fn detect_mc_version(json: &Value, dir_name: &str) -> Option<String> {
    static REL: OnceLock<Regex> = OnceLock::new();
    let rel = re(&REL, r"^1\.\d+(\.\d+)?$|^\d{2}\.\d+(\.\d+)?$");
    // 1. clientVersion (HMCL / PCL custom)
    if let Some(v) = json.get("clientVersion").and_then(Value::as_str) {
        return Some(v.into());
    }
    // 2. inheritsFrom already merged: parent's id is kept in `_parentId` by `scan::merge_json`
    if let Some(v) = json.get("_parentId").and_then(Value::as_str) {
        return Some(v.into());
    }
    // 3. libraries: forge "1.20.1-47.2.0", optifine "1.20.1_HD_U_I6"
    if let Some(libs) = json.get("libraries").and_then(Value::as_array) {
        for lib in libs {
            let Some(name) = lib.get("name").and_then(Value::as_str) else { continue };
            if let Some((g, a, v)) = coord(name) {
                if ((g == "net.minecraftforge" && (a == "forge" || a == "fmlloader")) || (g == "optifine" && a == "OptiFine"))
                    && let Some(mc) = v.split(['-', '_']).next()
                        && rel.is_match(mc) {
                            return Some(mc.into());
                        }
                if g == "net.minecraft" && a == "client" {
                    return Some(v.into());
                }
            }
        }
    }
    // 4. the JSON id, then the folder name
    for cand in [json.get("id").and_then(Value::as_str), Some(dir_name)].into_iter().flatten() {
        if rel.is_match(cand) || is_snapshot_name(cand) {
            return Some(cand.into());
        }
    }
    // 5. a name that starts with a release number, e.g. "1.20.1-Fabric"
    static PRE: OnceLock<Regex> = OnceLock::new();
    let pre = re(&PRE, r"(1\.\d+(?:\.\d+)?)");
    for cand in [json.get("id").and_then(Value::as_str), Some(dir_name)].into_iter().flatten() {
        if let Some(c) = pre.captures(cand) {
            return Some(c[1].into());
        }
    }
    json.get("assets").and_then(Value::as_str).filter(|a| rel.is_match(a)).map(String::from)
}

pub fn classify_kind(json: &Value, mc: Option<&str>, loaders: &[Loader], dir_name: &str) -> InstanceKind {
    let id = mc.unwrap_or(dir_name);
    let vtype = json.get("type").and_then(Value::as_str).unwrap_or("");
    // Loader precedence follows PCL: Fabric/Quilt > Forge > NeoForge, then LiteLoader / OptiFine.
    let has = |k: LoaderKind| loaders.iter().any(|l| l.kind == k);
    if has(LoaderKind::Fabric) {
        return InstanceKind::Fabric;
    }
    if has(LoaderKind::Quilt) {
        return InstanceKind::Quilt;
    }
    if has(LoaderKind::Forge) {
        return InstanceKind::Forge;
    }
    if has(LoaderKind::NeoForge) {
        return InstanceKind::NeoForge;
    }
    if has(LoaderKind::LiteLoader) {
        return InstanceKind::LiteLoader;
    }
    if has(LoaderKind::OptiFine) {
        return InstanceKind::OptiFine;
    }
    if fool_description(id).is_some() || fool_description(dir_name).is_some() {
        return InstanceKind::Fool;
    }
    if vtype == "old_alpha" || vtype == "old_beta" {
        return InstanceKind::Old;
    }
    if vtype == "snapshot" || is_snapshot_name(id) {
        return InstanceKind::Snapshot;
    }
    InstanceKind::Release
}

pub fn display_name(kind: InstanceKind, mc: Option<&str>, loaders: &[Loader]) -> String {
    let mut parts = String::new();
    for l in loaders {
        let label = match l.kind {
            LoaderKind::Forge => "Forge",
            LoaderKind::NeoForge => "NeoForge",
            LoaderKind::Fabric => "Fabric",
            LoaderKind::Quilt => "Quilt",
            LoaderKind::OptiFine => "OptiFine",
            LoaderKind::LiteLoader => "LiteLoader",
        };
        parts.push_str(", ");
        parts.push_str(label);
        if let Some(v) = &l.version {
            parts.push(' ');
            parts.push_str(v);
        }
    }
    let ver = mc.unwrap_or("");
    let lower = ver.to_lowercase();
    let base = match kind {
        InstanceKind::Old => "远古版本".to_string(),
        InstanceKind::Fool => format!("愚人节版本 {ver}"),
        InstanceKind::Error => String::new(),
        _ if lower.contains("pre") => format!("预发布版 {ver}"),
        _ if lower.contains("rc") => format!("发布候选 {ver}"),
        _ if lower.contains("experimental") => format!("实验性快照 {ver}"),
        _ if is_snapshot_name(ver) => format!("快照版 {ver}"),
        _ if ver.is_empty() => "正式版".to_string(),
        _ => format!("正式版 {ver}"),
    };
    format!("{base}{parts}").replace('_', "-")
}

pub fn icon_for(kind: InstanceKind) -> &'static str {
    match kind {
        InstanceKind::Release => "grass",
        InstanceKind::Snapshot => "command_block",
        InstanceKind::Old => "cobblestone",
        InstanceKind::Forge => "anvil",
        InstanceKind::NeoForge => "neoforge",
        InstanceKind::Fabric => "fabric",
        InstanceKind::Quilt => "fabric",
        InstanceKind::OptiFine => "grass_path",
        InstanceKind::LiteLoader => "egg",
        InstanceKind::Fool => "gold_block",
        InstanceKind::Error => "redstone_block",
    }
}

pub fn category_for(kind: InstanceKind, modable: bool, starred: bool, display_type: i32) -> Category {
    if starred {
        return Category::Star;
    }
    match display_type {
        1 => return Category::Hidden,
        2 => return Category::Modded,
        3 => return Category::Regular,
        4 => return Category::Rubbish,
        5 => return Category::Fool,
        6 => return Category::Error,
        _ => {}
    }
    match kind {
        InstanceKind::Error => Category::Error,
        InstanceKind::Fool => Category::Fool,
        _ if modable => Category::Modded,
        InstanceKind::Snapshot | InstanceKind::Old => Category::Rubbish,
        _ => Category::Regular,
    }
}

/// Folder/file name rules for instance names. Rejects path separators and dot names.
pub fn validate_name(name: &str) -> Result<(), String> {
    let t = name.trim();
    if t.is_empty() {
        return Err("名称不能为空".into());
    }
    if t != name {
        return Err("名称首尾不能包含空格".into());
    }
    if name == "." || name == ".." || name.ends_with('.') {
        return Err("名称不合法".into());
    }
    if name.chars().any(|c| matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0') || c.is_control()) {
        return Err("名称不能包含 / \\ : * ? \" < > | 等特殊字符".into());
    }
    if name.len() > 128 {
        return Err("名称过长".into());
    }
    Ok(())
}

pub fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}
