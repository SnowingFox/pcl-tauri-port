import type { DownloadApi } from "../download";
import type { GameVersion } from "../types";
import { delay, mockEmit, mockOn, uid } from "./bus";

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
const cancelled = new Set<string>();

export const downloadMock: DownloadApi = {
  async gameVersions() {
    await delay();
    return versions.map((v) => ({ ...v }));
  },
  async loaderVersions(loader, mc) {
    await delay();
    if (loader === "vanilla") return [];
    return [`${loader}-${mc}-3`, `${loader}-${mc}-2`, `${loader}-${mc}-1`];
  },
  async install(req) {
    await delay(60);
    if (!req.name.trim()) throw new Error("invalid_name");
    const taskId = uid("dl");
    let n = 0;
    const tick = () => {
      if (cancelled.has(taskId)) {
        mockEmit("download://progress", { taskId, progress: n / 10, message: "已取消", done: true, error: "cancelled" });
        return;
      }
      n++;
      mockEmit("download://progress", { taskId, progress: n / 10, message: n < 10 ? "下载文件中" : "安装完成", done: n >= 10 });
      if (n < 10) setTimeout(tick, 250);
    };
    setTimeout(tick, 250);
    return taskId;
  },
  async cancel(taskId) {
    cancelled.add(taskId);
  },
  onProgress: (cb) => mockOn("download://progress", cb),
};
