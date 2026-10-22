// Shapes exactly as the Rust backend serializes them (see docs/api/*.md and the serde derives).
// Only adapters.ts consumes these.

export interface RawAccountInfo {
  id: string;
  kind: string; // offline | microsoft | yggdrasil
  name: string;
  uuid: string;
  serverUrl?: string | null;
  serverName?: string | null;
  identifier?: string | null;
}
export interface RawDeviceCode {
  deviceCode?: string;
  userCode: string;
  verificationUri: string;
  expiresIn: number;
  interval?: number;
  message?: string | null;
}
export interface RawLoader {
  kind: string; // forge | neo_forge | fabric | quilt | opti_fine | lite_loader (docs say neoforge/optifine)
  version?: string | null;
}
export interface RawInstanceInfo {
  name: string;
  path: string;
  root: string;
  kind: string;
  mc_version?: string | null;
  loaders?: RawLoader[];
  release_time?: string | null;
  version_type?: string | null;
  description?: string;
  category?: string;
  icon?: string;
  custom_logo_path?: string | null;
  starred?: boolean;
  hidden?: boolean;
  modable?: boolean;
  game_dir?: string;
  isolated?: boolean;
  inherits_from?: string | null;
  error?: string | null;
}
export interface RawVersionManifest {
  latest?: { release: string; snapshot: string };
  versions: { id: string; type: string; url?: string; sha1?: string | null; releaseTime?: string | null }[];
}
export interface RawProgressEvent {
  task_id: string;
  stage: string;
  done_bytes: number;
  total_bytes: number;
  done_files: number;
  total_files: number;
  speed_bps: number;
  fraction: number;
  state: string;
}
export interface RawTaskInfo {
  task_id: string;
  kind: string;
  label: string;
  status: string;
  error?: string | null;
  last_progress?: RawProgressEvent | null;
}
export interface RawProject {
  platform: string;
  id: string;
  slug: string;
  title: string;
  description: string;
  icon_url?: string | null;
  downloads: number;
  follows: number;
  categories: string[];
  author?: string | null;
  updated?: string | null;
  page_url?: string | null;
}
export interface RawSearchResult {
  hits: RawProject[];
  total: number;
}
export interface RawLocalMod {
  path: string;
  file_name: string;
  enabled: boolean;
  size: number;
  sha1?: string;
  curseforge_fingerprint?: number;
  meta?: {
    mod_id?: string | null;
    name?: string | null;
    version?: string | null;
    description?: string | null;
    authors?: string[];
    loaders?: string[];
  };
}
export interface RawSave {
  folder: string;
  name: string;
  sizeBytes?: number;
  lastPlayedMs?: number | null;
  iconDataUrl?: string | null;
  path: string;
}
export interface RawJavaInfo {
  path: string;
  home: string;
  version: string;
  major: number;
  arch: string;
  vendor: string;
  is_jdk: boolean;
}
export interface RawCrashAnalysis {
  reasons?: { id: string; details?: string[]; message: string; actions?: { id: string; label: string }[] }[];
  summary?: string;
  // crash module serializes snake_case; app layer docs say camelCase. Accept both.
  files_used?: string[];
  filesUsed?: string[];
  primary_file?: string | null;
  primaryFile?: string | null;
}
export interface RawExitEvent {
  id: string;
  code: number | null;
  crashed?: boolean;
  crash?: RawCrashAnalysis | null;
  durationMs?: number;
}
export interface RawLogEvent {
  id: string;
  stream: string;
  line: string;
}
export interface RawLaunchProgress {
  launchId: string;
  stage: string;
  progress: number;
  message: string;
}
export interface RawModpackReport {
  taskId: string;
  instanceId: string;
  missingFiles?: { path: string; projectId: number | string; fileId: number | string; pageUrl: string }[];
}
