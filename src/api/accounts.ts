import { call, on, useMockApi } from "./runtime";
import * as mock from "./mock/accounts";
import { toAccount, toDeviceCode } from "./adapters";
import type { RawAccountInfo, RawDeviceCode } from "./raw";
import type { Account, DeviceCode, YggdrasilServerInfo } from "./types";

/** Single source of truth for command/event names of this domain. */
export const ACCOUNT_CMD = {
  list: "accounts_list",
  getSelected: "accounts_get_selected",
  select: "accounts_select",
  remove: "accounts_remove",
  addOffline: "accounts_add_offline",
  microsoftLogin: "accounts_microsoft_login",
  cancelLogin: "accounts_cancel_login",
  serverInfo: "accounts_yggdrasil_server_info",
  addYggdrasil: "accounts_add_yggdrasil",
  getSkin: "accounts_get_skin",
  getMsClientId: "accounts_get_ms_client_id",
  setMsClientId: "accounts_set_ms_client_id",
} as const;
export const ACCOUNT_EVT = { deviceCode: "accounts://device-code" } as const;

export interface AccountsApi {
  list(): Promise<Account[]>;
  selected(): Promise<string | null>;
  addOffline(name: string): Promise<Account>;
  remove(id: string): Promise<void>;
  select(id: string): Promise<void>;
  /**
   * Starts the Microsoft device-code flow. Resolves once the user finished signing in;
   * rejects (CANCELLED, DEVICE_CODE_EXPIRED, DECLINED, NOT_OWNED, ...) otherwise.
   * The code to show arrives through `onDeviceCode` while this promise is pending.
   */
  microsoftLogin(): Promise<Account>;
  cancelLogin(): Promise<void>;
  onDeviceCode(cb: (c: DeviceCode) => void): Promise<() => void>;
  serverInfo(url: string): Promise<YggdrasilServerInfo>;
  /** one account per profile the user owns on that server */
  addYggdrasil(server: string, identifier: string, password: string): Promise<Account[]>;
  /** png data url, or null */
  skin(id: string): Promise<string | null>;
  msClientId(): Promise<string | null>;
  setMsClientId(id: string | null): Promise<void>;
}

/** The real (Tauri) implementation; exported for contract tests. */
export const realAccountsApi: AccountsApi = {
  list: async () => (await call<RawAccountInfo[]>(ACCOUNT_CMD.list)).map(toAccount),
  selected: () => call(ACCOUNT_CMD.getSelected),
  addOffline: async (name) => toAccount(await call<RawAccountInfo>(ACCOUNT_CMD.addOffline, { name })),
  remove: (id) => call(ACCOUNT_CMD.remove, { id }),
  select: (id) => call(ACCOUNT_CMD.select, { id }),
  microsoftLogin: async () => toAccount(await call<RawAccountInfo>(ACCOUNT_CMD.microsoftLogin)),
  cancelLogin: () => call(ACCOUNT_CMD.cancelLogin),
  onDeviceCode: (cb) => on<RawDeviceCode>(ACCOUNT_EVT.deviceCode, (p) => cb(toDeviceCode(p))),
  serverInfo: (url) => call(ACCOUNT_CMD.serverInfo, { url }),
  addYggdrasil: async (server, identifier, password) =>
    (await call<RawAccountInfo[]>(ACCOUNT_CMD.addYggdrasil, { server, identifier, password })).map(toAccount),
  skin: (id) => call(ACCOUNT_CMD.getSkin, { id }),
  msClientId: () => call(ACCOUNT_CMD.getMsClientId),
  setMsClientId: (clientId) => call(ACCOUNT_CMD.setMsClientId, { clientId }),
};

export const accountsApi: AccountsApi = useMockApi ? mock.accountsMock : realAccountsApi;
