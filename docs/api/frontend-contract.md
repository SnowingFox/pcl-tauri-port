# Frontend API wiring

The frontend calls the backend only through `src/api/<domain>.ts`. Each file holds a `*_CMD` / `*_EVT`
constant block with every command and event name of that domain, a `real` implementation (exported as
`real*Api` for tests) and a mock in `src/api/mock/` with the same interface.

- Mock mode: `VITE_MOCK_API=true`, or automatic when not inside Tauri (`VITE_MOCK_API=false` forces real).
- `src/api/raw.ts` holds the payload shapes exactly as Rust serializes them; `src/api/adapters.ts` maps them to the UI
  types in `src/api/types.ts`. Adapters are pure and unit-tested with hand-written payloads.
- `src/api/runtime.ts#call` wraps `invoke` and always rejects with an `AppError { code, message, details }`.
  Structured `app_*` errors keep their code; legacy string errors are mapped by pattern (`src/api/errors.ts`).
  `describeError` turns a code into Chinese text plus an optional action (for example `JAVA_MISSING` -> "前往 Java 设置").
- `src/test/contract.test.ts` fails if a command name is neither in `generate_handler!` (`src-tauri/src/lib.rs`)
  nor in the `app_*` list of `docs/api/app.md`.

Arguments are camelCase on the JS side (Tauri maps them to snake_case Rust parameters). Struct fields inside
`query`, `settings` and `patch` are snake_case.

## Instance identity

`Instance.id` is the absolute folder path `<root>/versions/<name>` (what the backend calls `instanceId`).
Rename, duplicate, delete and open-in-Finder need `root` and `name` and take them from the same record.

## Commands used

| domain | command | notes |
|---|---|---|
| accounts | `accounts_list`, `accounts_get_selected`, `accounts_select`, `accounts_remove`, `accounts_add_offline {name}` | |
| accounts | `accounts_microsoft_login` | pending until sign-in finishes; code arrives via `accounts://device-code` |
| accounts | `accounts_cancel_login`, `accounts_yggdrasil_server_info {url}`, `accounts_add_yggdrasil {server, identifier, password}` | |
| accounts | `accounts_get_ms_client_id`, `accounts_set_ms_client_id {clientId}` | shown in the login dialog on `MS_CLIENT_ID_MISSING` |
| launch | `app_launch {instanceId, accountId}` -> `{launchId}`, `app_launch_kill {launchId}` | |
| instances | `instances_scan {root}`, `instance_select`, `instance_rename`, `instance_duplicate`, `instance_delete {permanent}`, `instance_open_in_finder {target}` | |
| instances | `instance_settings_get/set {instanceDir}` | backs the instance Settings tab |
| instances | `app_instance_mods {instanceId}`, `local_mod_set_enabled {path, enabled}`, `app_instance_saves {instanceId}` | |
| folders | `mc_folders_list`, `mc_folder_add`, `mc_folder_remove`, `mc_folder_select` | folder select is used by Settings > Launch |
| download | `download_version_manifest {force}`, `install_list_loader_versions {loader, mcVersion}` | |
| download | `app_install_game {mcVersion, loader, instanceName}` -> `{taskId}` | `loader: {kind, version} \| null` |
| download | `download_pause/resume/cancel {taskId}`, `download_list_tasks`, `download_clear_finished` | |
| resources | `resource_search {platform, kind, query}` | both platforms are queried in parallel; one failing platform becomes a warning |
| resources | `app_install_resource`, `app_install_modpack` | |
| settings | `settings_get`, `settings_patch {patch}`, `settings_reset`, `settings_path` | UI uses patch only |
| java | `java_discover {extraDirs}`, `java_inspect {path}` | |
| crash | `crash_analyze_instance`, `crash_analyze_file`, `crash_analyze_text` | |
| app | `app_open_url {url}`, `app_reveal {path}`, `app_data_dirs` | `openUrl` refuses non-http(s) before invoking |

## Events

| event | payload | consumer |
|---|---|---|
| `accounts://device-code` | `{deviceCode, userCode, verificationUri, expiresIn, interval, message?}` | login dialog (the secret `deviceCode` is dropped by the adapter) |
| `launch://progress` | `{launchId, stage, progress, message}` | game store |
| `launch://log` | `{id, stream, line}` | game store (batched, 2000 line cap) |
| `launch://exit` | `{id, code, crashed, crash, durationMs}` | game store, crash dialog, window behaviour |
| `download://progress` | `{task_id, stage, fraction, ...}` | tasks store |
| `download://finished` | `TaskInfo` | tasks store (toast, instance reload) |
| `app://modpack-report` | `{taskId, instanceId, missingFiles[]}` | CurseForge manual-download dialog |

## Frontend-only state

- Theme (mode, hue, reduce motion): `localStorage` key `pcl.ui`.
- Launch counters for the purchase and sponsorship prompts: `localStorage` key `pcl.launch`
  (`src/lib/counters.ts`, schedule in `src/lib/launchPrompts.ts`).
- Window behaviour after launch (`settings.game.launcher_visibility`) is applied in `src/lib/windowBehavior.ts` with
  `@tauri-apps/api/window`; permissions are in `src-tauri/capabilities/default.json`.

## Security notes

- Remote text (Modrinth/CurseForge titles and descriptions, crash messages, log lines) is rendered as React text only.
  `src/test/security.test.ts` fails the build on `dangerouslySetInnerHTML`, `innerHTML`, `window.open` or direct use of the
  opener plugin from UI code.
- Remote icons are only accepted when `https:` (or a PNG `data:` URL for saves).
- External links go through `appApi.openUrl` (http/https only).
