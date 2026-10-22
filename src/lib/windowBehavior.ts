import type { LauncherVisibility } from "../api/types";
import { inTauri } from "../api/runtime";

/** What to do with the launcher window. Mirrors PCL's `LaunchArgumentVisible` handling (ModLaunch/ModWatcher). */
export type WindowAction = "none" | "hide" | "minimize" | "close" | "show";

/** Called when the game window is up (launch stage `running`). */
export function actionOnGameStart(v: LauncherVisibility): WindowAction {
  switch (v) {
    case "close_immediately": return "close";
    case "hide_then_close":
    case "hide": return "hide";
    case "minimize": return "minimize";
    default: return "none";
  }
}

/**
 * Called when the game process ended. `clean` is false for crashes and user cancels (PCL keeps the launcher
 * open then so the user can read the crash analysis).
 */
export function actionOnGameExit(v: LauncherVisibility, clean: boolean): WindowAction {
  if (v === "hide_then_close") return clean ? "close" : "show";
  if (v === "hide") return "show";
  if (v === "minimize") return "show"; // un-minimize so the crash dialog is seen
  return "none";
}

export interface WindowDriver {
  hide(): Promise<void>;
  show(): Promise<void>;
  minimize(): Promise<void>;
  unminimize(): Promise<void>;
  setFocus(): Promise<void>;
  close(): Promise<void>;
}

let driver: WindowDriver | null = null;

/** Lazily binds to the Tauri window; stays a no-op in a plain browser (mock mode). */
async function tauriDriver(): Promise<WindowDriver | null> {
  if (!inTauri) return null;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  const w = getCurrentWindow();
  return {
    hide: () => w.hide(),
    show: () => w.show(),
    minimize: () => w.minimize(),
    unminimize: () => w.unminimize(),
    setFocus: () => w.setFocus(),
    close: () => w.close(),
  };
}

/** Tests inject a fake driver. */
export function setWindowDriver(d: WindowDriver | null): void {
  driver = d;
}

export async function runWindowAction(a: WindowAction): Promise<void> {
  if (a === "none") return;
  const d = driver ?? (await tauriDriver());
  if (!d) return;
  try {
    switch (a) {
      case "hide": await d.hide(); break;
      case "minimize": await d.minimize(); break;
      case "close": await d.close(); break;
      case "show":
        await d.show();
        await d.unminimize();
        await d.setFocus();
        break;
    }
  } catch (e) {
    console.warn("window action failed", a, e);
  }
}
