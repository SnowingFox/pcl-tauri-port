import { create } from "zustand";
import { settingsApi, type BackendSettings } from "../api";
import { defaultBackendSettings } from "../api/mock/settings";

interface SettingsState {
  value: BackendSettings;
  loaded: boolean;
  load(): Promise<void>;
  update(patch: (draft: BackendSettings) => void): Promise<void>;
}

export const useSettings = create<SettingsState>((set, get) => ({
  value: defaultBackendSettings(),
  loaded: false,
  async load() {
    set({ value: await settingsApi.get(), loaded: true });
  },
  async update(patch) {
    const next = structuredClone(get().value);
    patch(next);
    set({ value: next });
    await settingsApi.set(next);
  },
}));
