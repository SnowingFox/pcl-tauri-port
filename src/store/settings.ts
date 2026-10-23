import { create } from "zustand";
import { settingsApi, type DeepPartial, type GlobalSettings } from "../api";
import { defaultGlobalSettings } from "../api/defaults";
import { deepMerge } from "../api/util";

interface SettingsState {
  value: GlobalSettings;
  loaded: boolean;
  load(): Promise<void>;
  /**
   * Applies the patch to the UI immediately, persists it with `settings_patch`, then adopts the sanitized
   * value the backend returns (unless a newer patch is already in flight). Failures roll back to the backend value.
   */
  patch(p: DeepPartial<GlobalSettings>): Promise<void>;
  reset(): Promise<void>;
}

let seq = 0;

export const useSettings = create<SettingsState>((set, get) => ({
  value: defaultGlobalSettings(),
  loaded: false,
  async load() {
    set({ value: await settingsApi.get(), loaded: true });
  },
  async patch(p) {
    const mine = ++seq;
    set({ value: deepMerge(get().value, p) });
    try {
      const saved = await settingsApi.patch(p);
      if (mine === seq) set({ value: saved });
    } catch (e) {
      if (mine === seq) {
        try { set({ value: await settingsApi.get() }); } catch { /* keep optimistic value */ }
      }
      throw e;
    }
  },
  async reset() {
    seq++;
    set({ value: await settingsApi.reset() });
  },
}));
