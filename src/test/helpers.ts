import { vi } from "vitest";

/** Minimal localStorage for the node test environment. */
export function stubLocalStorage(): Map<string, string> {
  const mem = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (k: string) => mem.get(k) ?? null,
    setItem: (k: string, v: string) => void mem.set(k, v),
    removeItem: (k: string) => void mem.delete(k),
  });
  return mem;
}
