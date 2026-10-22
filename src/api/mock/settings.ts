import type { SettingsApi } from "../settings";
import { defaultGlobalSettings } from "../defaults";
import { deepMerge } from "../util";
import { delay } from "./bus";
import { MOCK_ROOT } from "./instances";

const seed = () => {
  const s = defaultGlobalSettings();
  s.game.folders = [{ name: "默认文件夹", path: MOCK_ROOT }];
  s.game.selected_folder = MOCK_ROOT;
  return s;
};
let state = seed();

export function resetSettingsMock(): void {
  state = seed();
}

export const settingsMock: SettingsApi = {
  async get() {
    await delay(40);
    return structuredClone(state);
  },
  async patch(p) {
    state = deepMerge(state, p);
    return structuredClone(state);
  },
  async reset() {
    state = seed();
    return structuredClone(state);
  },
  async path() {
    return "~/Library/Application Support/com.snowingfox.pcltauriport/settings.json";
  },
  async discoverJava() {
    await delay(500);
    return [
      { path: "/Library/Java/JavaVirtualMachines/temurin-21.jdk/Contents/Home/bin/java", home: "/Library/Java/JavaVirtualMachines/temurin-21.jdk/Contents/Home", version: "21.0.4", major: 21, arch: "arm64", vendor: "Eclipse Temurin", isJdk: true },
      { path: "/Library/Java/JavaVirtualMachines/temurin-17.jdk/Contents/Home/bin/java", home: "/Library/Java/JavaVirtualMachines/temurin-17.jdk/Contents/Home", version: "17.0.12", major: 17, arch: "arm64", vendor: "Eclipse Temurin", isJdk: true },
    ];
  },
  async inspectJava(path) {
    return { path, home: path, version: "21.0.4", major: 21, arch: "arm64", vendor: "Mock", isJdk: true };
  },
};
