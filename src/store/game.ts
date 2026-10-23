import { create } from "zustand";
import { instancesApi, launchApi, settingsApi, type Instance, type LaunchExit, type LaunchLog, type LaunchProgress } from "../api";
import { incrementLaunchCount } from "../lib/counters";
import { actionOnGameExit, actionOnGameStart, runWindowAction } from "../lib/windowBehavior";
import { useSettings } from "./settings";

const MAX_LOG = 2000;
const FLUSH_MS = 60;

export type LaunchPhase = "idle" | "launching" | "running";

interface GameState {
  instances: Instance[];
  instancesLoaded: boolean;
  instancesError: string | null;
  selectedInstanceId: string | null;
  phase: LaunchPhase;
  launchId: string | null;
  /** instance of the current / last launch (the crash dialog needs it) */
  launchedInstance: Instance | null;
  progress: LaunchProgress | null;
  logs: LaunchLog[];
  lastExit: LaunchExit | null;
  loadInstances(): Promise<void>;
  selectInstance(id: string): void;
  deleteInstance(id: string, permanent?: boolean): Promise<void>;
  renameInstance(id: string, newName: string): Promise<Instance>;
  duplicateInstance(id: string, newName: string): Promise<Instance>;
  start(accountId: string): Promise<void>;
  kill(): Promise<void>;
  clearLogs(): void;
  dismissExit(): void;
  // event sinks (also used by tests)
  onProgress(p: LaunchProgress): void;
  onLog(l: LaunchLog): void;
  onExit(e: LaunchExit): void;
}

/** Log lines are buffered and flushed in batches so a chatty game does not re-render the UI per line. */
let pending: LaunchLog[] = [];
let flushTimer: ReturnType<typeof setTimeout> | null = null;
let startedHandled = false;

export function flushLogs(): void {
  if (flushTimer) { clearTimeout(flushTimer); flushTimer = null; }
  if (!pending.length) return;
  const batch = pending;
  pending = [];
  useGame.setState((s) => {
    const merged = s.logs.concat(batch);
    return { logs: merged.length > MAX_LOG ? merged.slice(merged.length - MAX_LOG) : merged };
  });
}

function visibility() {
  return useSettings.getState().value.game.launcher_visibility;
}

export const useGame = create<GameState>((set, get) => ({
  instances: [],
  instancesLoaded: false,
  instancesError: null,
  selectedInstanceId: null,
  phase: "idle",
  launchId: null,
  launchedInstance: null,
  progress: null,
  logs: [],
  lastExit: null,
  async loadInstances() {
    try {
      const s = await settingsApi.get();
      const root = s.game.selected_folder ?? s.game.folders[0]?.path;
      const instances = root ? (await instancesApi.scan(root)).filter((i) => !i.hidden) : [];
      const cur = get().selectedInstanceId;
      const remembered = s.game.selected_instance;
      const pick =
        instances.find((i) => i.id === cur && !i.error) ??
        instances.find((i) => i.name === remembered && !i.error) ??
        instances.find((i) => !i.error) ??
        instances[0];
      set({ instances, instancesLoaded: true, instancesError: null, selectedInstanceId: pick?.id ?? null });
    } catch (e) {
      set({ instancesLoaded: true, instancesError: String((e as Error).message ?? e) });
      throw e;
    }
  },
  selectInstance(id) {
    set({ selectedInstanceId: id });
    const inst = get().instances.find((i) => i.id === id);
    if (inst) void instancesApi.select(inst).catch(() => {});
  },
  async deleteInstance(id, permanent = false) {
    const inst = get().instances.find((i) => i.id === id);
    if (!inst) return;
    await instancesApi.remove(inst, permanent);
    await get().loadInstances();
  },
  async renameInstance(id, newName) {
    const inst = get().instances.find((i) => i.id === id);
    if (!inst) throw new Error("instance not found");
    const next = await instancesApi.rename(inst, newName);
    await get().loadInstances();
    set({ selectedInstanceId: next.id });
    return next;
  },
  async duplicateInstance(id, newName) {
    const inst = get().instances.find((i) => i.id === id);
    if (!inst) throw new Error("instance not found");
    const next = await instancesApi.duplicate(inst, newName);
    await get().loadInstances();
    return next;
  },
  async start(accountId) {
    const id = get().selectedInstanceId;
    const inst = get().instances.find((i) => i.id === id);
    if (!inst) throw new Error("no_instance");
    if (get().phase !== "idle") return;
    startedHandled = false;
    pending = [];
    set({ phase: "launching", progress: null, logs: [], lastExit: null, launchId: null, launchedInstance: inst });
    try {
      const launchId = await launchApi.start(inst.id, accountId);
      // the game process exists now; PCL counts the launch at this point
      incrementLaunchCount();
      // events may already have adopted the id; the exit event may even have fired
      if (get().phase !== "idle") set({ launchId });
    } catch (e) {
      set({ phase: "idle", launchId: null, progress: null });
      throw e;
    }
  },
  async kill() {
    const id = get().launchId;
    if (id) await launchApi.kill(id);
  },
  clearLogs: () => { pending = []; set({ logs: [] }); },
  dismissExit: () => set({ lastExit: null }),
  onProgress(p) {
    const { launchId, phase } = get();
    if (phase === "idle") return;
    if (launchId && p.launchId !== launchId) return;
    const running = p.stage === "running";
    set({ progress: p, launchId: p.launchId, phase: running ? "running" : "launching" });
    if (running && !startedHandled) {
      startedHandled = true;
      void runWindowAction(actionOnGameStart(visibility()));
    }
  },
  onLog(l) {
    const { launchId, phase } = get();
    if (phase === "idle" && !launchId) return;
    if (launchId && l.launchId !== launchId) return;
    pending.push(l);
    if (!flushTimer) flushTimer = setTimeout(flushLogs, FLUSH_MS);
  },
  onExit(e) {
    const { launchId } = get();
    if (launchId && e.launchId !== launchId) return;
    flushLogs();
    const clean = !e.crashed && (e.code === 0 || e.code === null);
    set({ phase: "idle", launchId: null, progress: null, lastExit: e });
    void runWindowAction(actionOnGameExit(visibility(), clean));
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
