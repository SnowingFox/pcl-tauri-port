import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/** True when running inside a Tauri webview. */
export const inTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/**
 * Mock API is on when VITE_MOCK_API=true, or when not inside Tauri
 * (unless VITE_MOCK_API=false is set explicitly).
 */
export const useMockApi: boolean =
  import.meta.env.VITE_MOCK_API === "true" ||
  (import.meta.env.VITE_MOCK_API !== "false" && !inTauri);

export function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(cmd, args);
}

export type Unlisten = () => void;

/** Subscribe to a backend event (real Tauri event). */
export async function on<T>(event: string, cb: (payload: T) => void): Promise<Unlisten> {
  const un = await listen<T>(event, (e) => cb(e.payload));
  return un;
}
