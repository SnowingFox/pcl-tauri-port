import { beforeEach, describe, expect, it, vi } from "vitest";
import { stubLocalStorage } from "./helpers";

stubLocalStorage();

import { actionOnGameExit, actionOnGameStart, runWindowAction, setWindowDriver, type WindowDriver } from "../lib/windowBehavior";
import { useGame } from "../store/game";
import { useSettings } from "../store/settings";
import type { LauncherVisibility } from "../api";

function fakeDriver() {
  const calls: string[] = [];
  const d: WindowDriver = {
    hide: async () => void calls.push("hide"),
    show: async () => void calls.push("show"),
    minimize: async () => void calls.push("minimize"),
    unminimize: async () => void calls.push("unminimize"),
    setFocus: async () => void calls.push("focus"),
    close: async () => void calls.push("close"),
  };
  return { d, calls };
}

describe("window actions follow PCL LaunchArgumentVisible", () => {
  it("on game start", () => {
    expect(actionOnGameStart("close_immediately")).toBe("close");
    expect(actionOnGameStart("hide_then_close")).toBe("hide");
    expect(actionOnGameStart("hide")).toBe("hide");
    expect(actionOnGameStart("minimize")).toBe("minimize");
    expect(actionOnGameStart("keep")).toBe("none");
  });
  it("on game exit", () => {
    expect(actionOnGameExit("hide_then_close", true)).toBe("close");
    expect(actionOnGameExit("hide_then_close", false)).toBe("show"); // crash: stay open for the crash dialog
    expect(actionOnGameExit("hide", true)).toBe("show");
    expect(actionOnGameExit("hide", false)).toBe("show");
    expect(actionOnGameExit("minimize", false)).toBe("show");
    expect(actionOnGameExit("keep", false)).toBe("none");
    expect(actionOnGameExit("close_immediately", true)).toBe("none");
  });
});

describe("runWindowAction", () => {
  it("drives the window", async () => {
    const { d, calls } = fakeDriver();
    setWindowDriver(d);
    await runWindowAction("hide");
    await runWindowAction("show");
    await runWindowAction("minimize");
    await runWindowAction("close");
    await runWindowAction("none");
    expect(calls).toEqual(["hide", "show", "unminimize", "focus", "minimize", "close"]);
  });
  it("swallows driver failures", async () => {
    setWindowDriver({ ...fakeDriver().d, hide: async () => { throw new Error("denied"); } });
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    await expect(runWindowAction("hide")).resolves.toBeUndefined();
    warn.mockRestore();
  });
  it("is a no-op outside Tauri", async () => {
    setWindowDriver(null);
    await expect(runWindowAction("hide")).resolves.toBeUndefined();
  });
});

describe("game store applies the configured visibility", () => {
  const setVis = (v: LauncherVisibility) => {
    const cur = useSettings.getState().value;
    useSettings.setState({ value: { ...cur, game: { ...cur.game, launcher_visibility: v } } });
  };
  beforeEach(() => {
    useGame.setState({ phase: "launching", launchId: "L", progress: null, logs: [], lastExit: null });
  });

  it("hides once when the game reaches running and re-shows on a clean exit", async () => {
    const { d, calls } = fakeDriver();
    setWindowDriver(d);
    setVis("hide");
    useGame.getState().onProgress({ launchId: "L", stage: "running", progress: 1, message: "" });
    useGame.getState().onProgress({ launchId: "L", stage: "running", progress: 1, message: "" });
    await vi.waitFor(() => expect(calls).toContain("hide"));
    expect(calls.filter((c) => c === "hide")).toHaveLength(1);
    useGame.getState().onExit({ launchId: "L", code: 0, crashed: false, crash: null });
    await vi.waitFor(() => expect(calls).toContain("focus"));
  });
  it("hide_then_close closes after a clean exit but stays open after a crash", async () => {
    const clean = fakeDriver();
    setWindowDriver(clean.d);
    setVis("hide_then_close");
    useGame.getState().onExit({ launchId: "L", code: 0, crashed: false, crash: null });
    await vi.waitFor(() => expect(clean.calls).toEqual(["close"]));

    useGame.setState({ phase: "running", launchId: "L2" });
    const crash = fakeDriver();
    setWindowDriver(crash.d);
    useGame.getState().onExit({ launchId: "L2", code: 1, crashed: true, crash: null });
    await vi.waitFor(() => expect(crash.calls).toContain("show"));
    expect(crash.calls).not.toContain("close");
  });
  it("keep does nothing", async () => {
    const { d, calls } = fakeDriver();
    setWindowDriver(d);
    setVis("keep");
    useGame.getState().onProgress({ launchId: "L", stage: "running", progress: 1, message: "" });
    useGame.getState().onExit({ launchId: "L", code: 0, crashed: false, crash: null });
    await new Promise((r) => setTimeout(r, 10));
    expect(calls).toEqual([]);
  });
});
