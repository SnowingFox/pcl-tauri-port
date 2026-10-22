# App orchestration API (`src-tauri/src/app/`)

Glue layer that turns "launch instance X with account Y" into a running game, and the
install flows into real downloads. JS args are camelCase (Tauri converts to snake_case Rust
params). Return values and event payloads are camelCase unless stated otherwise.

## Conventions

### instanceId
`instanceId` is the absolute path of the instance folder (`InstanceInfo.path`, i.e.
`<mcRoot>/versions/<name>`). The backend derives `<mcRoot>` from it and rejects roots that are
not registered in `settings.game.folders` (`FORBIDDEN_PATH`). A bare name (no `/`) is resolved
against the currently selected folder.

### Errors
Every `app_*` command rejects with an object (not a string):

```ts
type AppError = { code: ErrorCode; message: string; details?: any };
```

| code | when | details |
|---|---|---|
| `JAVA_MISSING` | no installed Java satisfies the version | `{ requiredMajor: number, found: number[] }` |
| `ACCOUNT_NOT_FOUND` | unknown account id | - |
| `ACCOUNT_EXPIRED` | token refresh failed / needs re-login | `{ accountId }` |
| `INSTANCE_NOT_FOUND` | folder missing or has no version json | `{ instanceId }` |
| `FILES_MISSING` | still-missing files after auto repair | `{ issues: PrecheckIssue[] }` |
| `PRECHECK_FAILED` | other blocking precheck issue (empty name, bad memory) | `{ issues }` |
| `DOWNLOAD_FAILED` | repair/install download failed | `{ taskId?, reason }` |
| `LAUNCH_FAILED` | spawn failed | - |
| `NETWORK` | API/network failure | - |
| `INVALID_ARGUMENT` | bad input (unknown loader, datapack without world, non http url) | - |
| `FORBIDDEN_PATH` | path outside known game dirs | - |
| `UNSUPPORTED` | e.g. unknown modpack format | - |
| `IO` | filesystem error | - |
| `INTERNAL` | anything else | - |

`PrecheckIssue` is as in `launch.md`.

## Commands

### `app_launch(instanceId, accountId) -> { launchId: string }`
Returns as soon as the work is scheduled; everything else is reported through events.
Errors that are detectable up front (instance/account lookup, Java) reject the promise.
Failures later in the pipeline (download repair) reject too, since the command awaits
everything up to and including process spawn. Pipeline:

1. `check`: resolve instance + isolation-aware game dir, merge settings (global + per-instance:
   memory, Java, jvm/game args, window size), load account session (refreshes tokens).
2. `java`: per-instance Java, else global selected, else auto pick; `JAVA_MISSING` otherwise.
3. `download`: missing libraries / assets / client jar / authlib-injector are downloaded through
   the download engine and awaited.
4. `natives`: natives extraction, legacy asset materialisation.
5. `start`: spawn.
6. `running`: emitted once the first game output line is seen (or 5 s after spawn).

Access tokens are masked in every log and in the `launch://log` header line.

### `app_launch_kill(launchId) -> void`
Same as `launch_kill`.

### `app_install_game(mcVersion, loader?, instanceName?) -> { taskId: string }`
`loader: { kind: "fabric"|"quilt"|"forge"|"neoforge", version: string } | null`.
Installs into the selected game folder (`settings.game.selected_folder`, else the download
config's `gameDir`). `instanceName` defaults to the version id the installer produces. When it
differs, the version folder is renamed (json `id` rewritten, jar renamed) after install. Progress:
`download://progress` with the returned `taskId`, finish: `download://finished` (`TaskInfo`).
On success the instance is selected in settings so the instances list shows it after
`instances_scan`.

### `app_install_resource({ platform, projectId, versionId?, kind, instanceId, worldName? }) -> { taskId }`
(Positional JS args as named object keys; Tauri maps them: `platform`, `projectId`, `versionId`,
`kind`, `instanceId`, `worldName`.)

- `platform`: `"modrinth" | "curseforge"`, `kind`: `"mod" | "resourcepack" | "shader" | "datapack"`.
- If `versionId` is absent the newest release compatible with the instance's MC version (and
  loader for mods) is chosen.
- Required dependencies are resolved (mods only) and installed alongside; already present files
  with a matching SHA-1 are skipped.
- Target dirs under the isolation-aware game dir: `mods/`, `resourcepacks/`, `shaderpacks/`,
  `saves/<worldName>/datapacks/` (`worldName` is required for `datapack`, else `INVALID_ARGUMENT`).
- SHA-1 verified when the platform provides one.
- CurseForge files without a download URL fail the task with a message listing them; the list is
  also available in the finished event's `error`.
Events: `download://progress` / `download://finished` as for installs.

### `app_install_modpack({ source, instanceName }) -> { taskId }`
`source: { type: "file", path: string } | { type: "project", platform, projectId, versionId? }`.
Parses the pack, installs game + loader, downloads files with hash verification, extracts
overrides, registers the instance. Files that CurseForge does not allow third-party download
for are collected in `ModpackReport` and **do not fail the install**:

```ts
type ModpackReport = { instanceId: string; missingFiles: { path: string; projectId: number; fileId: number; pageUrl: string }[] };
```
Delivered via event `app://modpack-report` (`{ taskId, ...ModpackReport }`) right before
`download://finished`.

### `app_instance_mods(instanceId) -> LocalMod[]`
Same shape as `local_mods_scan` for `<gameDir>/mods`.

### `app_instance_saves(instanceId) -> SaveEntry[]`
```ts
type SaveEntry = { folder: string; name: string; sizeBytes: number; lastPlayedMs: number | null; iconDataUrl: string | null; path: string };
```
`name` is the world's level name (falls back to folder). `iconDataUrl` is `icon.png` as a data URL
when <= 256 KiB.

### `app_open_url(url) -> void`
http/https only (else `INVALID_ARGUMENT`), opened via tauri-plugin-opener.

### `app_reveal(path) -> void`
Reveals in Finder. `path` must be inside a registered Minecraft folder, an instance's game dir, or
the app data dir (`FORBIDDEN_PATH` otherwise).

### `app_data_dirs() -> { data: string; logs: string }`
Diagnostics helper for the settings page.

### Removed
`greet` is gone.

## Events

- `launch://progress`: `{ launchId, stage: "check"|"java"|"download"|"natives"|"start"|"running", progress: number /*0..1 overall*/, message: string }`
- `launch://log`: unchanged: `{ id, stream: "stdout"|"stderr"|"launcher", line }` (`id` == `launchId`).
- `launch://exit`: `{ id, code: number|null, crashed: boolean, crash: CrashAnalysis | null, durationMs: number }`.
  `crashed` is true when exit code != 0 (and not killed by the user) or the game died within 3 minutes
  of start with a non-zero code; `crash` is the `crash::CrashAnalysis` (`reasons`, `summary`,
  `filesUsed`, `primaryFile`; same shape as `crash_analyze_instance`) or null when not crashed.
  A user-initiated kill gives `crashed: false`.
- `download://progress`, `download://finished`: see `download.md`.
- `app://modpack-report`: see above.

## Persistence and startup

- All launcher state lives in `~/Library/Application Support/com.snowingfox.pcltauriport/`
  (settings, accounts, caches). Legacy `~/Library/Application Support/PCL-Tauri/` is migrated
  (copied, never deleted) on first start when the new dir has no settings yet.
- Logs: `<data>/logs/pcl.log.<date>` (daily rolling, last 7 kept), panics are logged too.
  Per-launch game logs: `<data>/logs/launch/<launchId>.log`.
- The download config (source, concurrency, game dir) is derived from `settings.download` and
  `settings.game.selected_folder`, applied at startup and on every `settings_set` / `settings_patch` /
  `mc_folder_select` (the commands themselves are unchanged; the app layer subscribes to the store).
