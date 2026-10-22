import { call, on, useMockApi } from "./runtime";
import * as mock from "./mock/download";
import type { DownloadProgress, GameVersion, InstallRequest, LoaderKind } from "./types";

export const DOWNLOAD_CMD = {
  gameVersions: "download_game_versions",
  loaderVersions: "download_loader_versions",
  install: "download_install",
  cancel: "download_cancel",
} as const;
export const DOWNLOAD_EVT = { progress: "download://progress" } as const;

export interface DownloadApi {
  gameVersions(): Promise<GameVersion[]>;
  loaderVersions(loader: LoaderKind, mcVersion: string): Promise<string[]>;
  /** returns taskId */
  install(req: InstallRequest): Promise<string>;
  cancel(taskId: string): Promise<void>;
  onProgress(cb: (p: DownloadProgress) => void): Promise<() => void>;
}

const real: DownloadApi = {
  gameVersions: () => call(DOWNLOAD_CMD.gameVersions),
  loaderVersions: (loader, mcVersion) => call(DOWNLOAD_CMD.loaderVersions, { loader, mcVersion }),
  install: (req) => call(DOWNLOAD_CMD.install, { req }),
  cancel: (taskId) => call(DOWNLOAD_CMD.cancel, { taskId }),
  onProgress: (cb) => on(DOWNLOAD_EVT.progress, cb),
};

export const downloadApi: DownloadApi = useMockApi ? mock.downloadMock : real;
