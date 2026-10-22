// UI-facing types. The real backend shapes live in ./raw.ts and are mapped by ./adapters.ts.

export type LoaderKind = "vanilla" | "fabric" | "forge" | "neoforge" | "quilt";
export type ResourceKind = "mod" | "modpack" | "resourcepack" | "shader";

// ---- accounts ----
export type AccountKind = "offline" | "microsoft" | "authlib";
export interface Account {
  id: string;
  kind: AccountKind;
  name: string;
  uuid: string;
  /** authlib-injector server url, when kind === "authlib" */
  server?: string;
  serverName?: string;
}
export interface DeviceCode {
  userCode: string;
  verificationUri: string;
  /** seconds until the code expires */
  expiresIn: number;
  interval: number;
  message?: string;
}
export interface YggdrasilServerInfo {
  url: string;
  name: string;
}

// ---- launch / instances ----
export type InstanceLoaderKind = "forge" | "neoforge" | "fabric" | "quilt" | "optifine" | "liteloader";
export interface InstanceLoader {
  kind: InstanceLoaderKind;
  version?: string;
}
export interface Instance {
  /** absolute path of `<root>/versions/<name>`; this is the `instanceId` of every app_* command */
  id: string;
  name: string;
  root: string;
  path: string;
  mcVersion: string;
  /** primary mod loader used for resource filtering */
  loader: LoaderKind;
  loaderVersion?: string;
  loaders: InstanceLoader[];
  /** human readable one-liner from the backend, e.g. "正式版 1.20.1, Fabric 0.15.0" */
  description: string;
  kind: string;
  starred: boolean;
  hidden: boolean;
  modable: boolean;
  isolated: boolean;
  gameDir: string;
  /** set when the version json is broken; such instances cannot be launched */
  error?: string;
}
export type LaunchStage = "check" | "java" | "download" | "natives" | "start" | "running";
export interface LaunchProgress {
  launchId: string;
  stage: LaunchStage;
  /** 0..1 overall */
  progress: number;
  message: string;
}
export type LogLevel = "info" | "warn" | "error" | "debug";
export type LogStream = "stdout" | "stderr" | "launcher";
export interface LaunchLog {
  launchId: string;
  level: LogLevel;
  stream: LogStream;
  line: string;
  ts: number;
}
export interface CrashAction {
  id: string;
  label: string;
}
export interface CrashReason {
  id: string;
  message: string;
  details: string[];
  actions: CrashAction[];
}
export interface CrashAnalysis {
  reasons: CrashReason[];
  summary: string;
  filesUsed: string[];
  primaryFile: string | null;
}
export interface LaunchExit {
  launchId: string;
  /** null when the process was killed by a signal */
  code: number | null;
  crashed: boolean;
  crash: CrashAnalysis | null;
  durationMs?: number;
}
export interface ModEntry {
  /** absolute path (changes when a mod is disabled: `.jar` <-> `.jar.disabled`) */
  path: string;
  fileName: string;
  name: string;
  version?: string;
  enabled: boolean;
  size: number;
  description?: string;
  authors: string[];
  loaders: string[];
}
export interface SaveEntry {
  folder: string;
  name: string;
  path: string;
  size: number;
  lastPlayed?: number;
  /** png data url (<= 256 KiB) */
  iconUrl?: string;
}
export interface McFolder {
  name: string;
  path: string;
}
export type OpenTarget = "instance" | "game" | "mods" | "saves" | "logs";

// ---- download ----
export type GameVersionType = "release" | "snapshot" | "old_beta" | "old_alpha";
export interface GameVersion {
  id: string;
  type: GameVersionType;
  releaseTime: string;
}
export interface LoaderVersion {
  version: string;
  stable?: boolean;
}
export type InstallLoaderKind = Exclude<LoaderKind, "vanilla">;
export interface InstallGameRequest {
  mcVersion: string;
  loader?: { kind: InstallLoaderKind; version: string };
  instanceName?: string;
}
export type TaskStatus = "running" | "paused" | "completed" | "failed" | "cancelled";
export interface DownloadProgress {
  taskId: string;
  stage: string;
  /** 0..1 for the whole task */
  fraction: number;
  doneBytes: number;
  totalBytes: number;
  doneFiles: number;
  totalFiles: number;
  speedBps: number;
  state: "running" | "paused" | "cancelled";
}
export interface DownloadFinished {
  taskId: string;
  kind: string;
  label: string;
  status: TaskStatus;
  error?: string;
}
export interface DownloadTask {
  taskId: string;
  kind: string;
  label: string;
  status: TaskStatus;
  stage: string;
  fraction: number;
  speedBps: number;
  doneBytes: number;
  totalBytes: number;
  doneFiles: number;
  totalFiles: number;
  error?: string;
}
export interface MissingFile {
  path: string;
  projectId: number | string;
  fileId: number | string;
  pageUrl: string;
}
export interface ModpackReport {
  taskId: string;
  instanceId: string;
  missingFiles: MissingFile[];
}

// ---- resources ----
export type ResourceSource = "modrinth" | "curseforge";
export type ResourceSourceFilter = ResourceSource | "all";
export interface ResourceQuery {
  kind: ResourceKind;
  query: string;
  source: ResourceSourceFilter;
  mcVersion?: string;
  loader?: LoaderKind;
  offset: number;
  limit: number;
}
export interface ResourceItem {
  id: string;
  slug: string;
  kind: ResourceKind;
  source: ResourceSource;
  title: string;
  /** untrusted plain text from the platform; render as text only */
  summary: string;
  author?: string;
  downloads: number;
  follows: number;
  iconUrl?: string;
  tags: string[];
  pageUrl?: string;
  updated?: string;
}
export interface ResourcePage {
  total: number;
  items: ResourceItem[];
  /** per-platform failures (e.g. CurseForge key missing) that did not stop the search */
  warnings: string[];
}
export type ModpackSource =
  | { type: "file"; path: string }
  | { type: "project"; platform: ResourceSource; projectId: string; versionId?: string };

// ---- settings (backend schema v1, snake_case as stored) ----
export type DownloadSource = "official" | "mirror_first" | "official_first";
export type LauncherVisibility = "close_immediately" | "hide_then_close" | "hide" | "minimize" | "keep";
export type IsolationMode = "off" | "moddable_only" | "non_release_only" | "moddable_and_non_release" | "all";
export type GameWindowMode = "fullscreen" | "default" | "custom" | "same_as_launcher";
export interface GlobalSettings {
  schema_version: number;
  game: {
    folders: McFolder[];
    selected_folder: string | null;
    selected_instance: string | null;
    isolation: IsolationMode;
    window_mode: GameWindowMode;
    window_width: number;
    window_height: number;
    jvm_args: string;
    game_args: string;
    pre_launch_command: string;
    wait_for_pre_launch: boolean;
    launcher_visibility: LauncherVisibility;
    process_priority: number;
    custom_title: string;
    custom_info: string;
  };
  memory: { mode: "auto" | "custom"; custom_mb: number; optimize_before_launch: boolean };
  java: { selected: string | null; custom_paths: string[]; prefer_64bit: boolean };
  download: {
    source: DownloadSource;
    version_source: DownloadSource;
    mod_source: DownloadSource;
    thread_count: number;
    speed_limit_bps: number;
    validate_ssl: boolean;
    translate_mod_names: boolean;
  };
  ui: Record<string, unknown>;
  window: Record<string, unknown>;
  launcher: { debug_mode: boolean; check_update_release: boolean; check_update_snapshot: boolean; telemetry: boolean };
}
export type DeepPartial<T> = { [K in keyof T]?: T[K] extends (infer U)[] ? U[] : T[K] extends object ? DeepPartial<T[K]> : T[K] };
export interface InstanceSettings {
  schema_version: number;
  isolated: boolean | null;
  memory_mb: number | null;
  java_path: string | null;
  jvm_args: string;
  game_args: string;
  pre_launch_command: string;
  wait_for_pre_launch: boolean;
  server_address: string;
  custom_title: string;
  custom_info: string;
  starred: boolean;
  display_type: number;
  logo: string;
  logo_custom: boolean;
  disable_mod_update_check: boolean;
}
export interface JavaInfo {
  path: string;
  home: string;
  version: string;
  major: number;
  arch: string;
  vendor: string;
  isJdk: boolean;
}
