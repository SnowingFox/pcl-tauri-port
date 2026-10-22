# Resources, local mods and modpacks API

Derived in flow from Plain Craft Launcher (see `LICENCE`); third-party derivative, attribution retained.
All commands return `Result<T, String>` (string error on rejection). Requests send the User-Agent
`SnowingFox/pcl-tauri-port/0.1.0 (+https://github.com/SnowingFox/pcl-tauri-port)`.

## Configuration
CurseForge requires an API key in env var `PCL_CURSEFORGE_KEY` (get one at console.curseforge.com).
It is never embedded in the source. Without it CurseForge commands reject with a descriptive error;
Modrinth needs no key.

## Types (serde, camelCase not applied: fields are snake_case)
- `Platform`: `"modrinth" | "curseforge"`
- `ResourceKind`: `"mod" | "modpack" | "resourcepack" | "shader" | "datapack" | "world"`
- `ModLoader`: `"forge" | "neoforge" | "fabric" | "quilt"`
- `SearchQuery`: `{ query, mc_version?, loader?, categories[], page, page_size }`
- `ProjectSummary`: `{ platform, id, slug, title, description, icon_url?, downloads, follows, categories[], author?, updated?, page_url? }`
- `SearchResult`: `{ hits: ProjectSummary[], total }`
- `ResourceFile`: `{ platform, id, project_id, name, file_name, version_number?, release_type, game_versions[], loaders[], sha1?, size, download_urls[], date_published, dependencies[], fingerprint? }`
- `Dependency`: `{ project_id?, file_id?, kind: "required"|"optional"|"embedded"|"incompatible"|"tool" }`
- `LocalMod`: `{ path, file_name, enabled, size, sha1, curseforge_fingerprint, meta: { mod_id?, name?, version?, description?, authors[], loaders[] } }`
- `ModUpdate`: `{ path, current_sha1, project_id, latest: ResourceFile }`
- `ModpackInstallPlan`: `{ format, name, version?, mc_version, loaders: {loader, version}[], files: PlanFile[], override_dirs[], warnings[] }`
- `PlanFile`: `{ path, urls[], sha1?, size?, optional, curseforge?: {project_id, file_id} }`

## Commands
| Command | Args | Returns |
|---|---|---|
| `resource_search` | `platform, kind, query: SearchQuery` | `SearchResult` |
| `resource_project` | `platform, id` | `ProjectSummary` |
| `resource_versions` | `platform, project_id, mc_version?, loader?` | `ResourceFile[]` |
| `curseforge_download_url` | `mod_id, file_id, file_name` | URL string (API, falling back to the forgecdn path) |
| `local_mods_scan` | `mods_dir` | `LocalMod[]` (reads fabric/quilt/mods.toml/neoforge.mods.toml/mcmod.info; missing dir gives `[]`) |
| `local_mod_set_enabled` | `path, enabled` | new path (renames `.jar` <-> `.jar.disabled`; errors if the target exists) |
| `local_mods_check_updates` | `mods: LocalMod[], mc_version, loader` | `ModUpdate[]` (Modrinth sha1 lookup; newer publish date and different hash) |
| `modpack_parse` | `archive` | `ModpackInstallPlan` (detects `.mrpack`, CurseForge `manifest.json` zip, MultiMC `mmc-pack.json`; rejects path traversal) |
| `modpack_resolve` | `plan` | `[plan, missing CurseForgeRef[]]` (fills CurseForge paths, URLs, hashes; needs key) |
| `modpack_extract_overrides` | `archive, plan, instance_dir` | files copied (zip-slip safe) |

## Install flow
`modpack::installer::install(archive, plan, instance_dir, accepted_optional, &dyn ModpackInstaller)` calls
`install_game` (vanilla + loaders), extracts overrides, then `download_files`. The download agent implements the
`ModpackInstaller` trait (`modpack/mod.rs`); no downloader exists here. It is not exposed as a command yet.

## Verification status
All parsing is tested against fixtures. No live HTTP calls were made.
