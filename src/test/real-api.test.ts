import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
const listeners = new Map<string, (e: { payload: unknown }) => void>();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: async (name: string, cb: (e: { payload: unknown }) => void) => {
    listeners.set(name, cb);
    return () => listeners.delete(name);
  },
}));

import { realAccountsApi } from "../api/accounts";
import { realAppApi } from "../api/app";
import { realCrashApi } from "../api/crash";
import { realDownloadApi } from "../api/download";
import { realInstancesApi } from "../api/instances";
import { realLaunchApi } from "../api/launch";
import { realResourcesApi } from "../api/resources";
import { realSettingsApi } from "../api/settings";
import { AppError } from "../api/errors";

const emit = (name: string, payload: unknown) => listeners.get(name)?.({ payload });

beforeEach(() => { invoke.mockReset(); listeners.clear(); });

describe("accounts (real)", () => {
  it("maps list + selected", async () => {
    invoke.mockResolvedValueOnce([{ id: "a", kind: "yggdrasil", name: "N", uuid: "u", serverUrl: "https://s" }]);
    const list = await realAccountsApi.list();
    expect(invoke).toHaveBeenCalledWith("accounts_list", undefined);
    expect(list[0].kind).toBe("authlib");
    invoke.mockResolvedValueOnce("a");
    expect(await realAccountsApi.selected()).toBe("a");
    expect(invoke).toHaveBeenLastCalledWith("accounts_get_selected", undefined);
  });
  it("microsoft login resolves with the account while the device code arrives by event", async () => {
    const codes: string[] = [];
    await realAccountsApi.onDeviceCode((c) => codes.push(c.userCode));
    let resolve!: (v: unknown) => void;
    invoke.mockReturnValueOnce(new Promise((r) => (resolve = r)));
    const p = realAccountsApi.microsoftLogin();
    emit("accounts://device-code", { deviceCode: "secret", userCode: "ZZZZ", verificationUri: "https://www.microsoft.com/link", expiresIn: 900, interval: 5 });
    expect(codes).toEqual(["ZZZZ"]);
    resolve({ id: "m1", kind: "microsoft", name: "Steve", uuid: "u" });
    expect((await p).kind).toBe("microsoft");
    expect(invoke).toHaveBeenCalledWith("accounts_microsoft_login", undefined);
  });
  it("turns a cancelled login into an AppError(CANCELLED)", async () => {
    invoke.mockRejectedValueOnce("login was cancelled");
    await expect(realAccountsApi.microsoftLogin()).rejects.toMatchObject({ code: "CANCELLED" });
  });
  it("yggdrasil and offline use the documented argument names", async () => {
    invoke.mockResolvedValueOnce([{ id: "y", kind: "yggdrasil", name: "P", uuid: "u" }]);
    const accs = await realAccountsApi.addYggdrasil("https://s", "me@x", "pw");
    expect(invoke).toHaveBeenCalledWith("accounts_add_yggdrasil", { server: "https://s", identifier: "me@x", password: "pw" });
    expect(accs).toHaveLength(1);
    invoke.mockResolvedValueOnce({ id: "o", kind: "offline", name: "Steve", uuid: "u" });
    await realAccountsApi.addOffline("Steve");
    expect(invoke).toHaveBeenLastCalledWith("accounts_add_offline", { name: "Steve" });
    await realAccountsApi.setMsClientId("cid");
    expect(invoke).toHaveBeenLastCalledWith("accounts_set_ms_client_id", { clientId: "cid" });
  });
});

describe("launch (real)", () => {
  it("app_launch returns the launch id and uses instanceId/accountId", async () => {
    invoke.mockResolvedValueOnce({ launchId: "L1" });
    expect(await realLaunchApi.start("/mc/versions/A", "acc")).toBe("L1");
    expect(invoke).toHaveBeenCalledWith("app_launch", { instanceId: "/mc/versions/A", accountId: "acc" });
  });
  it("surfaces structured errors with details", async () => {
    invoke.mockRejectedValueOnce({ code: "JAVA_MISSING", message: "no java", details: { requiredMajor: 21, found: [] } });
    const err = await realLaunchApi.start("/i", "a").catch((e) => e);
    expect(err).toBeInstanceOf(AppError);
    expect(err.code).toBe("JAVA_MISSING");
    expect(err.details).toEqual({ requiredMajor: 21, found: [] });
  });
  it("adapts progress, log and exit events", async () => {
    const seen: unknown[] = [];
    await realLaunchApi.onProgress((p) => seen.push(p));
    await realLaunchApi.onLog((l) => seen.push(l));
    await realLaunchApi.onExit((e) => seen.push(e));
    emit("launch://progress", { launchId: "L", stage: "download", progress: 0.4, message: "补全文件" });
    emit("launch://log", { id: "L", stream: "stderr", line: "[main/ERROR]: x" });
    emit("launch://exit", { id: "L", code: 1, crashed: true, crash: null, durationMs: 100 });
    expect(seen[0]).toMatchObject({ launchId: "L", stage: "download" });
    expect(seen[1]).toMatchObject({ launchId: "L", level: "error", stream: "stderr" });
    expect(seen[2]).toMatchObject({ launchId: "L", crashed: true });
    await realLaunchApi.kill("L");
    expect(invoke).toHaveBeenLastCalledWith("app_launch_kill", { launchId: "L" });
  });
});

describe("instances (real)", () => {
  const ref = { root: "/mc", name: "A" };
  it("uses root/name/newName and the app_* list commands with the path id", async () => {
    invoke.mockResolvedValue({ name: "B", path: "/mc/versions/B", root: "/mc", kind: "release", loaders: [] });
    await realInstancesApi.rename(ref, "B");
    expect(invoke).toHaveBeenLastCalledWith("instance_rename", { root: "/mc", name: "A", newName: "B" });
    await realInstancesApi.duplicate(ref, "C");
    expect(invoke).toHaveBeenLastCalledWith("instance_duplicate", { root: "/mc", name: "A", newName: "C" });
    invoke.mockResolvedValue("~/.Trash/A");
    await realInstancesApi.remove(ref);
    expect(invoke).toHaveBeenLastCalledWith("instance_delete", { root: "/mc", name: "A", permanent: false });
    invoke.mockResolvedValue(undefined);
    await realInstancesApi.openIn(ref, "mods");
    expect(invoke).toHaveBeenLastCalledWith("instance_open_in_finder", { root: "/mc", name: "A", target: "mods" });
    await realInstancesApi.getSettings({ path: "/mc/versions/A" });
    expect(invoke).toHaveBeenLastCalledWith("instance_settings_get", { instanceDir: "/mc/versions/A" });
    invoke.mockResolvedValueOnce([]);
    await realInstancesApi.mods({ id: "/mc/versions/A" });
    expect(invoke).toHaveBeenLastCalledWith("app_instance_mods", { instanceId: "/mc/versions/A" });
  });
  it("scan maps InstanceInfo[]", async () => {
    invoke.mockResolvedValueOnce([{ name: "A", path: "/mc/versions/A", root: "/mc", kind: "release", mc_version: "1.21", loaders: [] }]);
    const [i] = await realInstancesApi.scan("/mc");
    expect(invoke).toHaveBeenCalledWith("instances_scan", { root: "/mc" });
    expect(i).toMatchObject({ id: "/mc/versions/A", mcVersion: "1.21", loader: "vanilla" });
  });
  it("toggling a mod passes the path and returns the renamed path", async () => {
    invoke.mockResolvedValueOnce("/i/mods/a.jar.disabled");
    const p = await realInstancesApi.setModEnabled({ path: "/i/mods/a.jar" } as never, false);
    expect(invoke).toHaveBeenCalledWith("local_mod_set_enabled", { path: "/i/mods/a.jar", enabled: false });
    expect(p).toBe("/i/mods/a.jar.disabled");
  });
});

describe("download (real)", () => {
  it("maps the Mojang manifest and requests loader versions by loader/mcVersion", async () => {
    invoke.mockResolvedValueOnce({ latest: { release: "1.21", snapshot: "x" }, versions: [{ id: "1.21", type: "release", releaseTime: "2024-06-13T00:00:00+00:00" }] });
    expect(await realDownloadApi.gameVersions()).toEqual([{ id: "1.21", type: "release", releaseTime: "2024-06-13" }]);
    expect(invoke).toHaveBeenCalledWith("download_version_manifest", { force: false });
    invoke.mockResolvedValueOnce([{ version: "0.16.0", stable: true }]);
    await realDownloadApi.loaderVersions("fabric", "1.21");
    expect(invoke).toHaveBeenLastCalledWith("install_list_loader_versions", { loader: "fabric", mcVersion: "1.21" });
    invoke.mockClear();
    expect(await realDownloadApi.loaderVersions("vanilla", "1.21")).toEqual([]);
    expect(invoke).not.toHaveBeenCalled();
  });
  it("app_install_game sends loader as an object and null when vanilla", async () => {
    invoke.mockResolvedValue({ taskId: "T" });
    expect(await realDownloadApi.installGame({ mcVersion: "1.21", loader: { kind: "fabric", version: "0.16.0" }, instanceName: "My" })).toBe("T");
    expect(invoke).toHaveBeenLastCalledWith("app_install_game", { mcVersion: "1.21", loader: { kind: "fabric", version: "0.16.0" }, instanceName: "My" });
    await realDownloadApi.installGame({ mcVersion: "1.21" });
    expect(invoke).toHaveBeenLastCalledWith("app_install_game", { mcVersion: "1.21", loader: null, instanceName: null });
  });
  it("pause/resume/cancel use taskId and events are adapted", async () => {
    invoke.mockResolvedValue(undefined);
    await realDownloadApi.pause("T"); expect(invoke).toHaveBeenLastCalledWith("download_pause", { taskId: "T" });
    await realDownloadApi.resume("T"); expect(invoke).toHaveBeenLastCalledWith("download_resume", { taskId: "T" });
    await realDownloadApi.cancel("T"); expect(invoke).toHaveBeenLastCalledWith("download_cancel", { taskId: "T" });
    const got: unknown[] = [];
    await realDownloadApi.onProgress((p) => got.push(p));
    await realDownloadApi.onFinished((f) => got.push(f));
    await realDownloadApi.onModpackReport((r) => got.push(r));
    emit("download://progress", { task_id: "T", stage: "assets", done_bytes: 1, total_bytes: 2, done_files: 1, total_files: 2, speed_bps: 5, fraction: 0.5, state: "running" });
    emit("download://finished", { task_id: "T", kind: "install", label: "x", status: "completed", error: null, last_progress: null });
    emit("app://modpack-report", { taskId: "T", instanceId: "/i", missingFiles: [] });
    expect(got[0]).toMatchObject({ taskId: "T", fraction: 0.5 });
    expect(got[1]).toMatchObject({ taskId: "T", status: "completed" });
    expect(got[2]).toMatchObject({ taskId: "T", missingFiles: [] });
  });
});

describe("resources (real)", () => {
  const hit = (platform: string) => ({ platform, id: "p", slug: "p", title: "T", description: "d", icon_url: null, downloads: 1, follows: 1, categories: [], author: null, updated: null, page_url: null });
  it("searches one platform with the snake_case SearchQuery", async () => {
    invoke.mockResolvedValueOnce({ hits: [hit("modrinth")], total: 1 });
    const page = await realResourcesApi.search({ kind: "mod", query: "sodium", source: "modrinth", mcVersion: "1.21", loader: "fabric", offset: 40, limit: 20 });
    expect(invoke).toHaveBeenCalledWith("resource_search", {
      platform: "modrinth", kind: "mod",
      query: { query: "sodium", mc_version: "1.21", loader: "fabric", categories: [], page: 2, page_size: 20 },
    });
    expect(page.items[0].source).toBe("modrinth");
  });
  it("searches both platforms and degrades to a warning when one fails", async () => {
    invoke.mockImplementation(async (_c: string, a: { platform: string }) => {
      if (a.platform === "curseforge") throw "reqwest: PCL_CURSEFORGE_KEY is not set";
      return { hits: [hit("modrinth")], total: 3 };
    });
    const page = await realResourcesApi.search({ kind: "mod", query: "", source: "all", offset: 0, limit: 20 });
    expect(invoke).toHaveBeenCalledTimes(2);
    expect(page.items).toHaveLength(1);
    expect(page.warnings[0]).toContain("CurseForge");
    expect(page.warnings[0]).toContain("PCL_CURSEFORGE_KEY");
  });
  it("rejects when every platform fails", async () => {
    invoke.mockRejectedValue("network error: down");
    await expect(realResourcesApi.search({ kind: "mod", query: "", source: "all", offset: 0, limit: 20 })).rejects.toMatchObject({ code: "NETWORK" });
  });
  it("vanilla loader is not sent as a filter", async () => {
    invoke.mockResolvedValueOnce({ hits: [], total: 0 });
    await realResourcesApi.search({ kind: "resourcepack", query: "", source: "modrinth", loader: "vanilla", offset: 0, limit: 20 });
    expect((invoke.mock.calls[0][1] as { query: { loader: unknown } }).query.loader).toBeNull();
  });
  it("install resource / modpack use the app_install_* contract", async () => {
    invoke.mockResolvedValue({ taskId: "T" });
    await realResourcesApi.installResource({ platform: "modrinth", projectId: "P", kind: "mod", instanceId: "/mc/versions/A" });
    expect(invoke).toHaveBeenLastCalledWith("app_install_resource", { platform: "modrinth", projectId: "P", versionId: null, kind: "mod", instanceId: "/mc/versions/A" });
    await realResourcesApi.installModpack({ type: "project", platform: "curseforge", projectId: "1" }, "Pack");
    expect(invoke).toHaveBeenLastCalledWith("app_install_modpack", { source: { type: "project", platform: "curseforge", projectId: "1" }, instanceName: "Pack" });
  });
});

describe("settings / app / crash (real)", () => {
  it("settings use get + patch and java_discover", async () => {
    invoke.mockResolvedValueOnce({ schema_version: 1 });
    await realSettingsApi.get();
    expect(invoke).toHaveBeenLastCalledWith("settings_get", undefined);
    invoke.mockResolvedValueOnce({ schema_version: 1 });
    await realSettingsApi.patch({ game: { launcher_visibility: "hide" } });
    expect(invoke).toHaveBeenLastCalledWith("settings_patch", { patch: { game: { launcher_visibility: "hide" } } });
    invoke.mockResolvedValueOnce([{ path: "/j/bin/java", home: "/j", version: "21.0.1", major: 21, arch: "arm64", vendor: "Temurin", is_jdk: true }]);
    const [j] = await realSettingsApi.discoverJava();
    expect(invoke).toHaveBeenLastCalledWith("java_discover", { extraDirs: null });
    expect(j).toMatchObject({ major: 21, isJdk: true });
    invoke.mockResolvedValueOnce({ path: "/j/bin/java", home: "/j", version: "17", major: 17, arch: "x64", vendor: "v", is_jdk: false });
    await realSettingsApi.inspectJava("/j/bin/java");
    expect(invoke).toHaveBeenLastCalledWith("java_inspect", { path: "/j/bin/java" });
  });
  it("openUrl refuses non-http(s) before reaching the backend", async () => {
    await expect(realAppApi.openUrl("file:///etc/passwd")).rejects.toMatchObject({ code: "INVALID_ARGUMENT" });
    await expect(realAppApi.openUrl("javascript:alert(1)")).rejects.toMatchObject({ code: "INVALID_ARGUMENT" });
    expect(invoke).not.toHaveBeenCalled();
    invoke.mockResolvedValueOnce(undefined);
    await realAppApi.openUrl("https://www.minecraft.net/");
    expect(invoke).toHaveBeenCalledWith("app_open_url", { url: "https://www.minecraft.net/" });
  });
  it("crash analysis maps snake_case results", async () => {
    invoke.mockResolvedValueOnce({ reasons: [{ id: "out_of_memory", details: [], message: "m", actions: [] }], summary: "s", files_used: ["f"], primary_file: "f" });
    const a = await realCrashApi.analyzeInstance({ root: "/mc", name: "A" }, ["line"]);
    expect(invoke).toHaveBeenCalledWith("crash_analyze_instance", { root: "/mc", name: "A", liveOutput: ["line"] });
    expect(a.primaryFile).toBe("f");
  });
});
