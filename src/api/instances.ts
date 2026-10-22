import { call, useMockApi } from "./runtime";
import * as mock from "./mock/instances";
import { toInstance, toModEntry, toSave } from "./adapters";
import type { RawInstanceInfo, RawLocalMod, RawSave } from "./raw";
import type { Instance, InstanceSettings, McFolder, ModEntry, OpenTarget, SaveEntry } from "./types";

export const INSTANCE_CMD = {
  foldersList: "mc_folders_list",
  folderAdd: "mc_folder_add",
  folderRemove: "mc_folder_remove",
  folderSelect: "mc_folder_select",
  scan: "instances_scan",
  select: "instance_select",
  rename: "instance_rename",
  duplicate: "instance_duplicate",
  delete: "instance_delete",
  openInFinder: "instance_open_in_finder",
  settingsGet: "instance_settings_get",
  settingsSet: "instance_settings_set",
  mods: "app_instance_mods",
  modSetEnabled: "local_mod_set_enabled",
  saves: "app_instance_saves",
} as const;

type Ref = Pick<Instance, "root" | "name">;

export interface InstancesApi {
  folders(): Promise<McFolder[]>;
  addFolder(name: string, path: string): Promise<McFolder[]>;
  removeFolder(path: string): Promise<McFolder[]>;
  selectFolder(path: string): Promise<void>;
  scan(root: string): Promise<Instance[]>;
  select(inst: Ref): Promise<void>;
  rename(inst: Ref, newName: string): Promise<Instance>;
  duplicate(inst: Ref, newName: string): Promise<Instance>;
  /** moves to the Trash unless `permanent`; returns the new location */
  remove(inst: Ref, permanent?: boolean): Promise<string>;
  openIn(inst: Ref, target: OpenTarget): Promise<void>;
  getSettings(inst: Pick<Instance, "path">): Promise<InstanceSettings>;
  setSettings(inst: Pick<Instance, "path">, s: InstanceSettings): Promise<void>;
  mods(inst: Pick<Instance, "id">): Promise<ModEntry[]>;
  /** returns the new absolute path of the mod file */
  setModEnabled(mod: ModEntry, enabled: boolean): Promise<string>;
  saves(inst: Pick<Instance, "id">): Promise<SaveEntry[]>;
}

/** The real (Tauri) implementation; exported for contract tests. */
export const realInstancesApi: InstancesApi = {
  folders: () => call(INSTANCE_CMD.foldersList),
  addFolder: (name, path) => call(INSTANCE_CMD.folderAdd, { name, path }),
  removeFolder: (path) => call(INSTANCE_CMD.folderRemove, { path }),
  selectFolder: (path) => call(INSTANCE_CMD.folderSelect, { path }),
  scan: async (root) => (await call<RawInstanceInfo[]>(INSTANCE_CMD.scan, { root })).map(toInstance),
  select: (i) => call(INSTANCE_CMD.select, { root: i.root, name: i.name }),
  rename: async (i, newName) => toInstance(await call<RawInstanceInfo>(INSTANCE_CMD.rename, { root: i.root, name: i.name, newName })),
  duplicate: async (i, newName) => toInstance(await call<RawInstanceInfo>(INSTANCE_CMD.duplicate, { root: i.root, name: i.name, newName })),
  remove: (i, permanent = false) => call(INSTANCE_CMD.delete, { root: i.root, name: i.name, permanent }),
  openIn: (i, target) => call(INSTANCE_CMD.openInFinder, { root: i.root, name: i.name, target }),
  getSettings: (i) => call(INSTANCE_CMD.settingsGet, { instanceDir: i.path }),
  setSettings: (i, settings) => call(INSTANCE_CMD.settingsSet, { instanceDir: i.path, settings }),
  mods: async (i) => (await call<RawLocalMod[]>(INSTANCE_CMD.mods, { instanceId: i.id })).map(toModEntry),
  setModEnabled: (mod, enabled) => call(INSTANCE_CMD.modSetEnabled, { path: mod.path, enabled }),
  saves: async (i) => (await call<RawSave[]>(INSTANCE_CMD.saves, { instanceId: i.id })).map(toSave),
};

export const instancesApi: InstancesApi = useMockApi ? mock.instancesMock : realInstancesApi;
