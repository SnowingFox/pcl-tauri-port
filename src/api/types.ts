// Frontend-assumed contract types. Reconcile with the real Rust commands
// (see docs/api/frontend-contract.md).

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
}
export interface DeviceCode {
  userCode: string;
  verificationUri: string;
  expiresIn: number;
  interval: number;
}
export type MsLoginEvent =
  | { state: "pending" }
  | { state: "success"; account: Account }
  | { state: "error"; message: string };

// ---- launch / instances ----
export interface Instance {
  id: string;
  name: string;
  mcVersion: string;
  loader: LoaderKind;
  loaderVersion?: string;
  lastPlayed?: number;
  favorite?: boolean;
}
export type LaunchStage =
  | "preparing"
  | "checking"
  | "downloading"
  | "authenticating"
  | "starting"
  | "running";
export interface LaunchProgress {
  launchId: string;
  stage: LaunchStage;
  /** 0..1 */
  progress: number;
  message: string;
}
export type LogLevel = "info" | "warn" | "error" | "debug";
export interface LaunchLog {
  launchId: string;
  level: LogLevel;
  line: string;
  ts: number;
}
export interface LaunchExit {
  launchId: string;
  code: number;
  crashed: boolean;
  /** short analysis hint produced by the backend, if any */
  reason?: string;
  reportPath?: string;
}
export interface ModEntry {
  fileName: string;
  name: string;
  version?: string;
  enabled: boolean;
  size: number;
}
export interface SaveEntry {
  folder: string;
  name: string;
  lastPlayed: number;
  size: number;
}

// ---- download ----
export type GameVersionType = "release" | "snapshot" | "old_beta" | "old_alpha";
export interface GameVersion {
  id: string;
  type: GameVersionType;
  releaseTime: string;
}
export interface InstallRequest {
  name: string;
  mcVersion: string;
  loader: LoaderKind;
  loaderVersion?: string;
}
export interface DownloadProgress {
  taskId: string;
  progress: number;
  message: string;
  done: boolean;
  error?: string;
}

// ---- resources ----
export type ResourceSource = "modrinth" | "curseforge";
export interface ResourceQuery {
  kind: ResourceKind;
  query: string;
  source: ResourceSource;
  mcVersion?: string;
  loader?: LoaderKind;
  offset: number;
  limit: number;
}
export interface ResourceItem {
  id: string;
  kind: ResourceKind;
  source: ResourceSource;
  title: string;
  summary: string;
  author: string;
  downloads: number;
  iconUrl?: string;
  tags: string[];
}
export interface ResourcePage {
  total: number;
  items: ResourceItem[];
}

// ---- settings ----
export interface JavaInstall {
  path: string;
  version: string;
  vendor: string;
  is64: boolean;
}
export type WindowCloseAction = "keep" | "hide" | "exit";
export interface BackendSettings {
  java: { autoSelect: boolean; path: string; autoMemory: boolean; memoryMb: number; jvmArgs: string };
  launch: {
    gameArgs: string;
    width: number;
    height: number;
    fullscreen: boolean;
    onLaunch: WindowCloseAction;
  };
  download: { source: "official" | "bmclapi"; threads: number; verifyFiles: boolean };
}
