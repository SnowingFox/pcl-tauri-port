import { call, useMockApi } from "./runtime";
import * as mock from "./mock/settings";
import { toJavaInfo } from "./adapters";
import type { RawJavaInfo } from "./raw";
import type { DeepPartial, GlobalSettings, JavaInfo } from "./types";

export const SETTINGS_CMD = {
  get: "settings_get",
  patch: "settings_patch",
  reset: "settings_reset",
  path: "settings_path",
  javaDiscover: "java_discover",
  javaInspect: "java_inspect",
} as const;

export interface SettingsApi {
  get(): Promise<GlobalSettings>;
  /** recursive merge; the backend validates and returns the sanitized result */
  patch(p: DeepPartial<GlobalSettings>): Promise<GlobalSettings>;
  reset(): Promise<GlobalSettings>;
  path(): Promise<string>;
  discoverJava(extraDirs?: string[]): Promise<JavaInfo[]>;
  inspectJava(path: string): Promise<JavaInfo>;
}

/** The real (Tauri) implementation; exported for contract tests. */
export const realSettingsApi: SettingsApi = {
  get: () => call(SETTINGS_CMD.get),
  patch: (patch) => call(SETTINGS_CMD.patch, { patch }),
  reset: () => call(SETTINGS_CMD.reset),
  path: () => call(SETTINGS_CMD.path),
  discoverJava: async (extraDirs) => (await call<RawJavaInfo[]>(SETTINGS_CMD.javaDiscover, { extraDirs: extraDirs ?? null })).map(toJavaInfo),
  inspectJava: async (path) => toJavaInfo(await call<RawJavaInfo>(SETTINGS_CMD.javaInspect, { path })),
};

export const settingsApi: SettingsApi = useMockApi ? mock.settingsMock : realSettingsApi;
