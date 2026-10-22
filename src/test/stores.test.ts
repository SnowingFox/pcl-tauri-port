import { beforeEach, describe, expect, it } from "vitest";
import { stubLocalStorage } from "./helpers";

const mem = stubLocalStorage();

import { useUi, DEFAULT_HUE } from "../store/ui";
import { flushLogs, useGame } from "../store/game";
import { useAccounts } from "../store/accounts";
import { useSettings } from "../store/settings";
import { activeTasks, useTasks } from "../store/tasks";
import { t, tRaw } from "../i18n";
import { resetAccountsMock } from "../api/mock/accounts";
import { resetInstancesMock } from "../api/mock/instances";
import { resetSettingsMock } from "../api/mock/settings";
import { mockConfig } from "../api/mock/bus";
import { setWindowDriver } from "../lib/windowBehavior";

mockConfig.latency = 0;
setWindowDriver({ hide: async () => {}, show: async () => {}, minimize: async () => {}, unminimize: async () => {}, setFocus: async () => {}, close: async () => {} });

describe("ui store", () => {
  beforeEach(() => { mem.clear(); useUi.setState({ toasts: [] }); });
  it("defaults to blue hue (licence)", () => {
    expect(DEFAULT_HUE).toBe("blue");
    expect(useUi.getState().hue).toBe("blue");
  });
  it("persists theme choice", () => {
    useUi.getState().setHue("green");
    expect(JSON.parse(mem.get("pcl.ui")!).hue).toBe("green");
  });
  it("toast add/dismiss and optional action", () => {
    const id = useUi.getState().toast("hi", "info", 0, { label: "go", route: "/settings/java" });
    expect(useUi.getState().toasts[0].action).toEqual({ label: "go", route: "/settings/java" });
    useUi.getState().dismissToast(id);
    expect(useUi.getState().toasts).toHaveLength(0);
  });
  it("keeps at most five toasts", () => {
    for (let i = 0; i < 9; i++) useUi.getState().toast(String(i), "info", 0);
    expect(useUi.getState().toasts).toHaveLength(5);
    expect(useUi.getState().toasts.at(-1)?.text).toBe("8");
  });
});

describe("game store events", () => {
  beforeEach(() => useGame.setState({ phase: "idle", launchId: null, progress: null, logs: [], lastExit: null }));
  const log = (launchId: string, line: string) => ({ launchId, level: "info" as const, stream: "stdout" as const, line, ts: 0 });

  it("tracks progress, logs and exit for the active launch only", () => {
    useGame.setState({ launchId: "L1", phase: "launching" });
    const g = useGame.getState();
    g.onProgress({ launchId: "L1", stage: "java", progress: 0.3, message: "x" });
    expect(useGame.getState().phase).toBe("launching");
    g.onProgress({ launchId: "L1", stage: "running", progress: 1, message: "x" });
    expect(useGame.getState().phase).toBe("running");
    g.onLog(log("L1", "a"));
    g.onLog(log("other", "b"));
    flushLogs();
    expect(useGame.getState().logs.map((l) => l.line)).toEqual(["a"]);
    g.onExit({ launchId: "other", code: 1, crashed: true, crash: null });
    expect(useGame.getState().phase).toBe("running");
    g.onExit({ launchId: "L1", code: 1, crashed: true, crash: null });
    expect(useGame.getState().phase).toBe("idle");
    expect(useGame.getState().lastExit?.crashed).toBe(true);
  });
  it("adopts the launch id from the first progress event when start() has not returned yet", () => {
    useGame.setState({ phase: "launching", launchId: null });
    useGame.getState().onProgress({ launchId: "early", stage: "check", progress: 0.1, message: "" });
    expect(useGame.getState().launchId).toBe("early");
  });
  it("ignores stray events while idle", () => {
    useGame.getState().onProgress({ launchId: "x", stage: "check", progress: 0.1, message: "" });
    useGame.getState().onLog(log("x", "late"));
    flushLogs();
    expect(useGame.getState().phase).toBe("idle");
    expect(useGame.getState().logs).toHaveLength(0);
  });
  it("caps the log buffer", () => {
    useGame.setState({ launchId: "L", phase: "running" });
    for (let i = 0; i < 2100; i++) useGame.getState().onLog(log("L", String(i)));
    flushLogs();
    expect(useGame.getState().logs).toHaveLength(2000);
    expect(useGame.getState().logs.at(-1)?.line).toBe("2099");
  });
});

describe("game store instances (mock api)", () => {
  beforeEach(() => {
    resetSettingsMock(); resetInstancesMock();
    useGame.setState({ instances: [], selectedInstanceId: null, phase: "idle", launchId: null, instancesLoaded: false });
  });
  it("loads the instances of the selected folder and picks a launchable one", async () => {
    await useGame.getState().loadInstances();
    const s = useGame.getState();
    expect(s.instancesLoaded).toBe(true);
    expect(s.instances.length).toBeGreaterThan(0);
    expect(s.selectedInstanceId).toBe(s.instances[0].id);
  });
  it("renames, duplicates and deletes", async () => {
    await useGame.getState().loadInstances();
    const first = useGame.getState().instances[0];
    const renamed = await useGame.getState().renameInstance(first.id, "Renamed");
    expect(renamed.name).toBe("Renamed");
    expect(useGame.getState().selectedInstanceId).toBe(renamed.id);
    await useGame.getState().duplicateInstance(renamed.id, "Renamed copy");
    expect(useGame.getState().instances.some((i) => i.name === "Renamed copy")).toBe(true);
    await expect(useGame.getState().duplicateInstance(renamed.id, "bad/name")).rejects.toThrow();
    await useGame.getState().deleteInstance(renamed.id);
    expect(useGame.getState().instances.some((i) => i.name === "Renamed")).toBe(false);
  });
  it("start() counts the launch and returns to idle on failure", async () => {
    await useGame.getState().loadInstances();
    await expect(useGame.getState().start("")).rejects.toMatchObject({ code: "ACCOUNT_NOT_FOUND" });
    expect(useGame.getState().phase).toBe("idle");
  });
});

describe("accounts store (mock api)", () => {
  beforeEach(() => { resetAccountsMock(); useAccounts.setState({ accounts: [], selectedId: null }); });
  it("adds an offline account, selects it and removes it", async () => {
    const a = await useAccounts.getState().addOffline("Steve");
    expect(useAccounts.getState().selectedId).toBe(a.id);
    await useAccounts.getState().remove(a.id);
    expect(useAccounts.getState().accounts).toHaveLength(0);
  });
  it("rejects invalid names", async () => {
    await expect(useAccounts.getState().addOffline("a b")).rejects.toThrow();
  });
});

describe("settings store (mock api)", () => {
  beforeEach(() => { resetSettingsMock(); });
  it("patches optimistically and keeps the backend result", async () => {
    await useSettings.getState().load();
    const p = useSettings.getState().patch({ game: { launcher_visibility: "hide" } });
    expect(useSettings.getState().value.game.launcher_visibility).toBe("hide");
    await p;
    expect(useSettings.getState().value.game.launcher_visibility).toBe("hide");
    expect(useSettings.getState().value.game.folders.length).toBe(1); // untouched keys survive the merge
  });
  it("resets to defaults", async () => {
    await useSettings.getState().patch({ download: { thread_count: 7 } });
    await useSettings.getState().reset();
    expect(useSettings.getState().value.download.thread_count).toBe(64);
  });
});

describe("tasks store", () => {
  beforeEach(() => useTasks.setState({ tasks: {}, order: [], report: null }));
  it("creates a task lazily from an early progress event and keeps the label from track()", () => {
    useTasks.getState().onProgress({ taskId: "T", stage: "libraries", fraction: 0.2, doneBytes: 1, totalBytes: 5, doneFiles: 1, totalFiles: 5, speedBps: 10, state: "running" });
    useTasks.getState().track("T", "1.21 Fabric");
    const s = useTasks.getState();
    expect(s.tasks.T).toMatchObject({ label: "1.21 Fabric", fraction: 0.2, status: "running" });
    expect(activeTasks(s)).toHaveLength(1);
  });
  it("reflects pause state from events and finishes", () => {
    useTasks.getState().track("T", "x");
    useTasks.getState().onProgress({ taskId: "T", stage: "assets", fraction: 0.5, doneBytes: 0, totalBytes: 0, doneFiles: 0, totalFiles: 0, speedBps: 0, state: "paused" });
    expect(useTasks.getState().tasks.T.status).toBe("paused");
    useTasks.getState().onFinished({ taskId: "T", kind: "install", label: "x", status: "completed" });
    expect(useTasks.getState().tasks.T).toMatchObject({ status: "completed", fraction: 1 });
    expect(activeTasks(useTasks.getState())).toHaveLength(0);
  });
  it("late progress cannot resurrect a finished task", () => {
    useTasks.getState().track("T", "x");
    useTasks.getState().onFinished({ taskId: "T", kind: "install", label: "x", status: "failed", error: "boom" });
    useTasks.getState().onProgress({ taskId: "T", stage: "assets", fraction: 0.9, doneBytes: 0, totalBytes: 0, doneFiles: 0, totalFiles: 0, speedBps: 0, state: "running" });
    expect(useTasks.getState().tasks.T.status).toBe("failed");
    expect(useTasks.getState().tasks.T.error).toBe("boom");
  });
  it("shows the CurseForge missing-file report only when there are files", () => {
    useTasks.getState().onReport({ taskId: "T", instanceId: "/i", missingFiles: [] });
    expect(useTasks.getState().report).toBeNull();
    useTasks.getState().onReport({ taskId: "T", instanceId: "/i", missingFiles: [{ path: "mods/a.jar", projectId: 1, fileId: 2, pageUrl: "https://www.curseforge.com/x" }] });
    expect(useTasks.getState().report?.missingFiles).toHaveLength(1);
    useTasks.getState().dismissReport();
    expect(useTasks.getState().report).toBeNull();
  });
});

describe("i18n", () => {
  it("resolves keys, interpolates and falls back to the key", () => {
    expect(t("nav.launch")).toBe("启动");
    expect(t("account.added", { name: "Steve" })).toBe("已添加账号 Steve");
    expect(t("nope.key")).toBe("nope.key");
    expect(tRaw<unknown[]>("help.items")?.length).toBeGreaterThan(0);
  });
});
