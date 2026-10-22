import { call, useMockApi } from "./runtime";
import * as mock from "./mock/settings";
import type { BackendSettings, JavaInstall } from "./types";

export const SETTINGS_CMD = {
  get: "settings_get",
  set: "settings_set",
  scanJava: "settings_scan_java",
  openUrl: "settings_open_url",
} as const;

export interface SettingsApi {
  get(): Promise<BackendSettings>;
  set(s: BackendSettings): Promise<void>;
  scanJava(): Promise<JavaInstall[]>;
  openUrl(url: string): Promise<void>;
}

const real: SettingsApi = {
  get: () => call(SETTINGS_CMD.get),
  set: (settings) => call(SETTINGS_CMD.set, { settings }),
  scanJava: () => call(SETTINGS_CMD.scanJava),
  openUrl: (url) => call(SETTINGS_CMD.openUrl, { url }),
};

export const settingsApi: SettingsApi = useMockApi ? mock.settingsMock : real;
