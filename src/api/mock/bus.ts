import type { Unlisten } from "../runtime";

type Handler = (payload: unknown) => void;
const handlers = new Map<string, Set<Handler>>();

export const mockConfig = { latency: 120 };

export function delay(ms = mockConfig.latency): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}

export function mockEmit(event: string, payload: unknown): void {
  handlers.get(event)?.forEach((h) => h(payload));
}

export async function mockOn<T>(event: string, cb: (p: T) => void): Promise<Unlisten> {
  let set = handlers.get(event);
  if (!set) handlers.set(event, (set = new Set()));
  const h = cb as Handler;
  set.add(h);
  return () => set!.delete(h);
}

let seq = 0;
export const uid = (p: string) => `${p}-${Date.now().toString(36)}-${++seq}`;
