import type { SettingsApi } from "../settings";
import type { BackendSettings } from "../types";
import { delay } from "./bus";

export const defaultBackendSettings = (): BackendSettings => ({
  java: { autoSelect: true, path: "", autoMemory: true, memoryMb: 4096, jvmArgs: "" },
  launch: { gameArgs: "", width: 854, height: 480, fullscreen: false, onLaunch: "keep" },
  download: { source: "bmclapi", threads: 16, verifyFiles: true },
});

let state = defaultBackendSettings();

export function resetSettingsMock(): void {
  state = defaultBackendSettings();
}

export const settingsMock: SettingsApi = {
  async get() {
    await delay(40);
    return structuredClone(state);
  },
  async set(s) {
    state = structuredClone(s);
  },
  async scanJava() {
    await delay(500);
    return [
      { path: "/Library/Java/JavaVirtualMachines/temurin-21.jdk/Contents/Home/bin/java", version: "21.0.4", vendor: "Eclipse Temurin", is64: true },
      { path: "/Library/Java/JavaVirtualMachines/temurin-17.jdk/Contents/Home/bin/java", version: "17.0.12", vendor: "Eclipse Temurin", is64: true },
    ];
  },
  async openUrl(url) {
    if (typeof window !== "undefined") window.open(url, "_blank", "noopener,noreferrer");
  },
};
