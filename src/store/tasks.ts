import { create } from "zustand";
import { downloadApi, type DownloadFinished, type DownloadProgress, type ModpackReport, type TaskStatus } from "../api";
import { reportError } from "../lib/notify";
import { useUi } from "./ui";
import { useGame } from "./game";

export interface TaskView {
  taskId: string;
  label: string;
  status: TaskStatus;
  stage: string;
  fraction: number;
  speedBps: number;
  doneFiles: number;
  totalFiles: number;
  error?: string;
}

interface TasksState {
  tasks: Record<string, TaskView>;
  order: string[];
  report: ModpackReport | null;
  /** register a task right after the start command returned its id */
  track(taskId: string, label: string): void;
  pause(taskId: string): Promise<void>;
  resume(taskId: string): Promise<void>;
  cancel(taskId: string): Promise<void>;
  clearFinished(): void;
  dismissReport(): void;
  onProgress(p: DownloadProgress): void;
  onFinished(f: DownloadFinished): void;
  onReport(r: ModpackReport): void;
}

const isDone = (s: TaskStatus) => s === "completed" || s === "failed" || s === "cancelled";
export const activeTasks = (s: Pick<TasksState, "tasks" | "order">): TaskView[] =>
  s.order.map((id) => s.tasks[id]).filter((t): t is TaskView => !!t && !isDone(t.status));

export const useTasks = create<TasksState>((set, get) => ({
  tasks: {},
  order: [],
  report: null,
  track(taskId, label) {
    set((s) => {
      const existing = s.tasks[taskId];
      return {
        tasks: { ...s.tasks, [taskId]: { ...(existing ?? { stage: "", fraction: 0, speedBps: 0, doneFiles: 0, totalFiles: 0 }), taskId, label, status: existing?.status ?? "running" } },
        order: s.order.includes(taskId) ? s.order : [...s.order, taskId],
      };
    });
  },
  async pause(taskId) {
    try { await downloadApi.pause(taskId); set((s) => patchTask(s, taskId, { status: "paused" })); } catch (e) { reportError(e); }
  },
  async resume(taskId) {
    try { await downloadApi.resume(taskId); set((s) => patchTask(s, taskId, { status: "running" })); } catch (e) { reportError(e); }
  },
  async cancel(taskId) {
    try { await downloadApi.cancel(taskId); } catch (e) { reportError(e); }
  },
  clearFinished() {
    void downloadApi.clearFinished().catch(() => {});
    set((s) => {
      const order = s.order.filter((id) => s.tasks[id] && !isDone(s.tasks[id].status));
      return { order, tasks: Object.fromEntries(order.map((id) => [id, s.tasks[id]])) };
    });
  },
  dismissReport: () => set({ report: null }),
  onProgress(p) {
    set((s) => {
      // events can arrive before `track` (the command returns after the first progress): create lazily
      const known = s.tasks[p.taskId];
      if (known && isDone(known.status)) return s;
      const base: TaskView = known ?? { taskId: p.taskId, label: "", status: "running", stage: "", fraction: 0, speedBps: 0, doneFiles: 0, totalFiles: 0 };
      const status: TaskStatus = p.state === "paused" ? "paused" : p.state === "cancelled" ? "cancelled" : "running";
      return {
        tasks: { ...s.tasks, [p.taskId]: { ...base, status, stage: p.stage, fraction: p.fraction, speedBps: p.speedBps, doneFiles: p.doneFiles, totalFiles: p.totalFiles } },
        order: s.order.includes(p.taskId) ? s.order : [...s.order, p.taskId],
      };
    });
  },
  onFinished(f) {
    const prev = get().tasks[f.taskId];
    set((s) => ({
      tasks: { ...s.tasks, [f.taskId]: { ...(prev ?? { stage: "", fraction: 0, speedBps: 0, doneFiles: 0, totalFiles: 0 }), taskId: f.taskId, label: prev?.label || f.label, status: f.status, error: f.error, fraction: f.status === "completed" ? 1 : (prev?.fraction ?? 0) } },
      order: s.order.includes(f.taskId) ? s.order : [...s.order, f.taskId],
    }));
    const ui = useUi.getState();
    const label = prev?.label || f.label;
    if (f.status === "completed") {
      ui.toast(`${label || "任务"} 已完成`, "success");
      void useGame.getState().loadInstances().catch(() => {});
    } else if (f.status === "failed") {
      ui.toast(`${label || "任务"} 失败：${f.error ?? "未知错误"}`, "error", 8000);
    }
  },
  onReport(r) {
    set({ report: r.missingFiles.length ? r : null });
  },
}));

function patchTask(s: TasksState, id: string, p: Partial<TaskView>): Partial<TasksState> {
  const t = s.tasks[id];
  return t ? { tasks: { ...s.tasks, [id]: { ...t, ...p } } } : {};
}

export async function bindTaskEvents(): Promise<() => void> {
  const uns = await Promise.all([
    downloadApi.onProgress((p) => useTasks.getState().onProgress(p)),
    downloadApi.onFinished((f) => useTasks.getState().onFinished(f)),
    downloadApi.onModpackReport((r) => useTasks.getState().onReport(r)),
  ]);
  return () => uns.forEach((u) => u());
}
