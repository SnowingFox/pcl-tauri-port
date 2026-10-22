/// <reference types="node" />
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";
import { stubLocalStorage } from "./helpers";

stubLocalStorage();

// Force real mode, then talk to a fake backend that, like Tauri, rejects commands that are not registered.
vi.stubEnv("VITE_MOCK_API", "false");

const libRs = readFileSync(new URL("../../src-tauri/src/lib.rs", import.meta.url), "utf-8");
const registered = new Set([...libRs.matchAll(/^\s*(?:[a-z_]+::)*([a-z_0-9]+),?\s*$/gm)].map((m) => m[1]));
// owned by feat/app-integration (docs/api/app.md); a real backend exposes them after the merge
const appCommands = ["app_launch", "app_launch_kill", "app_install_game", "app_install_resource", "app_install_modpack", "app_instance_mods", "app_instance_saves", "app_open_url", "app_reveal", "app_data_dirs"];
appCommands.forEach((c) => registered.add(c));

const ROOT = "/mc";
const INST = "/mc/versions/Fabric 1.21";
const calls: string[] = [];
const listeners = new Map<string, (e: { payload: unknown }) => void>();
const emit = (name: string, payload: unknown) => listeners.get(name)?.({ payload });

const settings = {
  schema_version: 1,
  game: { folders: [{ name: "默认文件夹", path: ROOT }], selected_folder: ROOT, selected_instance: null, isolation: "all", window_mode: "default", window_width: 854, window_height: 480, jvm_args: "", game_args: "", pre_launch_command: "", wait_for_pre_launch: true, launcher_visibility: "keep", process_priority: 1, custom_title: "", custom_info: "PCL" },
  memory: { mode: "auto", custom_mb: 4096, optimize_before_launch: false },
  java: { selected: null, custom_paths: [], prefer_64bit: true },
  download: { source: "mirror_first", version_source: "mirror_first", mod_source: "official_first", thread_count: 64, speed_limit_bps: 0, validate_ssl: true, translate_mod_names: true },
  ui: {}, window: {}, launcher: { debug_mode: false, check_update_release: false, check_update_snapshot: false, telemetry: false },
};

async function backend(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
  calls.push(cmd);
  if (!registered.has(cmd)) throw `Command ${cmd} not found`;
  switch (cmd) {
    case "settings_get": return settings;
    case "instances_scan":
      expect(args).toEqual({ root: ROOT });
      return [{ name: "Fabric 1.21", path: INST, root: ROOT, kind: "fabric", mc_version: "1.21", loaders: [{ kind: "fabric", version: "0.16.0" }], description: "正式版 1.21, Fabric 0.16.0", starred: false, hidden: false, modable: true, game_dir: INST, isolated: true }];
    case "instance_select": return undefined;
    case "accounts_list": return [{ id: "acc1", kind: "microsoft", name: "Steve", uuid: "u" }];
    case "accounts_get_selected": return "acc1";
    case "app_launch":
      expect(args).toEqual({ instanceId: INST, accountId: "acc1" });
      return { launchId: "L1" };
    default: throw `unexpected command in this scenario: ${cmd}`;
  }
}

vi.mock("@tauri-apps/api/core", () => ({ invoke: (cmd: string, args?: Record<string, unknown>) => backend(cmd, args) }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: async (name: string, cb: (e: { payload: unknown }) => void) => {
    listeners.set(name, cb);
    return () => listeners.delete(name);
  },
}));

type Stores = {
  game: typeof import("../store/game");
  settings: typeof import("../store/settings");
  accounts: typeof import("../store/accounts");
  prompt: typeof import("../store/prompt");
  runtime: typeof import("../api/runtime");
};
let s!: Stores;

beforeAll(async () => {
  vi.resetModules();
  s = {
    game: await import("../store/game"),
    settings: await import("../store/settings"),
    accounts: await import("../store/accounts"),
    prompt: await import("../store/prompt"),
    runtime: await import("../api/runtime"),
  };
  const { setWindowDriver } = await import("../lib/windowBehavior");
  setWindowDriver({ hide: async () => {}, show: async () => {}, minimize: async () => {}, unminimize: async () => {}, setFocus: async () => {}, close: async () => {} });
});
afterAll(() => vi.unstubAllEnvs());

describe("boot + launch against a fake backend in real mode", () => {
  it("uses the real API", () => expect(s.runtime.useMockApi).toBe(false));

  it("boot sequence only calls registered commands", async () => {
    await s.game.bindGameEvents();
    await s.settings.useSettings.getState().load();
    await s.game.useGame.getState().loadInstances();
    await s.accounts.useAccounts.getState().refresh();
    expect(s.game.useGame.getState().instances.map((i) => i.id)).toEqual([INST]);
    expect(s.game.useGame.getState().selectedInstanceId).toBe(INST);
    expect(s.accounts.useAccounts.getState().selectedId).toBe("acc1");
    expect(calls).toEqual(expect.arrayContaining(["settings_get", "instances_scan", "accounts_list", "accounts_get_selected"]));
  });

  it("launch -> progress stages -> running -> crash with analysis", async () => {
    const acc = s.accounts.useAccounts.getState().selected()!;
    await s.prompt.usePrompt.getState().requestLaunch(acc); // Microsoft account on launch #0: no prompt
    expect(calls).toContain("app_launch");
    expect(s.game.useGame.getState().launchId).toBe("L1");
    expect(s.game.useGame.getState().phase).toBe("launching");

    emit("launch://progress", { launchId: "L1", stage: "download", progress: 0.4, message: "补全文件" });
    expect(s.game.useGame.getState().progress?.stage).toBe("download");
    emit("launch://log", { id: "L1", stream: "launcher", line: "java -cp ***" });
    emit("launch://log", { id: "L1", stream: "stdout", line: "[main/INFO]: hello" });
    emit("launch://progress", { launchId: "L1", stage: "running", progress: 1, message: "" });
    expect(s.game.useGame.getState().phase).toBe("running");
    s.game.flushLogs();
    expect(s.game.useGame.getState().logs.map((l) => l.line)).toEqual(["java -cp ***", "[main/INFO]: hello"]);

    emit("launch://exit", {
      id: "L1", code: 1, crashed: true, durationMs: 9000,
      crash: { reasons: [{ id: "out_of_memory", details: [], message: "Minecraft 内存不足。", actions: [{ id: "increase_memory", label: "增加内存" }] }], summary: "Minecraft 内存不足。", files_used: [], primary_file: null },
    });
    const g = s.game.useGame.getState();
    expect(g.phase).toBe("idle");
    expect(g.lastExit?.crashed).toBe(true);
    expect(g.lastExit?.crash?.reasons[0].actions[0].id).toBe("increase_memory");
  });

  it("a structured launch error reaches the caller with its code", async () => {
    s.game.useGame.setState({ phase: "idle", launchId: null });
    vi.spyOn(await import("@tauri-apps/api/core"), "invoke").mockImplementationOnce(async () => {
      throw { code: "JAVA_MISSING", message: "no java", details: { requiredMajor: 21, found: [] } };
    });
    await expect(s.game.useGame.getState().start("acc1")).rejects.toMatchObject({ code: "JAVA_MISSING" });
    expect(s.game.useGame.getState().phase).toBe("idle");
  });
});
