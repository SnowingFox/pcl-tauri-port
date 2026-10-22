import { describe, expect, it } from "vitest";
import { accountsApi, downloadApi, launchApi, resourcesApi, settingsApi, useMockApi } from "../api";
import { mockConfig } from "../api/mock/bus";

mockConfig.latency = 0;

describe("mock api selection", () => {
  it("is used outside Tauri", () => expect(useMockApi).toBe(true));
});

describe("launch mock", () => {
  it("emits progress -> running and exits on kill", async () => {
    const events: string[] = [];
    const u1 = await launchApi.onProgress((p) => events.push(p.stage));
    const u2 = await launchApi.onExit(() => events.push("exit"));
    const id = await launchApi.start("1.21.4", "acc");
    await new Promise((r) => setTimeout(r, 2800));
    expect(events).toContain("running");
    await launchApi.kill(id);
    expect(events.at(-1)).toBe("exit");
    u1(); u2();
  });
  it("rejects missing account", async () => {
    await expect(launchApi.start("1.21.4", "")).rejects.toThrow("no_account");
  });
  it("toggles mods", async () => {
    await launchApi.toggleMod("x", "sodium-0.6.0.jar", false);
    expect((await launchApi.listMods("x")).find((m) => m.name === "Sodium")?.enabled).toBe(false);
  });
});

describe("accounts mock ms login", () => {
  it("fires success event", async () => {
    let ok = false;
    const un = await accountsApi.onMsLogin((e) => { if (e.state === "success") ok = true; });
    const code = await accountsApi.msLoginStart();
    expect(code.userCode).toBeTruthy();
    await new Promise((r) => setTimeout(r, 3000));
    expect(ok).toBe(true);
    un();
  });
});

describe("download / resources / settings mocks", () => {
  it("lists versions and loader versions", async () => {
    expect((await downloadApi.gameVersions()).length).toBeGreaterThan(0);
    expect(await downloadApi.loaderVersions("vanilla", "1.21.4")).toEqual([]);
    expect((await downloadApi.loaderVersions("fabric", "1.21.4")).length).toBeGreaterThan(0);
  });
  it("install completes", async () => {
    let done = false;
    const un = await downloadApi.onProgress((p) => { if (p.done) done = true; });
    await downloadApi.install({ name: "t", mcVersion: "1.21.4", loader: "vanilla" });
    await new Promise((r) => setTimeout(r, 3000));
    expect(done).toBe(true);
    un();
  });
  it("filters resources", async () => {
    const r = await resourcesApi.search({ kind: "mod", query: "sod", source: "modrinth", offset: 0, limit: 10 });
    expect(r.items.map((i) => i.title)).toEqual(["Sodium"]);
  });
  it("round-trips settings", async () => {
    const s = await settingsApi.get();
    s.download.threads = 7;
    await settingsApi.set(s);
    expect((await settingsApi.get()).download.threads).toBe(7);
  });
});
