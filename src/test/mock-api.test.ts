import { describe, expect, it } from "vitest";
import { accountsApi, downloadApi, instancesApi, launchApi, resourcesApi, settingsApi, useMockApi } from "../api";
import { mockConfig } from "../api/mock/bus";
import { mockDeviceCode } from "../api/mock/accounts";
import { resetInstancesMock } from "../api/mock/instances";

mockConfig.latency = 0;

describe("mock api selection", () => {
  it("is used outside Tauri", () => expect(useMockApi).toBe(true));
});

describe("launch mock", () => {
  it("emits progress -> running and exits on kill", async () => {
    const events: string[] = [];
    const u1 = await launchApi.onProgress((p) => events.push(p.stage));
    const u2 = await launchApi.onExit(() => events.push("exit"));
    const id = await launchApi.start("/x/versions/1.21.4", "acc");
    await new Promise((r) => setTimeout(r, 2800));
    expect(events).toContain("running");
    await launchApi.kill(id);
    expect(events.at(-1)).toBe("exit");
    u1(); u2();
  });
  it("rejects a missing account with a structured error", async () => {
    await expect(launchApi.start("/x/versions/1.21.4", "")).rejects.toMatchObject({ code: "ACCOUNT_NOT_FOUND" });
  });
});

describe("instances mock", () => {
  it("toggles mods by path and reports the renamed path", async () => {
    resetInstancesMock();
    const [inst] = await instancesApi.scan("/r");
    const mods = await instancesApi.mods(inst);
    const sodium = mods.find((m) => m.name === "Sodium")!;
    const next = await instancesApi.setModEnabled(sodium, false);
    expect(next.endsWith(".jar.disabled")).toBe(true);
    expect((await instancesApi.mods(inst)).find((m) => m.name === "Sodium")?.enabled).toBe(false);
  });
  it("persists per-instance settings", async () => {
    const [inst] = await instancesApi.scan("/r");
    const s = await instancesApi.getSettings(inst);
    s.memory_mb = 3072;
    await instancesApi.setSettings(inst, s);
    expect((await instancesApi.getSettings(inst)).memory_mb).toBe(3072);
  });
});

describe("accounts mock microsoft login", () => {
  it("emits the device code then resolves with the account", async () => {
    let code = "";
    const un = await accountsApi.onDeviceCode((c) => { code = c.userCode; });
    const acc = await accountsApi.microsoftLogin();
    expect(code).toBe(mockDeviceCode.userCode);
    expect(acc.kind).toBe("microsoft");
    un();
  });
  it("cancel rejects the pending login with CANCELLED", async () => {
    const p = accountsApi.microsoftLogin();
    setTimeout(() => void accountsApi.cancelLogin(), 20);
    await expect(p).rejects.toMatchObject({ code: "CANCELLED" });
  });
});

describe("download / resources / settings mocks", () => {
  it("lists versions and loader versions", async () => {
    expect((await downloadApi.gameVersions()).length).toBeGreaterThan(0);
    expect(await downloadApi.loaderVersions("vanilla", "1.21.4")).toEqual([]);
    expect((await downloadApi.loaderVersions("fabric", "1.21.4"))[0].version).toContain("fabric");
  });
  it("install emits progress and finishes", async () => {
    const seen: number[] = [];
    let status = "";
    const u1 = await downloadApi.onProgress((p) => seen.push(p.fraction));
    const u2 = await downloadApi.onFinished((f) => { status = f.status; });
    await downloadApi.installGame({ mcVersion: "1.21.4" });
    await new Promise((r) => setTimeout(r, 3200));
    expect(seen.at(-1)).toBe(1);
    expect(status).toBe("completed");
    u1(); u2();
  });
  it("cancel emits a cancelled finish", async () => {
    let status = "";
    const un = await downloadApi.onFinished((f) => { status = f.status; });
    const id = await downloadApi.installGame({ mcVersion: "1.21.4" });
    await downloadApi.cancel(id);
    expect(status).toBe("cancelled");
    un();
  });
  it("filters resources and merges both platforms", async () => {
    const one = await resourcesApi.search({ kind: "mod", query: "sod", source: "modrinth", offset: 0, limit: 10 });
    expect(one.items.map((i) => i.title)).toEqual(["Sodium"]);
    const both = await resourcesApi.search({ kind: "mod", query: "sod", source: "all", offset: 0, limit: 10 });
    expect(both.items.map((i) => i.source).sort()).toEqual(["curseforge", "modrinth"]);
  });
  it("patches settings recursively", async () => {
    const s = await settingsApi.patch({ download: { thread_count: 7 } });
    expect(s.download.thread_count).toBe(7);
    expect(s.download.source).toBe("official_first");
    expect((await settingsApi.get()).download.thread_count).toBe(7);
  });
});
