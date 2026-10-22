import type { LaunchApi } from "../launch";
import type { LaunchStage } from "../types";
import { AppError } from "../errors";
import { delay, mockEmit, mockOn, uid } from "./bus";

const timers = new Map<string, ReturnType<typeof setTimeout>[]>();

const stages: [LaunchStage, string][] = [
  ["check", "检查游戏文件与账号"],
  ["java", "选择 Java"],
  ["download", "补全缺失文件"],
  ["natives", "解压本地库"],
  ["start", "启动游戏进程"],
];

export const launchMock: LaunchApi = {
  async start(instanceId, accountId) {
    await delay(60);
    if (!instanceId) throw new AppError("INSTANCE_NOT_FOUND", "instance not found");
    if (!accountId) throw new AppError("ACCOUNT_NOT_FOUND", "account not found");
    const launchId = uid("launch");
    const ts: ReturnType<typeof setTimeout>[] = [];
    timers.set(launchId, ts);
    const at = (ms: number, fn: () => void) => ts.push(setTimeout(fn, ms));
    stages.forEach(([stage, message], i) => {
      at(i * 450, () => {
        mockEmit("launch://progress", { launchId, stage, progress: (i + 1) / (stages.length + 1), message });
        mockEmit("launch://log", { id: launchId, stream: "launcher", line: `[Launcher] ${message}` });
      });
    });
    const t = stages.length * 450;
    at(t, () => {
      mockEmit("launch://progress", { launchId, stage: "running", progress: 1, message: "游戏运行中" });
      mockEmit("launch://log", { id: launchId, stream: "stdout", line: "[main/INFO]: Minecraft starting" });
    });
    at(t + 600, () => mockEmit("launch://log", { id: launchId, stream: "stdout", line: "[main/WARN]: Mock warning line" }));
    at(t + 1200, () => mockEmit("launch://log", { id: launchId, stream: "stdout", line: "[Render thread/INFO]: Sound engine started" }));
    return launchId;
  },
  async kill(launchId) {
    timers.get(launchId)?.forEach(clearTimeout);
    timers.delete(launchId);
    mockEmit("launch://exit", { id: launchId, code: null, crashed: false, crash: null, durationMs: 1000 });
  },
  onProgress: (cb) => mockOn("launch://progress", cb),
  onLog: (cb) => mockOn("launch://log", cb),
  onExit: (cb) => mockOn("launch://exit", cb),
};

/** test helper: emit a crash for a given launch */
export function mockCrash(launchId: string): void {
  mockEmit("launch://exit", {
    id: launchId, code: 1, crashed: true, durationMs: 4200,
    crash: {
      reasons: [{ id: "out_of_memory", details: [], message: "Minecraft 内存不足。", actions: [{ id: "increase_memory", label: "增加内存" }] }],
      summary: "Minecraft 内存不足。", filesUsed: [], primaryFile: null,
    },
  });
}
