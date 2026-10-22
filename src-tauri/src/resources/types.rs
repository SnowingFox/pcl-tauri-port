use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Modrinth,
    CurseForge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResourceKind {
    Mod,
    Modpack,
    ResourcePack,
    Shader,
    DataPack,
    World,
}

impl ResourceKind {
    pub fn modrinth_type(self) -> &'static str {
        match self {
            Self::Mod => "mod",
            Self::Modpack => "modpack",
            Self::ResourcePack => "resourcepack",
            Self::Shader => "shader",
            Self::DataPack => "mod", // datapacks are filtered by category on Modrinth
            Self::World => "mod",
        }
    }
    /// CurseForge `classId` for Minecraft (gameId 432).
    pub fn curseforge_class(self) -> u32 {
        match self {
            Self::Mod => 6,
            Self::Modpack => 4471,
            Self::ResourcePack => 12,
            Self::Shader => 6552,
            Self::DataPack => 6945,
            Self::World => 17,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModLoader {
    Forge,
    NeoForge,
    Fabric,
    Quilt,
}

impl ModLoader {
    pub fn modrinth_name(self) -> &'static str {
        match self {
            Self::Forge => "forge",
            Self::NeoForge => "neoforge",
            Self::Fabric => "fabric",
            Self::Quilt => "quilt",
        }
    }
    pub fn from_name(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "forge" => Some(Self::Forge),
            "neoforge" | "neo-forge" => Some(Self::NeoForge),
            "fabric" => Some(Self::Fabric),
            "quilt" => Some(Self::Quilt),
            _ => None,
        }
    }
    pub fn curseforge_id(self) -> u32 {
        match self {
            Self::Forge => 1,
            Self::Fabric => 4,
            Self::Quilt => 5,
            Self::NeoForge => 6,
        }
    }
    pub fn from_curseforge_id(id: u64) -> Option<Self> {
        match id {
            1 => Some(Self::Forge),
            4 => Some(Self::Fabric),
            5 => Some(Self::Quilt),
            6 => Some(Self::NeoForge),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchQuery {
    pub query: String,
    pub mc_version: Option<String>,
    pub loader: Option<ModLoader>,
    pub categories: Vec<String>,
    pub page: u32,
    pub page_size: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectSummary {
    pub platform: Platform,
    pub id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub icon_url: Option<String>,
    pub downloads: u64,
    pub follows: u64,
    pub categories: Vec<String>,
    pub author: Option<String>,
    pub updated: Option<String>,
    pub page_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchResult {
    pub hits: Vec<ProjectSummary>,
    pub total: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DependencyKind {
    Required,
    Optional,
    Embedded,
    Incompatible,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Dependency {
    pub project_id: Option<String>,
    pub file_id: Option<String>,
    pub kind: DependencyKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceFile {
    pub platform: Platform,
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub file_name: String,
    pub version_number: Option<String>,
    /// release | beta | alpha
    pub release_type: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<ModLoader>,
    pub sha1: Option<String>,
    pub size: u64,
    /// May be empty on CurseForge when the author disabled third-party distribution.
    pub download_urls: Vec<String>,
    pub date_published: String,
    pub dependencies: Vec<Dependency>,
    pub fingerprint: Option<u32>,
}
