# Launch / Java API

Derived from the behaviour of Plain Craft Launcher (see upstream LICENCE).
All commands are in `src-tauri/src/launch/mod.rs` and `src-tauri/src/java/mod.rs`.
Errors (`LaunchError`) are serialized as plain strings. Arguments use camelCase on the JS side (Tauri default) for top-level command args; struct fields are snake_case.

## Types

```ts
type AuthData = { name: string; uuid: string; access_token: string; user_type: string }; // user_type: "msa" | "legacy" | "mojang"
type LaunchRequest = {
  minecraft_dir: string; version_id: string; auth: AuthData;
  java_path?: string | null;       // null = auto select
  game_dir?: string | null;        // null = <mc>/versions/<id>
  max_memory_mb?: number | null; min_memory_mb?: number | null;
  extra_jvm_args?: string[]; extra_game_args?: string[];
  demo?: boolean; resolution?: [number, number] | null;
};
type JavaInfo = { path: string; home: string; version: string; major: number; arch: "arm64"|"x64"|"x86"|"unknown"; vendor: string; is_jdk: boolean };
type PrecheckIssue =
  | { kind: "version_json_missing"; path: string } | { kind: "client_jar_missing"; path: string }
  | { kind: "asset_index_missing"; id: string } | { kind: "libraries_missing"; count: number; first: string[] }
  | { kind: "assets_missing"; count: number }   // warning only
  | { kind: "no_java"; required_major: number }
  | { kind: "java_arch_mismatch"; java_arch: string; system_arch: string } // warning only
  | { kind: "invalid_memory" } | { kind: "empty_player_name" };
type PrecheckReport = { ok: boolean; issues: PrecheckIssue[]; required_java_major: number; java: JavaInfo | null };
type MissingAsset = { name: string; hash: string; size: number; path: string };
```

## Commands

- `java_discover(extraDirs?: string[]) -> JavaInfo[]` scans JAVA_HOME, `/usr/libexec/java_home -V`, `/Library/Java/JavaVirtualMachines`, `~/Library/Java/JavaVirtualMachines`, Homebrew, sdkman, `~/.jdks`, plus Mojang runtime dirs in `extraDirs`. Sorted by major desc. May spawn `java -version` when no `release` file exists.
- `java_inspect(path: string) -> JavaInfo` (rejects with a string). Accepts a java executable or a home.
- `launch_precheck(request) -> PrecheckReport` read-only. `ok` is false if any blocking issue exists.
- `launch_build_command(request) -> string` extracts natives (side effect) and returns the command line with the access token masked.
- `launch_start(request, logDir?: string) -> string` returns an instance id. Spawns the game. If `logDir` is set, output is also written to `<logDir>/<id>.log`.
- `launch_kill(id) -> void` sends SIGTERM (taskkill on Windows).
- `launch_list_running() -> string[]`
- `launch_missing_assets(minecraftDir, versionId) -> MissingAsset[]` for the download module.

## Events

- `launch://log` payload `{ id: string; stream: "stdout"|"stderr"|"launcher"; line: string }`. The first event per launch has stream `launcher` and holds the masked command line.
- `launch://exit` payload `{ id: string; code: number | null }` (null if killed by signal).

## Interface assumptions

- Files must already be downloaded (versions/, libraries/, assets/). This module does not download; it reports what is missing.
- Account module supplies `AuthData`; offline accounts should pass a generated uuid and any non-empty token.
