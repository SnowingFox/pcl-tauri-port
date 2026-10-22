import type { LaunchApi } from "../launch";
import type { Instance, LaunchStage, ModEntry, SaveEntry } from "../types";
import { delay, mockEmit, mockOn, uid } from "./bus";

const instances: Instance[] = [
  { id: "1.21.1-fabric", name: "1.21.1 Fabric", mcVersion: "1.21.1", loader: "fabric", loaderVersion: "0.16.5", lastPlayed: Date.now() - 3600_000, favorite: true },
  { id: "1.20.1-forge", name: "1.20.1 Forge", mcVersion: "1.20.1", loader: "forge", loaderVersion: "47.3.0", lastPlayed: Date.now() - 86400_000 },
  { id: "1.21.4", name: "1.21.4", mcVersion: "1.21.4", loader: "vanilla" },
];
const mods = new Map<string, ModEntry[]>();
const timers = new Map<string, ReturnType<typeof setTimeout>[]>();

function modsFor(id: string): ModEntry[] {
  let m = mods.get(id);
  if (!m) {
    m = [
      { fileName: "sodium-0.6.0.jar", name: "Sodium", version: "0.6.0", enabled: true, size: 1_200_000 },
      { fileName: "lithium-0.14.0.jar", name: "Lithium", version: "0.14.0", enabled: true, size: 800_000 },
      { fileName: "iris-1.8.0.jar", name: "Iris Shaders", version: "1.8.0", enabled: false, size: 2_400_000 },
    ];
    mods.set(id, m);
  }
  return m;
}

const stages: [LaunchStage, string][] = [
  ["preparing", "准备启动环境"],
  ["checking", "检查游戏文件"],
  ["downloading", "补全缺失文件"],
  ["authenticating", "验证账号"],
  ["starting", "启动游戏进程"],
];

export const launchMock: LaunchApi = {
  async listInstances() {
    await delay();
    return instances.map((i) => ({ ...i }));
  },
  async start(instanceId, accountId) {
    await delay(60);
    const inst = instances.find((i) => i.id === instanceId);
    if (!inst) throw new Error("instance_not_found");
    if (!accountId) throw new Error("no_account");
    const launchId = uid("launch");
    const ts: ReturnType<typeof setTimeout>[] = [];
    timers.set(launchId, ts);
    const at = (ms: number, fn: () => void) => ts.push(setTimeout(fn, ms));
    stages.forEach(([stage, message], i) => {
      at(i * 450, () => {
        mockEmit("launch://progress", { launchId, stage, progress: (i + 1) / (stages.length + 1), message });
        mockEmit("launch://log", { launchId, level: "info", line: `[Launcher] ${message}`, ts: Date.now() });
      });
    });
    const t = stages.length * 450;
    at(t, () => {
      mockEmit("launch://progress", { launchId, stage: "running", progress: 1, message: "游戏运行中" });
      mockEmit("launch://log", { launchId, level: "info", line: `[main/INFO]: Minecraft ${inst.mcVersion} starting`, ts: Date.now() });
    });
    at(t + 600, () => mockEmit("launch://log", { launchId, level: "warn", line: "[main/WARN]: Mock warning line", ts: Date.now() }));
    at(t + 1200, () => mockEmit("launch://log", { launchId, level: "info", line: "[Render thread/INFO]: Sound engine started", ts: Date.now() }));
    return launchId;
  },
  async kill(launchId) {
    timers.get(launchId)?.forEach(clearTimeout);
    timers.delete(launchId);
    mockEmit("launch://exit", { launchId, code: 0, crashed: false });
  },
  async deleteInstance(id) {
    const i = instances.findIndex((x) => x.id === id);
    if (i >= 0) instances.splice(i, 1);
  },
  async listMods(id) {
    await delay();
    return modsFor(id).map((m) => ({ ...m }));
  },
  async toggleMod(id, fileName, enabled) {
    const m = modsFor(id).find((x) => x.fileName === fileName);
    if (m) m.enabled = enabled;
  },
  async listSaves(): Promise<SaveEntry[]> {
    await delay();
    return [
      { folder: "New World", name: "新的世界", lastPlayed: Date.now() - 7200_000, size: 52_000_000 },
      { folder: "Survival", name: "生存存档", lastPlayed: Date.now() - 5 * 86400_000, size: 310_000_000 },
    ];
  },
  async openFolder() {},
  onProgress: (cb) => mockOn("launch://progress", cb),
  onLog: (cb) => mockOn("launch://log", cb),
  onExit: (cb) => mockOn("launch://exit", cb),
};

/** test helper: emit a crash for a given launch */
export function mockCrash(launchId: string): void {
  mockEmit("launch://exit", { launchId, code: 1, crashed: true, reason: "Mock: 模组冲突导致崩溃" });
}
