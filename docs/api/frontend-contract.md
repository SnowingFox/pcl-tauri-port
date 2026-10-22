# Frontend API contract (assumed)

The frontend calls the backend only through `src/api/<domain>.ts`. Each file has a
`*_CMD` / `*_EVT` constant block holding every command and event name, and a `real`
implementation that maps arguments 1:1. Types live in `src/api/types.ts`.
Mock mode: `VITE_MOCK_API=true`, or automatic when not inside Tauri (`VITE_MOCK_API=false` forces real).
Tauri converts camelCase JS argument keys to snake_case Rust parameter names.

## accounts (`src/api/accounts.ts`)
| command | args | returns |
|---|---|---|
| accounts_list | - | Account[] |
| accounts_selected | - | string \| null |
| accounts_add_offline | name | Account |
| accounts_remove | id | void |
| accounts_select | id | void |
| accounts_ms_login_start | - | DeviceCode (device-code flow) |
| accounts_ms_login_cancel | - | void |
| accounts_authlib_login | server, username, password | Account |

Event `accounts://ms-login`: `{state:"pending"} | {state:"success",account} | {state:"error",message}`.

## launch (`src/api/launch.ts`)
| command | args | returns |
|---|---|---|
| launch_list_instances | - | Instance[] |
| launch_start | instanceId, accountId | launchId string |
| launch_kill | launchId | void |
| launch_delete_instance | instanceId | void |
| launch_list_mods | instanceId | ModEntry[] |
| launch_toggle_mod | instanceId, fileName, enabled | void |
| launch_list_saves | instanceId | SaveEntry[] |
| launch_open_instance_folder | instanceId | void |

Events: `launch://progress` (LaunchProgress), `launch://log` (LaunchLog), `launch://exit` (LaunchExit; `crashed` drives the crash dialog).

## download (`src/api/download.ts`)
`download_game_versions` -> GameVersion[]; `download_loader_versions {loader, mcVersion}` -> string[];
`download_install {req: InstallRequest}` -> taskId; `download_cancel {taskId}`.
Event `download://progress` (DownloadProgress, `done`=true ends the task, `error` set on failure).

## resources (`src/api/resources.ts`)
`resources_search {query: ResourceQuery}` -> ResourcePage; `resources_install {source, id, instanceId}` -> void.

## settings (`src/api/settings.ts`)
`settings_get` -> BackendSettings; `settings_set {settings}`; `settings_scan_java` -> JavaInstall[]; `settings_open_url {url}`.
Theme (mode, hue, reduce motion) is frontend-only, stored in localStorage (`pcl.ui`).

## Known gaps to reconcile
- Per-instance settings (Instance > Settings tab) are UI-only; no backend command assumed yet.
- Instance install target for resources uses `instanceId`; real backend may want a path.
- Window close/hide behaviour (`launch.onLaunch`) must be implemented by Rust.
- `tauri.conf.json` uses `titleBarStyle: Overlay` with `data-tauri-drag-region` on the title bar; capability `core:window:allow-start-dragging` may be needed in `capabilities/default.json`.
- openUrl uses `settings_open_url`; could be swapped for `@tauri-apps/plugin-opener`.
