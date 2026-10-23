/// <reference types="node" />
import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { ACCOUNT_CMD, ACCOUNT_EVT } from "../api/accounts";
import { APP_CMD } from "../api/app";
import { CRASH_CMD } from "../api/crash";
import { DOWNLOAD_CMD, DOWNLOAD_EVT } from "../api/download";
import { INSTANCE_CMD } from "../api/instances";
import { LAUNCH_CMD, LAUNCH_EVT } from "../api/launch";
import { RESOURCE_CMD } from "../api/resources";
import { SETTINGS_CMD } from "../api/settings";

const libRs = readFileSync(new URL("../../src-tauri/src/lib.rs", import.meta.url), "utf-8");

/** Commands registered in `generate_handler![...]` on this branch. */
const registered = new Set(
  [...libRs.matchAll(/^\s*(?:[a-z_]+::)*([a-z_0-9]+),?\s*$/gm)].map((m) => m[1]),
);

/**
 * `app_*` commands owned by the backend integration branch (feat/app-integration, docs/api/app.md).
 * They are not in lib.rs on this branch yet; once merged they will be, and this list stays valid.
 */
const APP_COMMANDS = new Set([
  "app_launch", "app_launch_kill", "app_install_game", "app_install_resource", "app_install_modpack",
  "app_instance_mods", "app_instance_saves", "app_open_url", "app_reveal", "app_data_dirs",
]);

/** Events the backend emits (launch/accounts/download modules + app.md). */
const EMITTED_EVENTS = new Set([
  "launch://progress", "launch://log", "launch://exit", "accounts://device-code",
  "download://progress", "download://finished", "app://modpack-report",
]);

const allCommands: Record<string, string> = {
  ...ACCOUNT_CMD, ...APP_CMD, ...CRASH_CMD, ...DOWNLOAD_CMD, ...INSTANCE_CMD, ...LAUNCH_CMD, ...RESOURCE_CMD, ...SETTINGS_CMD,
};

describe("command names match the backend", () => {
  it("parses a plausible handler list from lib.rs", () => {
    expect(registered.has("accounts_list")).toBe(true);
    expect(registered.size).toBeGreaterThan(50);
  });

  for (const [key, name] of Object.entries(allCommands)) {
    it(`${key} -> ${name} is registered`, () => {
      expect(registered.has(name) || APP_COMMANDS.has(name), `${name} is neither in generate_handler! nor in docs/api/app.md`).toBe(true);
    });
  }

  it("never uses the guessed pre-wiring names", () => {
    // Names the pre-wiring frontend invented. None of them exist in the backend.
    const stale = [
      "accounts_ms_login_start", "accounts_ms_login_cancel", "accounts_authlib_login", "accounts_selected",
      "download_game_versions", "download_loader_versions", "download_install", "launch_list_instances", "launch_list_mods",
      "launch_list_saves", "launch_toggle_mod", "launch_delete_instance", "launch_open_instance_folder",
      "resources_search", "resources_install", "settings_scan_java", "settings_open_url",
    ];
    const used = new Set(Object.values(allCommands));
    for (const s of stale) {
      expect(registered.has(s) || APP_COMMANDS.has(s), `${s} should not exist in the backend`).toBe(false);
      expect(used.has(s), `${s} must not be called`).toBe(false);
    }
  });
});

describe("event names match the backend", () => {
  const events = { ...ACCOUNT_EVT, ...DOWNLOAD_EVT, ...LAUNCH_EVT };
  for (const [key, name] of Object.entries(events)) {
    it(`${key} -> ${name} is emitted`, () => expect(EMITTED_EVENTS.has(name)).toBe(true));
  }
  it("the Rust sources emit the launch/account events we listen to", () => {
    const launch = readFileSync(new URL("../../src-tauri/src/launch/mod.rs", import.meta.url), "utf-8");
    const accounts = readFileSync(new URL("../../src-tauri/src/accounts/commands.rs", import.meta.url), "utf-8");
    const download = readFileSync(new URL("../../src-tauri/src/download/mod.rs", import.meta.url), "utf-8");
    expect(launch).toContain('"launch://log"');
    expect(launch).toContain('"launch://exit"');
    expect(accounts).toContain('"accounts://device-code"');
    expect(download).toContain("download://progress");
    expect(download).toContain("download://finished");
  });
});

describe("window capability", () => {
  it("grants the window permissions the frontend uses", () => {
    const cap = JSON.parse(readFileSync(new URL("../../src-tauri/capabilities/default.json", import.meta.url), "utf-8")) as { permissions: string[] };
    for (const p of ["core:window:allow-hide", "core:window:allow-show", "core:window:allow-minimize", "core:window:allow-unminimize", "core:window:allow-set-focus", "core:window:allow-close", "core:window:allow-start-dragging"]) {
      expect(cap.permissions).toContain(p);
    }
  });
});
