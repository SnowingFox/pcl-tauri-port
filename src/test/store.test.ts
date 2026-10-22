import { beforeEach, describe, expect, it, vi } from "vitest";

// minimal localStorage for node env
const mem = new Map<string, string>();
vi.stubGlobal("localStorage", {
  getItem: (k: string) => mem.get(k) ?? null,
  setItem: (k: string, v: string) => void mem.set(k, v),
  removeItem: (k: string) => void mem.delete(k),
});

import { useUi, DEFAULT_HUE } from "../store/ui";
import { useGame } from "../store/game";
import { useAccounts } from "../store/accounts";
import { t, tRaw } from "../i18n";
import { resetAccountsMock } from "../api/mock/accounts";
import { mockConfig } from "../api/mock/bus";

mockConfig.latency = 0;

describe("ui store", () => {
  beforeEach(() => { mem.clear(); useUi.setState({ toasts: [] }); });
  it("defaults to blue hue", () => {
    expect(DEFAULT_HUE).toBe("blue");
    expect(useUi.getState().hue).toBe("blue");
  });
  it("persists theme choice", () => {
    useUi.getState().setHue("green");
    expect(JSON.parse(mem.get("pcl.ui")!).hue).toBe("green");
  });
  it("toast add/dismiss", () => {
    const id = useUi.getState().toast("hi", "info", 0);
    expect(useUi.getState().toasts).toHaveLength(1);
    useUi.getState().dismissToast(id);
    expect(useUi.getState().toasts).toHaveLength(0);
  });
});

describe("game store", () => {
  beforeEach(() => useGame.setState({ phase: "idle", launchId: null, progress: null, logs: [], lastExit: null }));
  it("tracks progress, logs and exit", () => {
    const g = useGame.getState();
    useGame.setState({ launchId: "L1", phase: "launching" });
    g.onProgress({ launchId: "L1", stage: "checking", progress: 0.3, message: "x" });
    expect(useGame.getState().phase).toBe("launching");
    g.onProgress({ launchId: "L1", stage: "running", progress: 1, message: "x" });
    expect(useGame.getState().phase).toBe("running");
    g.onLog({ launchId: "L1", level: "info", line: "a", ts: 0 });
    g.onLog({ launchId: "other", level: "info", line: "b", ts: 0 });
    expect(useGame.getState().logs).toHaveLength(1);
    g.onExit({ launchId: "L1", code: 1, crashed: true });
    expect(useGame.getState().phase).toBe("idle");
    expect(useGame.getState().lastExit?.crashed).toBe(true);
  });
  it("caps log buffer", () => {
    useGame.setState({ launchId: "L" });
    for (let i = 0; i < 2100; i++) useGame.getState().onLog({ launchId: "L", level: "info", line: String(i), ts: 0 });
    expect(useGame.getState().logs).toHaveLength(2000);
    expect(useGame.getState().logs.at(-1)?.line).toBe("2099");
  });
});

describe("accounts store (mock api)", () => {
  beforeEach(() => { resetAccountsMock(); useAccounts.setState({ accounts: [], selectedId: null }); });
  it("adds offline account and selects it", async () => {
    const a = await useAccounts.getState().addOffline("Steve");
    expect(useAccounts.getState().selectedId).toBe(a.id);
    await useAccounts.getState().remove(a.id);
    expect(useAccounts.getState().accounts).toHaveLength(0);
  });
  it("rejects invalid names", async () => {
    await expect(useAccounts.getState().addOffline("a b")).rejects.toThrow();
  });
});

describe("i18n", () => {
  it("resolves keys and falls back to the key", () => {
    expect(t("nav.launch")).toBe("启动");
    expect(t("nope.key")).toBe("nope.key");
    expect(tRaw<unknown[]>("help.items")?.length).toBeGreaterThan(0);
  });
});
