# Download and install API

Rust modules: `src-tauri/src/download/`, `src-tauri/src/install/`.
Concepts derived from Plain Craft Launcher (see its LICENCE); implementation is original Rust.

All commands return a `task_id` string when they start work. Errors are returned as strings.

## Config (`DownloadConfig`)
```
{ game_dir: string, source: "official" | "bmcl", concurrency: number, segments: number, retries: number }
```
Default `game_dir`: `~/Library/Application Support/minecraft`. Config is in-memory; the lead/UI layer should persist it and call `download_set_config` at startup. A task snapshots the config when it starts.

With `source: "bmcl"`, known Mojang/Forge/Fabric/Quilt/NeoForge URLs are rewritten to bmclapi2.bangbang93.com and the original URL is kept as fallback.

## Commands
| Command | Args | Returns |
|---|---|---|
| `download_get_config` | none | DownloadConfig |
| `download_set_config` | `config` | void |
| `download_version_manifest` | `force?: bool` | Mojang `version_manifest_v2` (`latest`, `versions[]`); cached 5 min in memory, falls back to on-disk copy offline |
| `download_start_file` | `req: {url, dest, sha1?, size?}` | task_id (generic verified file download) |
| `download_pause` / `download_resume` / `download_cancel` | `task_id` | void |
| `download_task_info` | `task_id` | TaskInfo |
| `download_list_tasks` | none | TaskInfo[] |
| `download_clear_finished` | none | void |
| `install_vanilla` | `mc_version` | task_id |
| `install_loader` | `loader: "fabric"\|"quilt"\|"forge"\|"neoforge"`, `mc_version`, `loader_version`, `java_path?` | task_id |
| `install_list_loader_versions` | `loader`, `mc_version` | `{version, stable?}[]` |
| `install_list_optifine` | `mc_version` | `{mcversion,type,patch,filename}[]` |
| `install_optifine_mod` | `mc_version`, `kind`, `patch`, `mods_dir` | task_id (downloads jar only) |
| `install_list_installed` | none | version ids under `<game_dir>/versions` |

## Events
- `download://progress`: `{task_id, stage, done_bytes, total_bytes, done_files, total_files, speed_bps, fraction (0..1 for the whole task), state: "running"|"paused"|"cancelled"}`. Throttled to about 150 ms.
- `download://finished`: `TaskInfo` `{task_id, kind, label, status: "completed"|"failed"|"cancelled"|..., error?, last_progress?}`.

Stages: `version-json`, `client-jar`, `libraries`, `asset-index`, `assets`, `log-config`, then for loaders `loader-profile`, `installer`, `loader-libraries`, `processors`.

## On-disk output
Standard Mojang layout under `game_dir`: `versions/<id>/<id>.json|.jar`, `libraries/`, `assets/indexes|objects|log_configs`. Loader version json uses `inheritsFrom` (vanilla json and jar are installed alongside). Loader version id is taken from the loader's own json (e.g. `fabric-loader-0.16.0-1.21`, `1.20.1-forge-47.2.0`).

## Resume
Re-running the same install skips every file that already validates (size and SHA-1). Interrupted single-stream downloads resume from `<file>.part` with HTTP Range. Segmented downloads restart that file.

## Status
- Done and unit-tested: hashing, URL rewriting, manifest/rules/library parsing, segment planning, engine (segments, resume, retry, hash failure, cancel, pause) against a local mock server, Forge profile/token parsing.
- Implemented but NOT exercised against the real network or a real JVM: vanilla install, Fabric, Quilt, Forge, NeoForge processors.
- Incomplete: OptiFine (jar download only, no standalone install), old-format Forge only lightly handled, Forge processor DOWNLOAD_MOJMAPS relies on the normal processor path, Java auto-download is not provided (needs `java_path`, `JAVA_HOME`, or system java).
