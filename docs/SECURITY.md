# Security

This document describes the threat model of the Tauri port, what the 0.1.0 hardening pass
fixed, and what is still open. It is a working document: update it when a command is added.

Scope of the review: the Rust commands in `src-tauri/src` (accounts, launch, java, download,
install, settings, instances, crash, resources, modpack), `tauri.conf.json`, the capability file
and the release workflow. It was a code review plus regression tests, not a penetration test, and
it has not been reviewed by a third party.

## Threat model

| Actor | Can do | Trusted? |
|---|---|---|
| The user | Everything the launcher does | Yes |
| The webview (JS) | Call any registered Tauri command with any arguments | **No.** A bug in the UI, a compromised npm dependency, or an XSS through remote metadata (mod descriptions, server names) gives an attacker this position |
| Remote services (Mojang, Modrinth, CurseForge, BMCLAPI, Forge, Fabric) | Return JSON, jars and images | Partly. TLS and published hashes are checked, but their *content* is treated as data, never as a path or a command |
| Third-party Yggdrasil login servers | Return profiles, skins, metadata, redirects | No. The user picks the server, but it can lie |
| Modpack archives (`.zip`, `.mrpack`) | Arbitrary zip entries and manifests | No |
| Other local users and processes | Read files with default permissions | No (see "Remaining") |

Out of scope: a malicious *mod* or *Minecraft server* (mods are code that runs in the game JVM
with the user's rights, like in every launcher), malware that already runs as the user, a
compromised build machine, and physical access.

Main assets: the Microsoft/Yggdrasil tokens (macOS Keychain), the user's files outside the game
folders, the ability to execute programs, and the integrity of the launcher binary.

Design rules that follow from this:

1. A path from JS is confined to a registered Minecraft folder (`settings::guard::confine*`), and
   what may be *written* is narrower than what may be *read* (content folders only).
2. A URL from JS or from remote JSON is `https` (downloads) or `http(s)` (API calls), has no
   credentials, and does not point at loopback, link-local, private or intranet addresses. This is
   enforced on the textual URL, on redirects and on DNS answers (`GuardedResolver`).
3. The launcher executes only a binary that is a real `java` (name, Mach-O/ELF header, executable
   bit, no symlink to something else). Files the launcher downloads or unzips never get the
   executable bit.
4. Tokens do not cross the IPC boundary and are not printed by `Debug`.
5. Remote data never becomes a path unchecked: library paths, maven coordinates, asset hashes, index
   ids, installer entries and API file names are validated as plain relative paths / hex.

## Findings

Severity is the impact if the webview is compromised (or the remote input is hostile), assuming no
other bug. Everything marked Fixed has a regression test; the test name is given where useful.

### Fixed in 0.1.0

| # | Severity | Finding | Fix |
|---|---|---|---|
| 1 | Critical | `java_inspect`, `launch_start`, `install_loader` ran `<path> -version` or spawned `<path>` for any path from JS: arbitrary program execution | `java::is_java_launcher` / `check_java_exe`; `inspect_refuses_non_java_executables`, `webview_requests_are_validated` |
| 2 | Critical | `launch_start` accepted `extra_jvm_args`, so `-javaagent:/tmp/x.jar` or `-XX:OnError=...` ran code in the JVM | `guard::jvm_arg_is_dangerous` for requests, global and per-instance settings; `dangerous_jvm_args` |
| 3 | Critical | `download_start_file` wrote any URL to any `dest`: overwrite `~/.zshrc`, a LaunchAgent, a version JSON, `libraries/` | `dest` must be below a content folder (mods, resourcepacks, shaderpacks, saves, config) of a registered folder; https and public hosts only; `file_requests_are_confined`, `content_dirs_only` |
| 4 | High | `mc_folder_add` / `settings_set` accepted any absolute folder as a "Minecraft folder", which is the trust anchor for every other file command (read, rename, delete, write) | `validate_new_root` refuses `/`, `$HOME`, `~/.ssh`, `~/Library`, system dirs and the launcher data dirs; also applied to `settings_set` and `settings_patch`; `root_candidates`, `settings_changes_are_checked` |
| 5 | High | Zip entries: modpack overrides could write through symlinks, replace `PCL/config.json` (which holds the Java path, JVM args and a pre-launch command) or `<name>.json`, and had no size limit (decompression bomb) | symlink refusal, launcher-owned file refusal, 4 GiB / 50,000-entry limits, bounded manifest reads; `extract_refuses_*` |
| 6 | High | `modpack_extract_overrides(instance_dir)` and `local_mods_scan(mods_dir)`, `local_mod_set_enabled(path)`, `install_optifine_mod(mods_dir)`, `instance_settings_*(instance_dir)`, `java_discover(extra_dirs)`, `download_set_config(game_dir)` took unrestricted paths | all confined to registered folders (instance dirs must be `<root>/versions/<name>`); `local_mod_set_enabled` only renames mod files and never through a symlink |
| 7 | High | Version/library JSON from the network or from an imported instance could escape the game folder: `path: "../../x"`, maven coordinates with `..`, asset hashes and index ids used as paths (`&hash[..2]` also panicked on non-ASCII) | `maven_path`, `Library::files`, `AssetObject::is_valid`, `load_index` reject anything but plain relative paths / hex; installer `filePath` / `data` entries checked; `hostile_index_is_rejected`, `maven_path_rejects_traversal` |
| 8 | High | SSRF: URLs from JS, modpack manifests and version JSON could point at `file:`, `127.0.0.1`, `169.254.169.254` or the LAN; redirects and DNS were unchecked | `check_remote_url`, `redirect_policy`, `GuardedResolver`, applied in `TaskCtx::{file,batch}`, the mrpack parser, `ModpackInstallPlan::validate`, skins and Yggdrasil; `remote_urls`, `redirects_into_internal_hosts_are_refused`, `resolved_addresses_are_filtered` |
| 9 | High | `accounts_get_session` returned the Minecraft access token and Yggdrasil client token to the webview | returns `SessionSummary` (no tokens); the launch path reads tokens in Rust |
| 10 | Medium | Third-party Yggdrasil flow: server-controlled `X-Authlib-Injector-API-Location` redirect to loopback, password over plain `http` to a public host | the redirect target is re-checked, `ensure_secure_for_credentials` requires https unless the host is on the LAN; `api_location_hint_cannot_point_at_loopback` |
| 11 | Medium | The main webview could be navigated to a remote page that would then hold the IPC bridge | `navigation_guard` plugin (only the bundled app, plus the dev server in debug builds), strict CSP, `freezePrototype`, no opener or fs permission; `webview_navigation_is_local_only` |
| 12 | Medium | `instance_*` commands on a symlinked `versions/<name>` could rename, copy or export a folder outside the game folder | `version_dir` uses `symlink_metadata`; `symlinked_versions_are_not_touched` |
| 13 | Medium | A panicking install task stayed "Running" forever; `panic = "abort"` would have killed the app | task bodies run as their own task and a panic becomes `Failed`; release profile keeps unwinding; `a_panicking_task_is_reported_as_failed` |
| 14 | Medium | Token-carrying structs derived `Debug`, so `{:?}` in a log, panic or assertion would print them | manual redacted `Debug` for `AuthSession`, `AuthData`, `MsTokens`, `McLogin`, `AuthResult`; `session_debug_and_summary_hide_tokens` |
| 15 | Medium | `crash_analyze_file` read any `.log` / `.txt` of any size into memory | 64 MiB limit (same as the automatic collector) |
| 16 | Low | `accounts.json` (names, UUIDs, server URLs) was world-readable (default umask) | written with mode 0600; `accounts_file_is_owner_only` |
| 17 | Low | The Azure client id setting accepted any string | must be a GUID |
| 18 | Low | The webview had `core:default` and `opener:default` | capability file lists only the window and event permissions the UI uses (see below) |

### Hardening of the shell (not a finding)

- CSP: `default-src 'none'`, `script-src 'self'`, `connect-src ipc:` only (the frontend does not
  fetch directly), `img-src` limited to `self`, `data:` and the Modrinth/CurseForge image CDNs,
  `object-src`, `frame-src`, `form-action`, `base-uri` all `'none'`. `style-src-attr 'unsafe-inline'`
  is the single relaxation, needed for React `style={{}}` attributes. It does not allow inline
  `<style>` elements or inline scripts. No `unsafe-eval`.
- Devtools are not compiled in: the `tauri` crate is built without the `devtools` feature, so they
  are unavailable in release builds even though the config does not disable them explicitly.
- Hardened runtime on, empty entitlements (`src-tauri/Entitlements.plist` explains why no JIT,
  library-validation or network entitlement is needed).
- The release workflow builds with a read-only token and keeps `contents: write` and no signing
  secrets in two different jobs; third-party actions are pinned by commit SHA.

### Remaining (known, not fixed)

| Severity | Item | Why it is still open / mitigation |
|---|---|---|
| Medium | **TOCTOU between a path check and its use.** The checks resolve symlinks, but a local attacker who can race the filesystem between the check and the `open` could still redirect a write | Needs local code execution as the user already, which is outside the threat model; closing it needs `openat`-style APIs throughout |
| Medium | **Downloads are not pinned to the publisher's host list.** `download_start_file` accepts any public `https` host. Hash verification only happens when the caller passes `sha1` | An allow-list (Modrinth, CurseForge CDN, Mojang, Forge, Fabric, NeoForge, BMCLAPI) would be safer, but modpack and mod platforms add hosts regularly. Revisit once the app layer owns all download calls |
| Medium | **Mojang and Forge installers run code.** `install_loader` runs the Forge/NeoForge installer processors (`java -cp ... Main`) from a jar downloaded over https. The jar is not signature-checked | Same trust as every launcher that installs Forge: the Forge maven. Only a real `java` is executed, and no URL or argument comes from JS |
| Medium | **`pre_launch_command` setting.** It is stored but not executed by this crate yet. When the app layer starts executing it, it runs arbitrary shell code by design | Must only be read from the user's own settings, never from an imported pack. Modpack extraction already refuses to write `PCL/config.json`. Revisit when the feature is wired |
| Low | **DNS rebinding is only filtered at resolve time** for the download and account clients. The resource clients (Modrinth/CurseForge API, fixed hosts) and the `skin_data_url` fetch use the account client; the Microsoft endpoints use fixed `https` hosts | Fixed hosts only |
| Low | **No update mechanism or update signature.** The app does not check for updates | Users reinstall from the release page; `SHA256SUMS` is published, signatures only exist once a Developer ID is configured |
| Low | **Unsigned builds.** Without secrets the release is unsigned and not notarized; Gatekeeper blocks it until the user removes the quarantine flag | Documented in the README; see `docs/RELEASING.md` |
| Low | **Settings file is world-readable.** `settings.json` holds paths and preferences, no secrets | The Azure client id is public by design |
| Low | **`validate_ssl` setting is stored but unused.** There is no "ignore TLS errors" code path | Keep it that way |
| Info | CSP `style-src-attr 'unsafe-inline'` | Needed by React inline styles; CSS injection cannot run script |
| Info | The Rust crates are pinned but there is no `cargo audit` / `npm audit` in CI yet | Dependabot is enabled for cargo, npm and github-actions |

## Reporting

Open a private security advisory on GitHub (Security tab of the repository) rather than a public
issue.

## Adding a command

1. Anything that is a path from JS goes through `settings::guard::confine*` (or
   `confine_instance_dir`, `confine_content` for writes).
2. Anything that is a URL goes through `check_remote_url` / `check_download_url`.
3. Never spawn a program named by JS; if you must run Java, use `java::check_java_exe`.
4. Never return an access token to JS. Use `SessionSummary`.
5. Add a regression test that tries the hostile input.
