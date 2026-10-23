# App orchestration API (`src-tauri/src/app/`)

Glue layer that turns "launch instance X with account Y" into a running game, and the
install flows into real downloads. JS args are camelCase (Tauri converts to snake_case Rust
params). Return values and event payloads are camelCase unless stated otherwise (the older
`download://*` events keep their snake_case fields, see `download.md`).

## Conventions

### instanceId
`instanceId` is the absolute path of the instance folder (`InstanceInfo.path`, i.e.
`<mcRoot>/versions/<name>`). The backend derives `<mcRoot>` from it and rejects roots that are
not registered in `settings.game.folders` (`FORBIDDEN_PATH`). A bare name (no `/`) is resolved
against the currently selected folder. Anything else relative is `INVALID_ARGUMENT`.

### Errors
Every `app_*` command rejects with an object (not a string):

```ts
type AppError = { code: ErrorCode; message: string /* zh-CN, masked */; details?: any };
```

| code | when | details |
|---|---|---|
| `JAVA_MISSING` | no installed Java satisfies the version | `{ requiredMajor: number, maxMajor: number \| null, found: number[] }` |
| `ACCOUNT_NOT_FOUND` | unknown account id | - |
| `ACCOUNT_EXPIRED` | token refresh failed / needs re-login | - |
| `MS_CLIENT_ID_MISSING` | Microsoft account but no Azure client ID configured (`accounts_set_ms_client_id` / `PCL_MS_CLIENT_ID`) | - |
| `ACCOUNT_ERROR` | other account failure | - |
| `INSTANCE_NOT_FOUND` | folder missing | `{ instanceId }` |
| `INSTANCE_INVALID` | version json broken / unreadable | `{ instanceId? }` |
| `INSTANCE_EXISTS` | install target name already used | `{ name }` |
| `INSTANCE_NOT_MODDABLE` | mod install on an instance without a loader | - |
| `FILES_MISSING` | still-missing files after auto repair | `{ issues: PrecheckIssue[] }` |
| `PRECHECK_FAILED` | other blocking precheck issue (empty name, bad memory) | `{ issues }` |
| `ALREADY_LAUNCHING` | a launch of the same instance is in progress | - |
| `DOWNLOAD_FAILED` | repair/install download failed (hash, status, ...) | - |
| `NETWORK` | API/network failure | - |
| `CURSEFORGE_KEY_MISSING` | `PCL_CURSEFORGE_KEY` not set | - |
| `NO_COMPATIBLE_VERSION` | no file fits the instance's MC version / loader | `{ projectId, mcVersion, loader }` |
| `MANUAL_DOWNLOAD_REQUIRED` | CurseForge author disabled third-party download | `{ projectId, fileId, fileName }` |
| `WORLD_NOT_FOUND` | datapack target world missing | `{ worldName }` |
| `LAUNCH_FAILED` | process could not be spawned | - |
| `INVALID_ARGUMENT` | bad input (unknown loader, datapack without world, non http url, bad id) | - |
| `UNSUPPORTED` | unrecognised modpack format | - |
| `FORBIDDEN_PATH` | path outside known game dirs | - |
| `NOT_FOUND` | path / launch id does not exist | - |
| `CANCELLED` | download task cancelled | - |
| `IO` / `INTERNAL` | filesystem / unexpected | - |

`PrecheckIssue` is as in `launch.md`. Failures **inside a download-manager task** (install,
resource, modpack) arrive in `download://finished` as `TaskInfo.error`; errors raised by the app
layer there use the form `"CODE: message"` (e.g. `"MANUAL_DOWNLOAD_REQUIRED: ..."`), plain engine
errors are free text (`hash mismatch for ...`).

## Commands

### `app_launch(instanceId, accountId, launchId?) -> { launchId: string }`
Resolves **after the game process was spawned** (it awaits repair downloads first), so call it
without blocking the UI and drive the progress bar from `launch://progress`. Pass your own
`launchId` (`[A-Za-z0-9._-]{1,64}`) when you need the id before the promise settles; otherwise one
is generated. Every progress event also carries `instanceId`, so a request can be correlated
before the id is known. Pipeline:

1. `check`: resolve instance + isolation-aware game dir, merge settings (global + per-instance:
   memory, jvm/game args, window mode/size, Java), load the account session (refreshes tokens).
2. `java`: instance Java, else global selected Java, else auto pick; `JAVA_MISSING` otherwise.
   Legacy loader setups (<= 1.16 Forge/LiteLoader/OptiFine, LaunchWrapper) are limited to Java 8.
3. `download`: missing libraries / assets / client jar (also of the `inheritsFrom` parent) are fetched
   through the download engine and awaited; authlib-injector for third-party accounts is
   downloaded to `<data>/tools/authlib-injector.jar` (reused when present, so offline launches work).
   While this runs, `launch://progress` events carry `taskId` (the task also shows up in
   `download://progress` and can be cancelled with `download_cancel`).
4. `natives`: natives extraction, legacy asset materialisation.
5. `start`: spawn.
6. `running`: emitted when the client logs `Setting user` / `Backend library` / `LWJGL Version` /
   OpenAL init, or 15 s after spawn at the latest.

`-javaagent:<jar>=<server>` and `-Dauthlibinjector.yggdrasil.prefetched=...` are put before the
other JVM arguments. Access tokens are masked in every log line, event and the log file.

Launch-core behaviour worth knowing (found by running real clients, see the report):
- The game process does **not** inherit `DYLD_*` variables from the launcher. A long
  `DYLD_FALLBACK_LIBRARY_PATH` (cargo, some IDE/dev shells) makes JNA < 5.13, i.e. Minecraft <= 1.20.x,
  abort in `dlopen` before the window opens.
- Forge/NeoForge 1.17-1.20.x (`cpw.mods.bootstraplauncher`, `-DignoreList=...${version_name}.jar`) need
  the classpath jar to be named after the version. For such versions `versions/<name>/<name>.jar` is
  created as a copy of the inherited vanilla jar (and refreshed when its size changes) and used on the
  classpath. Without it the game dies with "Module minecraft contains package ...".
- The first two lines of `<data>/logs/launch/<launchId>.log` are `[launcher] cwd=...` and the masked
  command line.

### `app_launch_kill(launchId) -> void`
Sends SIGTERM and, if the game is still alive after 5 s (hung client, mod ignoring the signal), SIGKILL.
The later `launch://exit` has `crashed: false` either way. `launch_kill` (the old command) shares the logic.

### `app_install_game(mcVersion, loader?, instanceName?) -> { taskId: string }`
`loader: { kind: "fabric"|"quilt"|"forge"|"neoforge", version: string } | null`.
Installs into the selected game folder (`settings.game.selected_folder`). Forge/NeoForge need a
Java on the machine up front (`JAVA_MISSING` otherwise). `instanceName` defaults to the version id
the installer produces; when given, the version folder is created under that name (json `id`
rewritten, jar renamed; a plain vanilla install keeps its Minecraft version in `clientVersion`).
`INSTANCE_EXISTS` if the name is taken. Progress: `download://progress` with the returned
`taskId` (`fraction` is monotonic 0..1), end: `download://finished`. On success the instance is
selected in settings and `app://instance-installed` `{ name, instanceId, mcVersion }` is emitted;
call `instances_scan` to refresh the list.

### `app_install_resource(platform, projectId, versionId?, kind, instanceId, worldName?) -> { taskId }`
- `platform`: `"modrinth" | "curseforge"`, `kind`: `"mod" | "resourcepack" | "shader" | "datapack"`.
- Without `versionId` the best compatible file is chosen: matches the instance's MC version (and
  the loader for mods; Quilt also accepts Fabric builds), release over beta over alpha, newest first.
- Required dependencies are resolved for mods (up to depth 6 / 60 files). Dependencies that cannot
  be resolved do not fail the install, they are listed in `app://resource-report`
  `{ taskId, installed: PlannedFile[], skippedDependencies: { projectId, reason }[] }`.
- Target dirs under the isolation-aware game dir: `mods/`, `resourcepacks/`, `shaderpacks/`,
  `saves/<worldName>/datapacks/` (`worldName` is required for `datapack`, else `INVALID_ARGUMENT`,
  unknown world -> `WORLD_NOT_FOUND`). Server-supplied file names are validated (no separators).
- SHA-1 is verified by the download engine; a mismatch fails the task and leaves no file.
- Version / compatibility problems are reported **synchronously** (promise rejects); download
  problems arrive in `download://finished`.

### `app_install_modpack(source, instanceName) -> { taskId }`
`source: { type: "file", path } | { type: "project", platform, projectId, versionId? }`.
Parses the pack (Modrinth `.mrpack`, CurseForge zip, MultiMC/Prism zip), installs game + first
loader, extracts overrides, downloads files with SHA-1 verification, marks the instance as
isolated and registers it. For `project` the archive is downloaded first (latest release when no
`versionId`). A failed install removes the half-built instance.

CurseForge files that the author does not allow third parties to download are **not** a failure:
the CDN URL is tried as a fallback, whatever still cannot be fetched (plus files deleted
upstream) is reported:

```ts
type ModpackReport = { instanceId: string; missingFiles: { path: string; projectId: number; fileId: number; pageUrl: string }[]; warnings: string[] };
```
Event `app://modpack-report` `{ taskId, instanceId, missingFiles, warnings }` right before
`download://finished`; also available later via `app_modpack_report(taskId) -> ModpackReport | null`.
CurseForge packs need `PCL_CURSEFORGE_KEY` (otherwise the task fails with `CURSEFORGE_KEY_MISSING`).
Forge/NeoForge packs need a Java up front (`JAVA_MISSING`).

### `app_instance_mods(instanceId) -> LocalMod[]`
Same shape as `local_mods_scan` for `<gameDir>/mods`.

### `app_instance_saves(instanceId) -> SaveEntry[]`
```ts
type SaveEntry = { folder: string; name: string; sizeBytes: number; lastPlayedMs: number | null; iconDataUrl: string | null; path: string };
```
Directories in `<gameDir>/saves` that contain a `level.dat`, newest first. `name` is the level name
from `level.dat` (falls back to the folder name), `lastPlayedMs` the stored last-played time (file
mtime as fallback), `iconDataUrl` is `icon.png` as a data URL when <= 256 KiB. `folder` is the
value to use as `worldName` for datapacks.

### `app_open_url(url) -> void`
http/https only (else `INVALID_ARGUMENT`), opened via tauri-plugin-opener.

### `app_reveal(path) -> void`
Reveals in Finder. `path` must exist and lie inside a registered Minecraft folder or the app data
dir after symlink resolution (`FORBIDDEN_PATH` / `NOT_FOUND` otherwise).

### `app_data_dirs() -> { data: string; logs: string }`
Diagnostics helper for the settings page.

### Removed
`greet` is gone.

## Events

- `launch://progress`: `{ launchId, instanceId, stage: "check"|"java"|"download"|"natives"|"start"|"running", progress: number /*0..1, monotonic*/, message: string, taskId?: string }`
- `launch://log`: `{ id, launchId, stream: "stdout"|"stderr"|"launcher", level: "info"|"warn"|"error"|"debug", line, ts /*ms*/ }`. `id` == `launchId` (kept for older callers). The first event per launch has stream `launcher` and holds the masked command line. Lines are token-masked.
- `launch://exit`: `{ id, launchId, code: number|null, crashed: boolean, crash: CrashAnalysis | null, reason?: string, reportPath?: string | null, durationMs: number }`.
  `crashed` = the process ended with a non-zero exit code (or died from a signal) **and the user did not kill it**. For those exits `crash` is the crash module's analysis (`reasons`, `summary`, `filesUsed`, `primaryFile`; fields camelCase) over the output captured from the process plus the logs / crash reports written in the last 3 minutes (PCL's rule); `reason` is its `summary` and `reportPath` the `primaryFile`, matching the frontend `LaunchExit` type. Clean exits and user kills give `crashed: false, crash: null`.
- `download://progress`, `download://finished`: see `download.md`.
- `app://instance-installed`, `app://resource-report`, `app://modpack-report`: see above.

## Persistence and startup

- All launcher state lives in `~/Library/Application Support/com.snowingfox.pcltauriport/`
  (`settings.json`, `accounts.json`, `logs/`, `tools/`). `PCL_DATA_DIR` overrides it (tests, portable).
  The legacy `~/Library/Application Support/PCL-Tauri/` is **copied** (never moved or deleted, never
  overwriting existing files) on startup, before settings are read.
- Logs: `<data>/logs/pcl.<date>.log` (daily rolling, last 7 kept), `panic.log` for panics (written
  synchronously because release builds abort on panic). Per-launch game output:
  `<data>/logs/launch/<launchId>.log` (last 30 kept). Everything goes through a masking writer;
  access tokens are never logged. `RUST_LOG` overrides the level, `settings.launcher.debug_mode`
  enables debug output.
- The download config is derived from the settings: game dir = selected Minecraft folder, source =
  `download.version_source` (`official` | `mirror_first` -> BMCLAPI first | `official_first`),
  `thread_count` -> concurrency, plus the new `download.segments` / `download.retries`. It is applied
  at startup and on **every** settings change (`settings_set` / `settings_patch` / `settings_reset` /
  `mc_folder_select` / `instance_select`, ...). `download_set_config` now writes through to the
  settings, so it survives a restart. New `source` value for `DownloadConfig`: `"officialfirst"`.
- A task snapshots the config when it starts; install/modpack tasks use the selected folder at that moment.
- Tauri sync commands run on the main thread (no Tokio context); `Manager::spawn` handles that now.
