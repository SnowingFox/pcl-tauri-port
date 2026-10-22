import { call, on, useMockApi } from "./runtime";
import * as mock from "./mock/launch";
import type { Instance, LaunchExit, LaunchLog, LaunchProgress, ModEntry, SaveEntry } from "./types";

export const LAUNCH_CMD = {
  listInstances: "launch_list_instances",
  start: "launch_start",
  kill: "launch_kill",
  deleteInstance: "launch_delete_instance",
  listMods: "launch_list_mods",
  toggleMod: "launch_toggle_mod",
  listSaves: "launch_list_saves",
  openFolder: "launch_open_instance_folder",
} as const;
export const LAUNCH_EVT = {
  progress: "launch://progress",
  log: "launch://log",
  exit: "launch://exit",
} as const;

export interface LaunchApi {
  listInstances(): Promise<Instance[]>;
  /** returns a launchId; progress/log/exit arrive via events */
  start(instanceId: string, accountId: string): Promise<string>;
  kill(launchId: string): Promise<void>;
  deleteInstance(instanceId: string): Promise<void>;
  listMods(instanceId: string): Promise<ModEntry[]>;
  toggleMod(instanceId: string, fileName: string, enabled: boolean): Promise<void>;
  listSaves(instanceId: string): Promise<SaveEntry[]>;
  openFolder(instanceId: string): Promise<void>;
  onProgress(cb: (p: LaunchProgress) => void): Promise<() => void>;
  onLog(cb: (l: LaunchLog) => void): Promise<() => void>;
  onExit(cb: (e: LaunchExit) => void): Promise<() => void>;
}

const real: LaunchApi = {
  listInstances: () => call(LAUNCH_CMD.listInstances),
  start: (instanceId, accountId) => call(LAUNCH_CMD.start, { instanceId, accountId }),
  kill: (launchId) => call(LAUNCH_CMD.kill, { launchId }),
  deleteInstance: (instanceId) => call(LAUNCH_CMD.deleteInstance, { instanceId }),
  listMods: (instanceId) => call(LAUNCH_CMD.listMods, { instanceId }),
  toggleMod: (instanceId, fileName, enabled) =>
    call(LAUNCH_CMD.toggleMod, { instanceId, fileName, enabled }),
  listSaves: (instanceId) => call(LAUNCH_CMD.listSaves, { instanceId }),
  openFolder: (instanceId) => call(LAUNCH_CMD.openFolder, { instanceId }),
  onProgress: (cb) => on(LAUNCH_EVT.progress, cb),
  onLog: (cb) => on(LAUNCH_EVT.log, cb),
  onExit: (cb) => on(LAUNCH_EVT.exit, cb),
};

export const launchApi: LaunchApi = useMockApi ? mock.launchMock : real;
