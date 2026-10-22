import { beforeEach, describe, expect, it, vi } from "vitest";
import { stubLocalStorage } from "./helpers";

const mem = stubLocalStorage();

import { BUY_COUNTS, BUY_URL, SPONSOR_COUNTS, SPONSOR_URL, decideLaunchPrompts, needsPrompt } from "../lib/launchPrompts";
import { incrementLaunchCount, markBought, readCounters } from "../lib/counters";
import { usePrompt } from "../store/prompt";
import { useGame } from "../store/game";
import { useAccounts } from "../store/accounts";
import type { Account } from "../api";
import { mockConfig } from "../api/mock/bus";
import { resetAccountsMock } from "../api/mock/accounts";

mockConfig.latency = 0;

// PCL's McLaunchPrecheck schedule, copied from ModLaunch.vb. Ours must be a superset (never sparser).
const PCL_BUY = [3, 8, 15, 30, 50, 70, 90, 110, 130, 180, 220, 280, 330, 380, 450, 550, 660, 750, 880, 950, 1100, 1300, 1500, 1700, 1900];
const PCL_SPONSOR = [10, 20, 40, 60, 80, 100, 120, 150, 200, 250, 300, 350, 400, 500, 600, 700, 800, 900, 1000, 1200, 1400, 1600, 1800, 2000];

describe("prompt schedule (PCL LICENCE heavy use rule 4)", () => {
  it("is at least as dense as PCL McLaunchPrecheck", () => {
    for (const n of PCL_BUY) expect(BUY_COUNTS, `buy@${n}`).toContain(n);
    for (const n of PCL_SPONSOR) expect(SPONSOR_COUNTS, `sponsor@${n}`).toContain(n);
  });
  it("also prompts on the very first offline launch", () => {
    expect(decideLaunchPrompts({ launchCount: 0, accountKind: "offline", boughtHint: false }).buy).toBe(true);
  });
  it("links to the official store and the author sponsor page", () => {
    expect(BUY_URL).toBe("https://www.minecraft.net/");
    expect(SPONSOR_URL).toBe("https://meloong.com/afd/a/LTCat");
  });
});

describe("decideLaunchPrompts gating", () => {
  const d = (launchCount: number, accountKind: Account["kind"], boughtHint = false) => decideLaunchPrompts({ launchCount, accountKind, boughtHint });
  it("shows the purchase prompt for offline and third-party accounts on scheduled launches", () => {
    for (const n of PCL_BUY) {
      expect(d(n, "offline").buy, `offline@${n}`).toBe(true);
      expect(d(n, "authlib").buy, `authlib@${n}`).toBe(true);
    }
  });
  it("does not show the purchase prompt to Microsoft accounts or after a Microsoft login was seen", () => {
    expect(d(3, "microsoft").buy).toBe(false);
    expect(d(3, "offline", true).buy).toBe(false);
  });
  it("shows the sponsorship prompt to every account kind on scheduled launches", () => {
    for (const kind of ["offline", "microsoft", "authlib"] as const) expect(d(10, kind).sponsor).toBe(true);
  });
  it("stays quiet on unscheduled launches", () => {
    expect(needsPrompt(d(1, "offline"))).toBe(false);
    expect(needsPrompt(d(2, "offline"))).toBe(false);
    expect(needsPrompt(d(4, "offline"))).toBe(false);
    expect(needsPrompt(d(11, "microsoft"))).toBe(false);
  });
  it("can show both prompts in one launch", () => {
    const both = BUY_COUNTS.find((n) => SPONSOR_COUNTS.includes(n));
    if (both !== undefined) expect(needsPrompt(d(both, "offline"))).toBe(true);
  });
});

describe("counters", () => {
  beforeEach(() => mem.clear());
  it("persist and increment", () => {
    expect(readCounters()).toEqual({ count: 0, bought: false });
    expect(incrementLaunchCount()).toBe(1);
    expect(incrementLaunchCount()).toBe(2);
    markBought();
    expect(readCounters()).toEqual({ count: 2, bought: true });
  });
  it("ignore corrupted storage (prompts keep firing from zero)", () => {
    mem.set("pcl.launch", "{not json");
    expect(readCounters()).toEqual({ count: 0, bought: false });
    mem.set("pcl.launch", JSON.stringify({ count: -5, bought: "yes" }));
    expect(readCounters()).toEqual({ count: 0, bought: false });
  });
});

describe("launch gating through the prompt store", () => {
  const offline: Account = { id: "o1", kind: "offline", name: "Steve", uuid: "u" };
  const ms: Account = { id: "m1", kind: "microsoft", name: "Alex", uuid: "u2" };
  const startSpy = vi.fn();

  beforeEach(() => {
    mem.clear();
    resetAccountsMock();
    startSpy.mockReset();
    startSpy.mockResolvedValue(undefined);
    usePrompt.setState({ pending: null });
    useGame.setState({ phase: "idle", start: startSpy as never });
    useAccounts.setState({ accounts: [], selectedId: null });
  });

  it("blocks the first offline launch behind the purchase prompt until confirmed", async () => {
    await usePrompt.getState().requestLaunch(offline);
    expect(startSpy).not.toHaveBeenCalled();
    expect(usePrompt.getState().pending?.decision.buy).toBe(true);
    await usePrompt.getState().confirm();
    expect(startSpy).toHaveBeenCalledWith("o1");
    expect(usePrompt.getState().pending).toBeNull();
  });
  it("cancelling the prompt never launches", async () => {
    await usePrompt.getState().requestLaunch(offline);
    usePrompt.getState().cancel();
    expect(startSpy).not.toHaveBeenCalled();
    expect(usePrompt.getState().pending).toBeNull();
  });
  it("launches straight away on an unscheduled count", async () => {
    mem.set("pcl.launch", JSON.stringify({ count: 1, bought: false }));
    await usePrompt.getState().requestLaunch(offline);
    expect(startSpy).toHaveBeenCalledWith("o1");
    expect(usePrompt.getState().pending).toBeNull();
  });
  it("Microsoft accounts only see the sponsorship prompt", async () => {
    mem.set("pcl.launch", JSON.stringify({ count: 3, bought: false }));
    await usePrompt.getState().requestLaunch(ms);
    expect(startSpy).toHaveBeenCalledTimes(1);
    startSpy.mockClear();
    mem.set("pcl.launch", JSON.stringify({ count: 10, bought: false }));
    await usePrompt.getState().requestLaunch(ms);
    expect(startSpy).not.toHaveBeenCalled();
    expect(usePrompt.getState().pending?.decision).toEqual({ buy: false, sponsor: true });
  });
  it("adopting a Microsoft account retires the purchase prompt", async () => {
    await useAccounts.getState().adopt(ms).catch(() => {});
    expect(readCounters().bought).toBe(true);
    mem.set("pcl.launch", JSON.stringify({ count: 3, bought: true }));
    await usePrompt.getState().requestLaunch(offline);
    expect(startSpy).toHaveBeenCalledWith("o1");
  });
  it("ignores a second request while a prompt is open or a game is running", async () => {
    await usePrompt.getState().requestLaunch(offline);
    await usePrompt.getState().requestLaunch(offline);
    expect(usePrompt.getState().pending).not.toBeNull();
    usePrompt.setState({ pending: null });
    useGame.setState({ phase: "running" });
    await usePrompt.getState().requestLaunch(offline);
    expect(usePrompt.getState().pending).toBeNull();
    expect(startSpy).not.toHaveBeenCalled();
  });
});
