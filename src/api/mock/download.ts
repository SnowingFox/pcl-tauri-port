import type { DownloadApi } from "../download";
import type { GameVersion } from "../types";
import { AppError } from "../errors";
import { delay, mockCancelTask, mockOn, mockPauseTask, mockResumeTask, mockRunTask, uid } from "./bus";

const versions: GameVersion[] = [
  { id: "1.21.4", type: "release", releaseTime: "2024-12-03" },
  { id: "25w05a", type: "snapshot", releaseTime: "2025-01-29" },
  { id: "1.21.1", type: "release", releaseTime: "2024-08-08" },
  { id: "1.20.1", type: "release", releaseTime: "2023-06-12" },
  { id: "1.16.5", type: "release", releaseTime: "2021-01-15" },
  { id: "1.12.2", type: "release", releaseTime: "2017-09-18" },
  { id: "b1.7.3", type: "old_beta", releaseTime: "2011-07-08" },
  { id: "a1.2.6", type: "old_alpha", releaseTime: "2010-12-03" },
];

export const downloadMock: DownloadApi = {
  async gameVersions() {
    await delay();
    return versions.map((v) => ({ ...v }));
  },
  async loaderVersions(loader, mc) {
    await delay();
    if (loader === "vanilla") return [];
    return [3, 2, 1].map((n) => ({ version: `${loader}-${mc}-${n}`, stable: n !== 3 }));
  },
  async installGame(req) {
    await delay(60);
    if (!req.mcVersion) throw new AppError("INVALID_ARGUMENT", "mcVersion is required");
    const taskId = uid("dl");
    mockRunTask(taskId, `${req.mcVersion}${req.loader ? ` ${req.loader.kind}` : ""}`);
    return taskId;
  },
  async pause(id) { mockPauseTask(id); },
  async resume(id) { mockResumeTask(id); },
  async cancel(id) { mockCancelTask(id); },
  async listTasks() { return []; },
  async clearFinished() {},
  onProgress: (cb) => mockOn("download://progress", (p: Record<string, unknown>) => cb(adaptProgress(p))),
  onFinished: (cb) => mockOn("download://finished", (f: Record<string, unknown>) => cb(adaptFinished(f))),
  onModpackReport: (cb) => mockOn("app://modpack-report", cb),
};

import { toDownloadFinished, toDownloadProgress } from "../adapters";
import type { RawProgressEvent, RawTaskInfo } from "../raw";
const adaptProgress = (p: Record<string, unknown>) => toDownloadProgress(p as unknown as RawProgressEvent);
const adaptFinished = (f: Record<string, unknown>) => toDownloadFinished(f as unknown as RawTaskInfo);
