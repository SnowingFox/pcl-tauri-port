import { call, useMockApi } from "./runtime";
import { toCrashAnalysis } from "./adapters";
import type { RawCrashAnalysis } from "./raw";
import type { CrashAnalysis, Instance } from "./types";

export const CRASH_CMD = {
  analyzeInstance: "crash_analyze_instance",
  analyzeFile: "crash_analyze_file",
  analyzeText: "crash_analyze_text",
} as const;

export interface CrashApi {
  /** analyzes logs written in the last 3 minutes (PCL behaviour) plus optional live output */
  analyzeInstance(inst: Pick<Instance, "root" | "name">, liveOutput?: string[]): Promise<CrashAnalysis>;
  analyzeFile(path: string): Promise<CrashAnalysis>;
  analyzeText(text: string): Promise<CrashAnalysis>;
}

const empty = (r: RawCrashAnalysis): CrashAnalysis => toCrashAnalysis(r) as CrashAnalysis;

export const realCrashApi: CrashApi = {
  analyzeInstance: async (i, liveOutput) =>
    empty(await call<RawCrashAnalysis>(CRASH_CMD.analyzeInstance, { root: i.root, name: i.name, liveOutput: liveOutput ?? null })),
  analyzeFile: async (path) => empty(await call<RawCrashAnalysis>(CRASH_CMD.analyzeFile, { path })),
  analyzeText: async (text) => empty(await call<RawCrashAnalysis>(CRASH_CMD.analyzeText, { text })),
};

const mockCrashApi: CrashApi = {
  async analyzeInstance() { return { reasons: [], summary: "", filesUsed: [], primaryFile: null }; },
  async analyzeFile() { return { reasons: [], summary: "", filesUsed: [], primaryFile: null }; },
  async analyzeText(text) {
    return /OutOfMemoryError/.test(text)
      ? { reasons: [{ id: "out_of_memory", message: "Minecraft 内存不足。", details: [], actions: [{ id: "increase_memory", label: "增加内存" }] }], summary: "Minecraft 内存不足。", filesUsed: [], primaryFile: null }
      : { reasons: [], summary: "", filesUsed: [], primaryFile: null };
  },
};

export const crashApi: CrashApi = useMockApi ? mockCrashApi : realCrashApi;
