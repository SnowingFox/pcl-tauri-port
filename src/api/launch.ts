import { call, on, useMockApi } from "./runtime";
import * as mock from "./mock/launch";
import { toLaunchExit, toLaunchLog, toLaunchProgress } from "./adapters";
import type { RawExitEvent, RawLaunchProgress, RawLogEvent } from "./raw";
import type { LaunchExit, LaunchLog, LaunchProgress } from "./types";

export const LAUNCH_CMD = {
  launch: "app_launch",
  kill: "app_launch_kill",
} as const;
export const LAUNCH_EVT = {
  progress: "launch://progress",
  log: "launch://log",
  exit: "launch://exit",
} as const;

export interface LaunchApi {
  /**
   * Runs the whole pipeline (check, Java, repair downloads, natives, spawn) and resolves with the launchId
   * once the process is spawned. Progress, log and exit arrive via events.
   */
  start(instanceId: string, accountId: string): Promise<string>;
  kill(launchId: string): Promise<void>;
  onProgress(cb: (p: LaunchProgress) => void): Promise<() => void>;
  onLog(cb: (l: LaunchLog) => void): Promise<() => void>;
  onExit(cb: (e: LaunchExit) => void): Promise<() => void>;
}

/** The real (Tauri) implementation; exported for contract tests. */
export const realLaunchApi: LaunchApi = {
  async start(instanceId, accountId) {
    const r = await call<{ launchId: string }>(LAUNCH_CMD.launch, { instanceId, accountId });
    return r.launchId;
  },
  kill: (launchId) => call(LAUNCH_CMD.kill, { launchId }),
  onProgress: (cb) => on<RawLaunchProgress>(LAUNCH_EVT.progress, (p) => cb(toLaunchProgress(p))),
  onLog: (cb) => on<RawLogEvent>(LAUNCH_EVT.log, (l) => cb(toLaunchLog(l))),
  onExit: (cb) => on<RawExitEvent>(LAUNCH_EVT.exit, (e) => cb(toLaunchExit(e))),
};

export const launchApi: LaunchApi = useMockApi ? mock.launchMock : realLaunchApi;
