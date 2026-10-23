# Settings, instances and crash analysis API

Rust modules: `src-tauri/src/settings`, `src-tauri/src/instances`, `src-tauri/src/crash`.
All commands are called with `invoke("<name>", { camelCaseArgs })`. Errors are rejected with a string.

Derived from Plain Craft Launcher 2 (see `LICENCE`).

## Storage

| What | Location |
| --- | --- |
| Global settings | `~/Library/Application Support/com.snowingfox.pcltauriport/settings.json` (override dir with `PCL_DATA_DIR`; the old `PCL-Tauri` dir is copied over on first start, see `app.md`) |
| Per-instance settings | `<root>/versions/<name>/PCL/config.json` |

Writes are atomic (temp file, fsync, rename). A corrupt `settings.json` is renamed to
`settings.json.corrupt` and defaults are used. Unknown keys are ignored, missing keys get defaults.
`schema_version` is currently `1`; the v0 flat prototype layout (`java_path`, `memory_mb`, `mc_folders`,
`download_source`) is migrated on load.

## Global settings schema (v1)

```jsonc
{
  "schema_version": 1,
  "game": {
    "folders": [{ "name": "默认文件夹", "path": "/Users/me/Library/Application Support/minecraft" }],
    "selected_folder": "<one of folders[].path>",
    "selected_instance": null,
    "isolation": "all",            // off | moddable_only | non_release_only | moddable_and_non_release | all
    "window_mode": "default",      // fullscreen | default | custom | same_as_launcher
    "window_width": 854, "window_height": 480,
    "jvm_args": "...", "game_args": "",
    "pre_launch_command": "", "wait_for_pre_launch": true,
    "launcher_visibility": "keep", // close_immediately | hide_then_close | hide | minimize | keep
    "process_priority": 1,         // 0 high, 1 normal, 2 low
    "custom_title": "", "custom_info": "PCL"
  },
  "memory": { "mode": "auto", "custom_mb": 4096, "optimize_before_launch": false },   // mode: auto | custom
  "java": { "selected": null, "custom_paths": [], "prefer_64bit": true },
  "download": {
    "source": "official_first", "version_source": "official_first", "mod_source": "official_first", // official | mirror_first | official_first (default: official_first; schema v2)
    "thread_count": 64, "speed_limit_bps": 0, "validate_ssl": true, "translate_mod_names": true
  },
  "ui": {
    "language": "zh-CN", "theme_mode": "system",   // light | dark | system
    "theme_id": "pcl", "hue": 180, "saturation": 80, "lightness": 20,
    "opacity": 1000, "background_blur": 0, "show_logo": true
  },
  "window": {
    "width": 1000, "height": 618, "x": null, "y": null, "maximized": false,
    "remember_geometry": true, "button_style": "pcl"  // pcl | mac_os
  },
  "launcher": { "debug_mode": false, "check_update_release": false, "check_update_snapshot": false, "telemetry": false }
}
```

Values are clamped on every write (memory >= 512 MB, threads 1..256, opacity 0..1000, etc.). On first
run the macOS default folder is added; `selected_folder` always points at a registered folder.

Per-instance settings (`InstanceSettings`): `isolated` (null = auto), `memory_mb` (null = global),
`java_path` (null = global), `jvm_args`, `game_args`, `pre_launch_command`, `wait_for_pre_launch`,
`server_address`, `custom_title`, `custom_info`, `starred`, `display_type` (0 auto, 1 hidden, 2 modded,
3 regular, 4 rubbish, 5 fool, 6 error), `logo`, `logo_custom`, `disable_mod_update_check`.

## Settings commands

| Command | Args | Returns | Notes |
| --- | --- | --- | --- |
| `settings_get` | none | `GlobalSettings` | |
| `settings_set` | `settings` | `GlobalSettings` | Replaces everything; sanitized and persisted. |
| `settings_patch` | `patch` (partial JSON) | `GlobalSettings` | Recursive merge. A patch with wrong types is rejected and nothing changes. |
| `settings_reset` | none | `GlobalSettings` | Back to defaults. |
| `settings_path` | none | string | Path of `settings.json`. |
| `instance_settings_get` | `instanceDir` | `InstanceSettings` | Defaults if the file is missing. |
| `instance_settings_set` | `instanceDir` (`<root>/versions/<name>`), `settings` | void | Fails if the folder does not exist. |

## Instance commands

`root` must be one of the registered folders (`game.folders[].path`); other paths are rejected.
Instance names cannot contain path separators or `: * ? " < > |`, cannot be `.`/`..`.

| Command | Args | Returns | Notes |
| --- | --- | --- | --- |
| `mc_folders_list` | none | `[{name, path}]` | |
| `mc_folder_add` | `name`, `path` | folder list | Absolute path; creates `versions/`. Re-adding renames. |
| `mc_folder_remove` | `path` | folder list | Unregisters only, never deletes files. |
| `mc_folder_select` | `path` | void | |
| `instances_scan` | `root` | `InstanceInfo[]` | Sorted by name; broken versions come back with `kind: "error"` and `error`. |
| `instance_get` | `root`, `name` | `InstanceInfo` | |
| `instance_select` | `root`, `name` | void | Stores the selection in settings. |
| `instance_rename` | `root`, `name`, `newName` | `InstanceInfo` | Moves folder, jar, natives dir, rewrites JSON `id`. |
| `instance_duplicate` | `root`, `name`, `newName` | `InstanceInfo` | Full copy, symlinks skipped, cleaned up on failure. Copy is un-starred. |
| `instance_delete` | `root`, `name`, `permanent` | string | Moves to `~/.Trash` unless `permanent`; returns the new path. Refuses symlinked version dirs. The UI must confirm first. |
| `instance_open_in_finder` | `root`, `name`, `target` | void | target: `instance` (reveal), `game`, `mods`, `saves`, `logs`. |
| `instance_export_info` | `root`, `name` | `ExportedInfo` | Instance info, settings, mod/resource/shader/save lists, size. Server address is stripped. |

`InstanceInfo`: `name, path, root, kind, mc_version, loaders[{kind, version}], release_time, version_type,
description, category, icon, custom_logo_path, starred, hidden, modable, game_dir, isolated, inherits_from, error`.

- `kind`: `error | release | snapshot | fool | old | forge | neoforge | fabric | quilt | optifine | lite_loader`
- `category`: `star | hidden | modded | regular | rubbish | fool | error`
- `icon`: `grass | command_block | cobblestone | anvil | neoforge | fabric | grass_path | egg | gold_block | redstone_block | custom`
- `game_dir` is the version folder when isolated, otherwise the root.

## Crash commands

| Command | Args | Returns |
| --- | --- | --- |
| `crash_analyze_instance` | `root`, `name`, `liveOutput?: string[]` | `CrashAnalysis` |
| `crash_analyze_file` | `path` (`.log`/`.txt`) | `CrashAnalysis` |
| `crash_analyze_text` | `text` | `CrashAnalysis` |

`crash_analyze_instance` only reads logs written in the last 3 minutes (PCL behaviour) from
`crash-reports/`, `hs_err*`, `logs/latest.log`, `logs/debug.log` and `*.log` in the game dir/root, plus the
captured process output.

```ts
type CrashAnalysis = {
  reasons: { id: string; details: string[]; message: string; actions: { id: string; label: string }[] }[];
  summary: string;          // all messages joined, with PCL's trailing hints
  files_used: string[];
  primary_file: string | null; // file to offer as "view output"
};
```

Reason ids: `jvm_args_invalid, mods_extracted, mixin_bootstrap_missing, out_of_memory, using_jdk, graphics_no_opengl,
using_openj9, java_too_new, java_incompatible, mod_name_special_chars, graphics_pixel_format, tiny_output,
player_triggered_crash, opengl_1282, signature_mismatch, mod_certain, mod_suspected, mod_config_broken,
mod_mixin_failed, mod_loader_error, mod_init_failed, stack_keywords, stack_mod_names, optifine_world_load,
block_crash, entity_crash, texture_too_large, no_logs, mod_duplicate, mod_incompatible,
optifine_forge_incompatible, fabric_solution, forge_error, old_forge_new_java, multiple_forge_in_json,
too_many_mods, night_config_bug, shaders_mod_with_optifine, forge_incomplete, mod_needs_java11,
mod_missing_dependency`, plus macOS additions `java_missing, mac_missing_start_on_first_thread, mac_arch_mismatch`.

Action ids the frontend should handle: `edit_jvm_args, open_mods_folder, increase_memory, remove_heavy_content,
change_java, download_java, install_dependency, disable_mods, reinstall_loader, change_optifine, update_system,
remove_resourcepack, remove_mods, redownload_game, reinstall_natives`.

## Not covered

- Zip import for crash reports and the "export report" zip (PCL `Import` / `Output`).
- Telemetry upload and username/token filtering of exported files.
- `FabricError` exists as a reason but no rule emits it (same as PCL).
