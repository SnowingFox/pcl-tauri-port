import { create } from "zustand";
import { launchApi, type Instance, type LaunchExit, type LaunchLog, type LaunchProgress } from "../api";

const MAX_LOG = 2000;

export type LaunchPhase = "idle" | "launching" | "running";

interface GameState {
  instances: Instance[];
  selectedInstanceId: string | null;
  phase: LaunchPhase;
  launchId: string | null;
  progress: LaunchProgress | null;
  logs: LaunchLog[];
  lastExit: LaunchExit | null;
  loadInstances(): Promise<void>;
  selectInstance(id: string): void;
  deleteInstance(id: string): Promise<void>;
  start(accountId: string): Promise<void>;
  kill(): Promise<void>;
  clearLogs(): void;
  dismissExit(): void;
  // event sinks (also used by tests)
  onProgress(p: LaunchProgress): void;
  onLog(l: LaunchLog): void;
  onExit(e: LaunchExit): void;
}

export const useGame = create<GameState>((set, get) => ({
  instances: [],
  selectedInstanceId: null,
  phase: "idle",
  launchId: null,
  progress: null,
  logs: [],
  lastExit: null,
  async loadInstances() {
    const instances = await launchApi.listInstances();
    const cur = get().selectedInstanceId;
    set({
      instances,
      selectedInstanceId: instances.some((i) => i.id === cur) ? cur : (instances[0]?.id ?? null),
    });
  },
  selectInstance: (id) => set({ selectedInstanceId: id }),
  async deleteInstance(id) {
    await launchApi.deleteInstance(id);
    await get().loadInstances();
  },
  async start(accountId) {
    const id = get().selectedInstanceId;
    if (!id) throw new Error("no_instance");
    if (get().phase !== "idle") return;
    set({ phase: "launching", progress: null, logs: [], lastExit: null });
    try {
      const launchId = await launchApi.start(id, accountId);
      set({ launchId });
    } catch (e) {
      set({ phase: "idle", launchId: null });
      throw e;
    }
  },
  async kill() {
    const id = get().launchId;
    if (id) await launchApi.kill(id);
  },
  clearLogs: () => set({ logs: [] }),
  dismissExit: () => set({ lastExit: null }),
  onProgress(p) {
    const { launchId } = get();
    if (launchId && p.launchId !== launchId) return;
    set({ progress: p, launchId: p.launchId, phase: p.stage === "running" ? "running" : "launching" });
  },
  onLog(l) {
    const { launchId } = get();
    if (launchId && l.launchId !== launchId) return;
    set((s) => {
      const logs = s.logs.length >= MAX_LOG ? s.logs.slice(s.logs.length - MAX_LOG + 1) : s.logs.slice();
      logs.push(l);
      return { logs };
    });
  },
  onExit(e) {
    const { launchId } = get();
    if (launchId && e.launchId !== launchId) return;
    set({
      phase: "idle",
      launchId: null,
      progress: null,
      lastExit: e,
    });
  },
}));

/** Wire backend events into the store. Returns an unsubscribe function. */
export async function bindGameEvents(): Promise<() => void> {
  const uns = await Promise.all([
    launchApi.onProgress((p) => useGame.getState().onProgress(p)),
    launchApi.onLog((l) => useGame.getState().onLog(l)),
    launchApi.onExit((e) => useGame.getState().onExit(e)),
  ]);
  return () => uns.forEach((u) => u());
}
