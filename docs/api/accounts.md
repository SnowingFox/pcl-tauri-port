# Accounts API (`src-tauri/src/accounts/`)

Derived in part from Plain Craft Launcher 2 (see its LICENCE); attribution retained.

All commands return errors as a plain string (the `AccountError` message).
Tokens are stored in the macOS Keychain (service `pcl-tauri-port`); `accounts.json`
in the app data dir holds only names, UUIDs and server info.

## Microsoft client ID (required for Microsoft login)

No client ID is bundled. Register your own Azure app:
1. Azure Portal > Microsoft Entra ID > App registrations > New registration.
2. Supported account types: "Personal Microsoft accounts only" (or any + personal).
3. Authentication > enable "Allow public client flows" (device code needs it).
4. Copy the Application (client) ID. Mojang may require your app to be approved
   for Minecraft services; see https://aka.ms/AppRegInfo.
5. Provide it through, in order: `accounts_set_ms_client_id`, env `PCL_MS_CLIENT_ID`
   (runtime), or build-time `PCL_MS_CLIENT_ID`. The placeholder in code is
   `<REGISTER-YOUR-AZURE-APP-AND-SET-PCL_MS_CLIENT_ID>` and is rejected.

## Types

- `AccountInfo { id, kind: "offline"|"microsoft"|"yggdrasil", name, uuid (dashed), serverUrl?, serverName?, identifier? }`
- `AuthSession { playerName, uuid (undashed), accessToken, userType ("legacy"|"msa"|"mojang"), accountKind, clientToken?, authlibInjector?: { serverUrl, prefetched? } }`
  Rust: `accounts::AuthSession` (serde camelCase). For yggdrasil, launch adds
  `-javaagent:<authlib-injector.jar>=<serverUrl>` and, if present,
  `-Dauthlibinjector.yggdrasil.prefetched=<prefetched>`.

## Commands

| Command | Args | Returns |
|---|---|---|
| `accounts_list` | - | `AccountInfo[]` |
| `accounts_get_selected` | - | `string \| null` |
| `accounts_select` | `id` | void |
| `accounts_remove` | `id` | void (also deletes keychain items) |
| `accounts_add_offline` | `name` (1-16 of `[A-Za-z0-9_]`) | `AccountInfo` |
| `accounts_offline_uuid` | `name` | dashed UUID string |
| `accounts_get_ms_client_id` | - | `string \| null` (effective value) |
| `accounts_set_ms_client_id` | `clientId: string \| null` | void |
| `accounts_microsoft_login` | - | `AccountInfo`; resolves after the user finishes sign-in |
| `accounts_cancel_login` | - | void; aborts a pending login (it then errors "login was cancelled") |
| `accounts_yggdrasil_server_info` | `url` | `{ url, name }` (accepts bare host, https URL, or `authlib-injector:yggdrasil-server:` link) |
| `accounts_add_yggdrasil` | `server, identifier, password` | `AccountInfo[]` (one per profile) |
| `accounts_get_session` | `id` | `AuthSession` (refreshes/validates tokens) |
| `accounts_get_skin` | `id` | PNG `data:` URL or null |
| `accounts_authlib_injector_info` | - | `[downloadUrl, sha256 \| null]`; the download agent fetches the jar |

## Events

- `accounts://device-code`, emitted once by `accounts_microsoft_login`, payload
  `{ deviceCode, userCode, verificationUri, expiresIn, interval, message? }`.
  The UI shows `userCode` and opens `verificationUri`.

## Rust-side use

`accounts::manager::AccountManager::session(id)` returns `AuthSession`; convert
field-by-field into the launch-core mirror struct.
