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

const taskTimers = new Map<string, ReturnType<typeof setTimeout>>();
const taskPaused = new Set<string>();

/**
 * Simulates a download task: `download://progress` ticks then `download://finished`.
 * Pause/resume/cancel are honoured through the helpers below.
 */
export function mockRunTask(taskId: string, label: string, opts: { kind?: string; steps?: number; interval?: number; fail?: string } = {}): void {
  const steps = opts.steps ?? 10;
  const interval = opts.interval ?? 250;
  let n = 0;
  const finish = (status: "completed" | "failed" | "cancelled", error?: string) => {
    taskTimers.delete(taskId);
    mockEmit("download://finished", { task_id: taskId, kind: opts.kind ?? "install", label, status, error: error ?? null, last_progress: null });
  };
  const tick = () => {
    if (taskPaused.has(taskId)) {
      taskTimers.set(taskId, setTimeout(tick, interval));
      return;
    }
    n++;
    mockEmit("download://progress", {
      task_id: taskId, stage: n < steps ? "libraries" : "assets", done_bytes: n * 1_000_000, total_bytes: steps * 1_000_000,
      done_files: n, total_files: steps, speed_bps: 4_000_000, fraction: n / steps, state: "running",
      active_files: 24, active_connections: 61, concurrency: 64, segments: 4, worker_threads: 18, cores: 18, cpu_cores: 0.6,
    });
    if (opts.fail && n >= Math.ceil(steps / 2)) return finish("failed", opts.fail);
    if (n >= steps) return finish("completed");
    taskTimers.set(taskId, setTimeout(tick, interval));
  };
  taskTimers.set(taskId, setTimeout(tick, interval));
}
export function mockPauseTask(id: string): void { taskPaused.add(id); }
export function mockResumeTask(id: string): void { taskPaused.delete(id); }
export function mockCancelTask(id: string): void {
  const t = taskTimers.get(id);
  if (!t) return;
  clearTimeout(t);
  taskTimers.delete(id);
  taskPaused.delete(id);
  mockEmit("download://finished", { task_id: id, kind: "install", label: "", status: "cancelled", error: "cancelled", last_progress: null });
}
