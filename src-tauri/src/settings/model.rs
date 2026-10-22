use serde::{Deserialize, Serialize};

/// Bump when the on-disk layout changes and add a step in `store::migrate`.
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct McFolderEntry {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MemoryMode {
    /// Pick automatically based on physical memory / mod count.
    #[default]
    Auto,
    /// Use `custom_mb`.
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DownloadSource {
    Official,
    /// Prefer mirrors, fall back to official (PCL default).
    #[default]
    MirrorFirst,
    /// Official first, fall back to mirrors.
    OfficialFirst,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    Light,
    Dark,
    #[default]
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WindowButtonStyle {
    #[default]
    Pcl,
    MacOs,
}

/// What the launcher does once the game window shows (PCL `LaunchArgumentVisible`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LauncherVisibility {
    CloseImmediately,
    HideThenClose,
    Hide,
    Minimize,
    #[default]
    Keep,
}

/// Game window size mode (PCL `LaunchArgumentWindowType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GameWindowMode {
    Fullscreen,
    #[default]
    Default,
    Custom,
    SameAsLauncher,
}

/// Which instances get a version-isolated game dir (PCL `LaunchArgumentIndieV2`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum IsolationMode {
    Off,
    ModdableOnly,
    NonReleaseOnly,
    ModdableAndNonRelease,
    #[default]
    All,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GameSettings {
    /// Known `.minecraft` folders. The first run seeds the macOS default.
    pub folders: Vec<McFolderEntry>,
    /// Path of the currently selected folder (must be in `folders`).
    pub selected_folder: Option<String>,
    /// Name of the currently selected instance inside the selected folder.
    pub selected_instance: Option<String>,
    pub isolation: IsolationMode,
    pub window_mode: GameWindowMode,
    pub window_width: u32,
    pub window_height: u32,
    pub jvm_args: String,
    pub game_args: String,
    pub pre_launch_command: String,
    pub wait_for_pre_launch: bool,
    pub launcher_visibility: LauncherVisibility,
    /// Process priority: 0 high, 1 normal, 2 low.
    pub process_priority: u8,
    /// Title / info template shown in the game window (PCL `LaunchArgumentTitle`).
    pub custom_title: String,
    pub custom_info: String,
}

pub const DEFAULT_JVM_ARGS: &str =
    "-XX:-OmitStackTraceInFastThrow -Dfml.ignoreInvalidMinecraftCertificates=True -Dfml.ignorePatchDiscrepancies=True";

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            folders: Vec::new(),
            selected_folder: None,
            selected_instance: None,
            isolation: IsolationMode::default(),
            window_mode: GameWindowMode::default(),
            window_width: 854,
            window_height: 480,
            jvm_args: DEFAULT_JVM_ARGS.to_string(),
            game_args: String::new(),
            pre_launch_command: String::new(),
            wait_for_pre_launch: true,
            launcher_visibility: LauncherVisibility::default(),
            process_priority: 1,
            custom_title: String::new(),
            custom_info: "PCL".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MemorySettings {
    pub mode: MemoryMode,
    /// Used when `mode == Custom`.
    pub custom_mb: u32,
    /// Run a free-memory pass before launch (PCL `VersionRamOptimize`).
    pub optimize_before_launch: bool,
}

impl Default for MemorySettings {
    fn default() -> Self {
        Self { mode: MemoryMode::Auto, custom_mb: 4096, optimize_before_launch: false }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct JavaSettings {
    /// Absolute path to a `java` executable. `None` = auto select.
    pub selected: Option<String>,
    /// Java executables the user added by hand.
    pub custom_paths: Vec<String>,
    /// Prefer 64-bit Java when auto selecting.
    pub prefer_64bit: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DownloadSettings {
    pub source: DownloadSource,
    pub version_source: DownloadSource,
    pub mod_source: DownloadSource,
    pub thread_count: u32,
    /// Bytes/sec cap, 0 = unlimited.
    pub speed_limit_bps: u64,
    pub validate_ssl: bool,
    pub translate_mod_names: bool,
}

impl Default for DownloadSettings {
    fn default() -> Self {
        Self {
            source: DownloadSource::MirrorFirst,
            version_source: DownloadSource::MirrorFirst,
            mod_source: DownloadSource::OfficialFirst,
            thread_count: 64,
            speed_limit_bps: 0,
            validate_ssl: true,
            translate_mod_names: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiSettings {
    /// BCP-47 tag. Only `zh-CN` ships for now.
    pub language: String,
    pub theme_mode: ThemeMode,
    pub theme_id: String,
    pub hue: u16,
    pub saturation: u8,
    pub lightness: u8,
    /// 0..=1000 (PCL `UiLauncherTransparent`).
    pub opacity: u16,
    pub background_blur: u8,
    pub show_logo: bool,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            language: "zh-CN".into(),
            theme_mode: ThemeMode::System,
            theme_id: "pcl".into(),
            hue: 180,
            saturation: 80,
            lightness: 20,
            opacity: 1000,
            background_blur: 0,
            show_logo: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowSettings {
    pub width: u32,
    pub height: u32,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub maximized: bool,
    pub remember_geometry: bool,
    pub button_style: WindowButtonStyle,
}

impl Default for WindowSettings {
    fn default() -> Self {
        Self { width: 1000, height: 618, x: None, y: None, maximized: false, remember_geometry: true, button_style: WindowButtonStyle::Pcl }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct LauncherSettings {
    pub debug_mode: bool,
    pub check_update_release: bool,
    pub check_update_snapshot: bool,
    /// Telemetry is opt-in on this port.
    pub telemetry: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GlobalSettings {
    pub schema_version: u32,
    pub game: GameSettings,
    pub memory: MemorySettings,
    pub java: JavaSettings,
    pub download: DownloadSettings,
    pub ui: UiSettings,
    pub window: WindowSettings,
    pub launcher: LauncherSettings,
}

impl Default for GlobalSettings {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            game: GameSettings::default(),
            memory: MemorySettings::default(),
            java: JavaSettings { prefer_64bit: true, ..Default::default() },
            download: DownloadSettings::default(),
            ui: UiSettings::default(),
            window: WindowSettings::default(),
            launcher: LauncherSettings::default(),
        }
    }
}

impl GlobalSettings {
    /// Clamp values into ranges the rest of the app can rely on and make sure
    /// the folder list / selection are consistent.
    pub fn sanitize(&mut self) {
        self.memory.custom_mb = self.memory.custom_mb.clamp(512, 1 << 20);
        self.download.thread_count = self.download.thread_count.clamp(1, 256);
        self.ui.opacity = self.ui.opacity.min(1000);
        self.ui.hue = self.ui.hue.min(360);
        self.ui.saturation = self.ui.saturation.min(100);
        self.ui.lightness = self.ui.lightness.min(100);
        self.ui.background_blur = self.ui.background_blur.min(100);
        self.game.window_width = self.game.window_width.clamp(100, 16384);
        self.game.window_height = self.game.window_height.clamp(100, 16384);
        self.game.process_priority = self.game.process_priority.min(2);
        self.window.width = self.window.width.clamp(400, 16384);
        self.window.height = self.window.height.clamp(300, 16384);
        // Drop empty and duplicate folders, keep order.
        let mut seen = std::collections::HashSet::new();
        self.game.folders.retain(|f| !f.path.is_empty() && seen.insert(f.path.clone()));
        // Seed default folder on first run.
        if self.game.folders.is_empty() {
            self.game.folders.push(McFolderEntry {
                name: "默认文件夹".into(),
                path: crate::settings::store::default_minecraft_dir().to_string_lossy().into_owned(),
            });
        }
        let valid = self.game.selected_folder.as_ref().map(|s| self.game.folders.iter().any(|f| &f.path == s)).unwrap_or(false);
        if !valid {
            self.game.selected_folder = self.game.folders.first().map(|f| f.path.clone());
        }
        self.java.custom_paths.dedup();
        self.schema_version = CURRENT_SCHEMA_VERSION;
    }
}
