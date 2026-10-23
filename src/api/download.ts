import { call, on, useMockApi } from "./runtime";
import * as mock from "./mock/download";
import { toDownloadFinished, toDownloadProgress, toDownloadTask, toGameVersions, toModpackReport } from "./adapters";
import type { RawProgressEvent, RawModpackReport, RawTaskInfo, RawVersionManifest } from "./raw";
import type {
  DownloadFinished, DownloadProgress, DownloadTask, GameVersion, InstallGameRequest, LoaderKind, LoaderVersion, ModpackReport,
} from "./types";

export const DOWNLOAD_CMD = {
  manifest: "download_version_manifest",
  loaderVersions: "install_list_loader_versions",
  optifineVersions: "install_list_optifine",
  installGame: "app_install_game",
  pause: "download_pause",
  resume: "download_resume",
  cancel: "download_cancel",
  listTasks: "download_list_tasks",
  clearFinished: "download_clear_finished",
} as const;
export const DOWNLOAD_EVT = {
  progress: "download://progress",
  finished: "download://finished",
  modpackReport: "app://modpack-report",
} as const;

export interface DownloadApi {
  gameVersions(force?: boolean): Promise<GameVersion[]>;
  loaderVersions(loader: LoaderKind, mcVersion: string): Promise<LoaderVersion[]>;
  /** returns the task id; progress/finish arrive via events */
  installGame(req: InstallGameRequest): Promise<string>;
  pause(taskId: string): Promise<void>;
  resume(taskId: string): Promise<void>;
  cancel(taskId: string): Promise<void>;
  listTasks(): Promise<DownloadTask[]>;
  clearFinished(): Promise<void>;
  onProgress(cb: (p: DownloadProgress) => void): Promise<() => void>;
  onFinished(cb: (f: DownloadFinished) => void): Promise<() => void>;
  onModpackReport(cb: (r: ModpackReport) => void): Promise<() => void>;
}

/** The real (Tauri) implementation; exported for contract tests. */
export const realDownloadApi: DownloadApi = {
  async gameVersions(force) {
    return toGameVersions(await call<RawVersionManifest>(DOWNLOAD_CMD.manifest, { force: force ?? false }));
  },
  async loaderVersions(loader, mcVersion) {
    if (loader === "vanilla") return [];
    return call<LoaderVersion[]>(DOWNLOAD_CMD.loaderVersions, { loader, mcVersion });
  },
  async installGame(req) {
    const r = await call<{ taskId: string }>(DOWNLOAD_CMD.installGame, {
      mcVersion: req.mcVersion,
      loader: req.loader ?? null,
      instanceName: req.instanceName ?? null,
    });
    return r.taskId;
  },
  pause: (taskId) => call(DOWNLOAD_CMD.pause, { taskId }),
  resume: (taskId) => call(DOWNLOAD_CMD.resume, { taskId }),
  cancel: (taskId) => call(DOWNLOAD_CMD.cancel, { taskId }),
  listTasks: async () => (await call<RawTaskInfo[]>(DOWNLOAD_CMD.listTasks)).map(toDownloadTask),
  clearFinished: () => call(DOWNLOAD_CMD.clearFinished),
  onProgress: (cb) => on<RawProgressEvent>(DOWNLOAD_EVT.progress, (p) => cb(toDownloadProgress(p))),
  onFinished: (cb) => on<RawTaskInfo>(DOWNLOAD_EVT.finished, (f) => cb(toDownloadFinished(f))),
  onModpackReport: (cb) => on<RawModpackReport>(DOWNLOAD_EVT.modpackReport, (r) => cb(toModpackReport(r))),
};

export const downloadApi: DownloadApi = useMockApi ? mock.downloadMock : realDownloadApi;
