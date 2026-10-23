// Pure mapping functions: Rust payload -> UI types. No I/O, fully unit-tested.
import type {
  Account, AccountKind, CrashAnalysis, DeviceCode, DownloadFinished, DownloadProgress, DownloadTask,
  GameVersion, GameVersionType, Instance, InstanceLoader, InstanceLoaderKind, JavaInfo, LaunchExit,
  LaunchLog, LaunchProgress, LaunchStage, LoaderKind, LogLevel, LogStream, MissingFile, ModEntry,
  ModpackReport, ResourceItem, ResourceKind, ResourcePage, ResourceSource, SaveEntry, TaskStatus,
} from "./types";
import type {
  RawAccountInfo, RawCrashAnalysis, RawDeviceCode, RawExitEvent, RawInstanceInfo, RawJavaInfo,
  RawLaunchProgress, RawLocalMod, RawLogEvent, RawModpackReport, RawProgressEvent, RawProject, RawSave,
  RawSearchResult, RawTaskInfo, RawVersionManifest,
} from "./raw";
import { safeImageUrl } from "./util";

const NAME_ALIASES: Record<string, string> = {
  neo_forge: "neoforge", "neo-forge": "neoforge", opti_fine: "optifine", lite_loader: "liteloader",
};
const normName = (s: string) => NAME_ALIASES[s.toLowerCase()] ?? s.toLowerCase();

// ---- accounts ----
export function toAccount(r: RawAccountInfo): Account {
  const kind: AccountKind = r.kind === "yggdrasil" || r.kind === "authlib" ? "authlib" : r.kind === "microsoft" ? "microsoft" : "offline";
  return {
    id: r.id, kind, name: r.name, uuid: r.uuid,
    server: r.serverUrl ?? undefined, serverName: r.serverName ?? undefined,
  };
}
export function toDeviceCode(r: RawDeviceCode): DeviceCode {
  return {
    userCode: r.userCode, verificationUri: r.verificationUri, expiresIn: r.expiresIn,
    interval: r.interval ?? 5, message: r.message ?? undefined,
  };
}

// ---- instances ----
const INSTANCE_LOADERS: InstanceLoaderKind[] = ["forge", "neoforge", "fabric", "quilt", "optifine", "liteloader"];
const PRIMARY_ORDER: LoaderKind[] = ["neoforge", "forge", "fabric", "quilt"];

export function toInstance(r: RawInstanceInfo): Instance {
  const loaders: InstanceLoader[] = (r.loaders ?? [])
    .map((l) => ({ kind: normName(l.kind) as InstanceLoaderKind, version: l.version ?? undefined }))
    .filter((l) => INSTANCE_LOADERS.includes(l.kind));
  const primary = PRIMARY_ORDER.map((k) => loaders.find((l) => l.kind === k)).find(Boolean);
  return {
    id: r.path, name: r.name, root: r.root, path: r.path,
    mcVersion: r.mc_version ?? "",
    loader: (primary?.kind as LoaderKind | undefined) ?? "vanilla",
    loaderVersion: primary?.version,
    loaders,
    description: r.description ?? "",
    kind: r.kind,
    starred: !!r.starred, hidden: !!r.hidden, modable: !!r.modable, isolated: !!r.isolated,
    gameDir: r.game_dir ?? r.path,
    error: r.error ?? (r.kind === "error" ? "version json is broken" : undefined),
  };
}

// ---- launch ----
const STAGES: LaunchStage[] = ["check", "java", "download", "natives", "start", "running"];
export function toLaunchProgress(r: RawLaunchProgress): LaunchProgress {
  const stage = (STAGES as string[]).includes(r.stage) ? (r.stage as LaunchStage) : "check";
  return { launchId: r.launchId, stage, progress: clamp01(r.progress), message: r.message ?? "" };
}

export function classifyLogLevel(stream: LogStream | string, line: string): LogLevel {
  if (/\b(FATAL|ERROR)\b|Exception|\bat [\w$.]+\(/.test(line)) return "error";
  if (/\bWARN\b/.test(line)) return "warn";
  if (/\bDEBUG\b|\bTRACE\b/.test(line)) return "debug";
  if (stream === "stderr" && !/\bINFO\b/.test(line)) return "warn";
  return "info";
}
export function toLaunchLog(r: RawLogEvent, now: () => number = Date.now): LaunchLog {
  const stream = (["stdout", "stderr", "launcher"].includes(r.stream) ? r.stream : "stdout") as LogStream;
  return { launchId: r.id, stream, level: stream === "launcher" ? "debug" : classifyLogLevel(stream, r.line), line: r.line, ts: now() };
}

export function toCrashAnalysis(r: RawCrashAnalysis | null | undefined): CrashAnalysis | null {
  if (!r) return null;
  return {
    reasons: (r.reasons ?? []).map((x) => ({
      id: x.id, message: x.message, details: x.details ?? [], actions: x.actions ?? [],
    })),
    summary: r.summary ?? "",
    filesUsed: r.filesUsed ?? r.files_used ?? [],
    primaryFile: r.primaryFile ?? r.primary_file ?? null,
  };
}
export function toLaunchExit(r: RawExitEvent): LaunchExit {
  const crash = toCrashAnalysis(r.crash);
  // Older backends (launch-core only) send {id, code}: infer a crash from a non-zero exit code.
  const crashed = r.crashed ?? (r.code !== null && r.code !== 0);
  return { launchId: r.id, code: r.code, crashed, crash, durationMs: r.durationMs };
}

export function toModEntry(r: RawLocalMod): ModEntry {
  return {
    path: r.path, fileName: r.file_name,
    name: r.meta?.name || r.file_name.replace(/\.jar(\.disabled)?$/i, ""),
    version: r.meta?.version ?? undefined, enabled: r.enabled, size: r.size,
    description: r.meta?.description ?? undefined,
    authors: r.meta?.authors ?? [], loaders: r.meta?.loaders ?? [],
  };
}
export function toSave(r: RawSave): SaveEntry {
  return {
    folder: r.folder, name: r.name || r.folder, path: r.path, size: r.sizeBytes ?? 0,
    lastPlayed: r.lastPlayedMs ?? undefined, iconUrl: safeImageUrl(r.iconDataUrl),
  };
}

// ---- download ----
export function toGameVersions(m: RawVersionManifest): GameVersion[] {
  return m.versions.map((v) => ({
    id: v.id,
    type: (["release", "snapshot", "old_beta", "old_alpha"].includes(v.type) ? v.type : "snapshot") as GameVersionType,
    releaseTime: (v.releaseTime ?? "").slice(0, 10),
  }));
}
export function toDownloadProgress(r: RawProgressEvent): DownloadProgress {
  return {
    taskId: r.task_id, stage: r.stage, fraction: clamp01(r.fraction), doneBytes: r.done_bytes, totalBytes: r.total_bytes,
    doneFiles: r.done_files, totalFiles: r.total_files, speedBps: r.speed_bps,
    state: r.state === "paused" || r.state === "cancelled" ? r.state : "running",
    load: {
      activeFiles: r.active_files ?? 0, activeConnections: r.active_connections ?? 0,
      concurrency: r.concurrency ?? 0, segments: r.segments ?? 0,
      workerThreads: r.worker_threads ?? 0, cores: r.cores ?? 0, cpuCores: r.cpu_cores ?? 0,
    },
  };
}
export function toDownloadFinished(r: RawTaskInfo): DownloadFinished {
  return { taskId: r.task_id, kind: r.kind, label: r.label, status: r.status as TaskStatus, error: r.error ?? undefined };
}
export function toDownloadTask(r: RawTaskInfo): DownloadTask {
  const p = r.last_progress;
  return {
    taskId: r.task_id, kind: r.kind, label: r.label, status: r.status as TaskStatus, error: r.error ?? undefined,
    stage: p?.stage ?? "", fraction: clamp01(p?.fraction ?? (r.status === "completed" ? 1 : 0)),
    speedBps: p?.speed_bps ?? 0, doneBytes: p?.done_bytes ?? 0, totalBytes: p?.total_bytes ?? 0,
    doneFiles: p?.done_files ?? 0, totalFiles: p?.total_files ?? 0,
  };
}
export function toModpackReport(r: RawModpackReport): ModpackReport {
  const missingFiles: MissingFile[] = (r.missingFiles ?? []).map((f) => ({
    path: f.path, projectId: f.projectId, fileId: f.fileId, pageUrl: f.pageUrl,
  }));
  return { taskId: r.taskId, instanceId: r.instanceId, missingFiles };
}

// ---- resources ----
export function toResourceItem(r: RawProject, kind: ResourceKind): ResourceItem {
  return {
    id: r.id, slug: r.slug, kind,
    source: (r.platform === "curseforge" ? "curseforge" : "modrinth") as ResourceSource,
    title: r.title, summary: r.description ?? "",
    author: r.author ?? undefined, downloads: r.downloads ?? 0, follows: r.follows ?? 0,
    iconUrl: safeImageUrl(r.icon_url), tags: r.categories ?? [],
    pageUrl: r.page_url ?? undefined, updated: r.updated ?? undefined,
  };
}
export function toResourcePage(parts: { kind: ResourceKind; result?: RawSearchResult; error?: string }[]): ResourcePage {
  const items: ResourceItem[] = [];
  const warnings: string[] = [];
  let total = 0;
  for (const p of parts) {
    if (p.error) warnings.push(p.error);
    if (!p.result) continue;
    total += p.result.total;
    items.push(...p.result.hits.map((h) => toResourceItem(h, p.kind)));
  }
  return { total, items, warnings };
}

// ---- settings ----
export function toJavaInfo(r: RawJavaInfo): JavaInfo {
  return { path: r.path, home: r.home, version: r.version, major: r.major, arch: r.arch, vendor: r.vendor, isJdk: r.is_jdk };
}

function clamp01(n: number): number {
  return Number.isFinite(n) ? Math.min(1, Math.max(0, n)) : 0;
}
