import { create } from "zustand";

export type ThemeMode = "light" | "dark" | "system";
export type HueName = "blue" | "purple" | "green" | "orange" | "pink" | "red";

/** Blue is the default main hue (required by the upstream licence). */
export const DEFAULT_HUE: HueName = "blue";
export const HUES: Record<HueName, number> = { blue: 215, purple: 265, green: 150, orange: 30, pink: 330, red: 5 };

export interface ToastAction {
  label: string;
  /** in-app route to navigate to */
  route: string;
}
export interface Toast {
  id: number;
  kind: "info" | "success" | "error";
  text: string;
  action?: ToastAction;
}

export interface UiState {
  mode: ThemeMode;
  hue: HueName;
  reduceMotion: boolean;
  toasts: Toast[];
  setMode(m: ThemeMode): void;
  setHue(h: HueName): void;
  setReduceMotion(v: boolean): void;
  toast(text: string, kind?: Toast["kind"], ms?: number, action?: ToastAction): number;
  dismissToast(id: number): void;
}

const KEY = "pcl.ui";
type Persisted = Pick<UiState, "mode" | "hue" | "reduceMotion">;

function load(): Persisted {
  const def: Persisted = { mode: "system", hue: DEFAULT_HUE, reduceMotion: false };
  try {
    const raw = typeof localStorage !== "undefined" ? localStorage.getItem(KEY) : null;
    if (!raw) return def;
    const p = JSON.parse(raw) as Partial<Persisted>;
    return {
      mode: p.mode === "light" || p.mode === "dark" || p.mode === "system" ? p.mode : def.mode,
      hue: p.hue && p.hue in HUES ? p.hue : def.hue,
      reduceMotion: !!p.reduceMotion,
    };
  } catch {
    return def;
  }
}

function save(s: Persisted): void {
  try {
    localStorage.setItem(KEY, JSON.stringify({ mode: s.mode, hue: s.hue, reduceMotion: s.reduceMotion }));
  } catch {
    /* storage unavailable */
  }
}

let toastSeq = 0;
/** Auto-dismiss timers by toast id, so a repeated toast can restart its own countdown. */
const toastTimers = new Map<number, ReturnType<typeof setTimeout>>();

export const useUi = create<UiState>((set, get) => ({
  ...load(),
  toasts: [],
  setMode: (mode) => { set({ mode }); save(get()); },
  setHue: (hue) => { set({ hue }); save(get()); },
  setReduceMotion: (reduceMotion) => { set({ reduceMotion }); save(get()); },
  toast: (text, kind = "info", ms = 3500, action) => {
    // The same message that is already on screen (a retried action failing the same way) is not
    // stacked again: it stays as one toast and its countdown restarts.
    const same = get().toasts.find((t) => t.kind === kind && t.text === text && t.action?.route === action?.route);
    const id = same ? same.id : ++toastSeq;
    if (!same) set((s) => ({ toasts: [...s.toasts.slice(-4), { id, kind, text, action }] }));
    clearTimeout(toastTimers.get(id));
    toastTimers.delete(id);
    if (ms > 0) toastTimers.set(id, setTimeout(() => get().dismissToast(id), ms));
    return id;
  },
  dismissToast: (id) => {
    clearTimeout(toastTimers.get(id));
    toastTimers.delete(id);
    set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) }));
  },
}));
