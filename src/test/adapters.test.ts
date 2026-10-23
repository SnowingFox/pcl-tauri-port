import { describe, expect, it } from "vitest";
import {
  classifyLogLevel, toAccount, toCrashAnalysis, toDeviceCode, toDownloadFinished, toDownloadProgress, toDownloadTask, toGameVersions,
  toInstance, toLaunchExit, toLaunchLog, toLaunchProgress, toModEntry, toModpackReport, toResourcePage, toSave,
} from "../api/adapters";
import { deepMerge, isHttpUrl, safeImageUrl } from "../api/util";

// Hand-written payloads in the exact shapes the Rust structs serialize (docs/api/*.md + serde derives).

describe("accounts adapters", () => {
  it("maps yggdrasil to authlib and keeps server info", () => {
    const a = toAccount({ id: "a1", kind: "yggdrasil", name: "Alex", uuid: "1111-2222", serverUrl: "https://skin.example/api/yggdrasil", serverName: "Example", identifier: "alex@example.com" });
    expect(a).toEqual({ id: "a1", kind: "authlib", name: "Alex", uuid: "1111-2222", server: "https://skin.example/api/yggdrasil", serverName: "Example" });
  });
  it("maps microsoft and offline", () => {
    expect(toAccount({ id: "m", kind: "microsoft", name: "N", uuid: "u" }).kind).toBe("microsoft");
    expect(toAccount({ id: "o", kind: "offline", name: "N", uuid: "u", serverUrl: null }).server).toBeUndefined();
  });
  it("drops the secret deviceCode and defaults the interval", () => {
    const d = toDeviceCode({ deviceCode: "SECRET", userCode: "ABCD1234", verificationUri: "https://www.microsoft.com/link", expiresIn: 900 });
    expect(d).toEqual({ userCode: "ABCD1234", verificationUri: "https://www.microsoft.com/link", expiresIn: 900, interval: 5, message: undefined });
    expect(JSON.stringify(d)).not.toContain("SECRET");
  });
});

describe("instance adapter", () => {
  const raw = {
    name: "1.20.1-Fabric", path: "/mc/versions/1.20.1-Fabric", root: "/mc", kind: "fabric", mc_version: "1.20.1",
    loaders: [{ kind: "fabric", version: "0.15.0" }, { kind: "opti_fine", version: "HD_U_I5" }], release_time: "2023-06-12T00:00:00+00:00",
    version_type: "release", description: "正式版 1.20.1, Fabric 0.15.0", category: "modded", icon: "fabric", custom_logo_path: null,
    starred: true, hidden: false, modable: true, game_dir: "/mc/versions/1.20.1-Fabric", isolated: true, inherits_from: null, error: null,
  };
  it("uses the instance path as id and picks the primary loader", () => {
    const i = toInstance(raw);
    expect(i.id).toBe("/mc/versions/1.20.1-Fabric");
    expect(i.loader).toBe("fabric");
    expect(i.loaderVersion).toBe("0.15.0");
    expect(i.loaders.map((l) => l.kind)).toEqual(["fabric", "optifine"]);
    expect(i.starred).toBe(true);
  });
  it("prefers neoforge over forge and normalizes neo_forge", () => {
    const i = toInstance({ ...raw, loaders: [{ kind: "neo_forge", version: "21.1.1" }] });
    expect(i.loader).toBe("neoforge");
  });
  it("is vanilla without loaders and flags broken versions", () => {
    expect(toInstance({ ...raw, kind: "release", loaders: [] }).loader).toBe("vanilla");
    const broken = toInstance({ ...raw, kind: "error", mc_version: null, error: "bad json" });
    expect(broken.error).toBe("bad json");
    expect(broken.mcVersion).toBe("");
  });
});

describe("launch adapters", () => {
  it("maps progress and falls back to check for unknown stages", () => {
    expect(toLaunchProgress({ launchId: "L", stage: "java", progress: 0.2, message: "m" })).toEqual({ launchId: "L", stage: "java", progress: 0.2, message: "m" });
    expect(toLaunchProgress({ launchId: "L", stage: "weird", progress: 7, message: "" })).toMatchObject({ stage: "check", progress: 1 });
  });
  it("classifies log lines", () => {
    expect(classifyLogLevel("stdout", "[main/INFO]: hi")).toBe("info");
    expect(classifyLogLevel("stdout", "[main/WARN]: careful")).toBe("warn");
    expect(classifyLogLevel("stdout", "[main/ERROR]: boom")).toBe("error");
    expect(classifyLogLevel("stderr", "java.lang.NullPointerException")).toBe("error");
    expect(classifyLogLevel("stderr", "something odd")).toBe("warn");
    expect(classifyLogLevel("stdout", "[x/DEBUG]: d")).toBe("debug");
  });
  it("maps the log event id to launchId and marks the launcher header as debug", () => {
    const l = toLaunchLog({ id: "L1", stream: "launcher", line: "java -cp ***" }, () => 42);
    expect(l).toEqual({ launchId: "L1", stream: "launcher", level: "debug", line: "java -cp ***", ts: 42 });
  });
  it("maps the exit event with a crash analysis (snake_case from the crash module)", () => {
    const e = toLaunchExit({
      id: "L1", code: 1, crashed: true, durationMs: 4200,
      crash: {
        reasons: [{ id: "out_of_memory", details: [], message: "Minecraft 内存不足。", actions: [{ id: "increase_memory", label: "增加内存" }] }],
        summary: "Minecraft 内存不足。", files_used: ["/mc/logs/latest.log"], primary_file: "/mc/logs/latest.log",
      },
    });
    expect(e.launchId).toBe("L1");
    expect(e.crashed).toBe(true);
    expect(e.crash?.reasons[0].actions[0]).toEqual({ id: "increase_memory", label: "增加内存" });
    expect(e.crash?.primaryFile).toBe("/mc/logs/latest.log");
    expect(e.crash?.filesUsed).toEqual(["/mc/logs/latest.log"]);
  });
  it("accepts camelCase crash fields too", () => {
    expect(toCrashAnalysis({ reasons: [], summary: "", filesUsed: ["a"], primaryFile: "a" })?.primaryFile).toBe("a");
    expect(toCrashAnalysis(null)).toBeNull();
  });
  it("infers a crash from a non-zero code when the backend sends the legacy payload", () => {
    expect(toLaunchExit({ id: "L", code: 1 }).crashed).toBe(true);
    expect(toLaunchExit({ id: "L", code: 0 }).crashed).toBe(false);
    expect(toLaunchExit({ id: "L", code: null }).crashed).toBe(false);
  });
});

describe("mods and saves", () => {
  it("maps a local mod and falls back to the file name", () => {
    const m = toModEntry({ path: "/i/mods/sodium.jar.disabled", file_name: "sodium.jar.disabled", enabled: false, size: 10, sha1: "x", curseforge_fingerprint: 1, meta: { name: null, version: "0.6.0", authors: ["jelly"] } });
    expect(m).toMatchObject({ name: "sodium", enabled: false, version: "0.6.0", authors: ["jelly"], path: "/i/mods/sodium.jar.disabled" });
  });
  it("maps saves and rejects non-png icon data", () => {
    const s = toSave({ folder: "w", name: "World", sizeBytes: 5, lastPlayedMs: 99, iconDataUrl: "data:image/png;base64,AAAA", path: "/s/w" });
    expect(s).toMatchObject({ name: "World", size: 5, lastPlayed: 99, iconUrl: "data:image/png;base64,AAAA" });
    expect(toSave({ folder: "w", name: "", path: "/s/w", iconDataUrl: "javascript:alert(1)" })).toMatchObject({ name: "w", iconUrl: undefined });
  });
});

describe("download adapters", () => {
  it("maps the Mojang manifest", () => {
    const v = toGameVersions({
      latest: { release: "1.21", snapshot: "24w01a" },
      versions: [
        { id: "1.21", type: "release", url: "u", releaseTime: "2024-06-13T10:00:00+00:00" },
        { id: "b1.7.3", type: "old_beta", releaseTime: "2011-07-08T00:00:00+00:00" },
        { id: "x", type: "mystery" },
      ],
    });
    expect(v).toEqual([
      { id: "1.21", type: "release", releaseTime: "2024-06-13" },
      { id: "b1.7.3", type: "old_beta", releaseTime: "2011-07-08" },
      { id: "x", type: "snapshot", releaseTime: "" },
    ]);
  });
  it("maps progress events (snake_case) and clamps the fraction", () => {
    const p = toDownloadProgress({ task_id: "T", stage: "libraries", done_bytes: 5, total_bytes: 10, done_files: 1, total_files: 2, speed_bps: 100, fraction: 1.4, state: "paused" });
    expect(p).toEqual({ taskId: "T", stage: "libraries", fraction: 1, doneBytes: 5, totalBytes: 10, doneFiles: 1, totalFiles: 2, speedBps: 100, state: "paused",
      load: { activeFiles: 0, activeConnections: 0, concurrency: 0, segments: 0, workerThreads: 0, cores: 0, cpuCores: 0 } });
  });
  it("maps finished events and task lists", () => {
    const info = { task_id: "T", kind: "install", label: "1.21", status: "failed", error: "http: boom", last_progress: null };
    expect(toDownloadFinished(info)).toEqual({ taskId: "T", kind: "install", label: "1.21", status: "failed", error: "http: boom" });
    expect(toDownloadTask({ ...info, status: "completed", error: null })).toMatchObject({ fraction: 1, error: undefined });
  });
  it("maps the CurseForge missing-file report", () => {
    const r = toModpackReport({ taskId: "T", instanceId: "/mc/versions/P", missingFiles: [{ path: "mods/a.jar", projectId: 1, fileId: 2, pageUrl: "https://www.curseforge.com/minecraft/mc-mods/a/files/2" }] });
    expect(r.missingFiles).toHaveLength(1);
    expect(toModpackReport({ taskId: "T", instanceId: "i" }).missingFiles).toEqual([]);
  });
});

describe("resource adapters", () => {
  const hit = (platform: string, id: string, icon: string | null) => ({
    platform, id, slug: id, title: `<img src=x onerror=alert(1)>${id}`, description: "<script>alert(1)</script>", icon_url: icon,
    downloads: 5, follows: 1, categories: ["fabric"], author: null, updated: null, page_url: "https://modrinth.com/mod/x",
  });
  it("merges platforms, keeps untrusted text as-is (rendered as text) and filters unsafe icons", () => {
    const page = toResourcePage([
      { kind: "mod", result: { hits: [hit("modrinth", "a", "https://cdn.modrinth.com/a.png")], total: 10 } },
      { kind: "mod", result: { hits: [hit("curseforge", "b", "http://evil.example/a.png")], total: 5 } },
      { kind: "mod", error: "CurseForge：未配置" },
    ]);
    expect(page.total).toBe(15);
    expect(page.items.map((i) => i.source)).toEqual(["modrinth", "curseforge"]);
    expect(page.items[0].iconUrl).toBe("https://cdn.modrinth.com/a.png");
    expect(page.items[1].iconUrl).toBeUndefined();
    expect(page.items[0].summary).toBe("<script>alert(1)</script>");
    expect(page.warnings).toEqual(["CurseForge：未配置"]);
  });
});

describe("util", () => {
  it("deepMerge merges objects and replaces arrays", () => {
    expect(deepMerge({ a: { b: 1, c: [1, 2] }, d: 1 }, { a: { c: [3] } })).toEqual({ a: { b: 1, c: [3] }, d: 1 });
  });
  it("only allows http(s) urls", () => {
    expect(isHttpUrl("https://x.y")).toBe(true);
    expect(isHttpUrl("http://x.y")).toBe(true);
    for (const u of ["javascript:alert(1)", "file:///etc/passwd", "data:text/html,x", "tauri://localhost", "not a url", ""]) expect(isHttpUrl(u)).toBe(false);
  });
  it("only allows https or png data urls as images", () => {
    expect(safeImageUrl("https://a/b.png")).toBeDefined();
    expect(safeImageUrl("http://a/b.png")).toBeUndefined();
    expect(safeImageUrl("data:text/html;base64,AAA")).toBeUndefined();
    expect(safeImageUrl(null)).toBeUndefined();
  });
});

describe("download progress load figures", () => {
  it("maps the engine's live load and tolerates an older backend without it", () => {
    const base = { task_id: "t", stage: "assets", done_bytes: 1, total_bytes: 2, done_files: 3, total_files: 4, speed_bps: 5, fraction: 0.5, state: "running" };
    const full = toDownloadProgress({ ...base, active_files: 24, active_connections: 61, concurrency: 64, segments: 4, worker_threads: 18, cores: 18, cpu_cores: 0.62 });
    expect(full.load).toEqual({ activeFiles: 24, activeConnections: 61, concurrency: 64, segments: 4, workerThreads: 18, cores: 18, cpuCores: 0.62 });
    const old = toDownloadProgress(base);
    expect(old.load).toEqual({ activeFiles: 0, activeConnections: 0, concurrency: 0, segments: 0, workerThreads: 0, cores: 0, cpuCores: 0 });
  });
});
