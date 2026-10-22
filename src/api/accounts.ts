import { call, on, useMockApi } from "./runtime";
import * as mock from "./mock/accounts";
import type { Account, DeviceCode, MsLoginEvent } from "./types";

/** Single source of truth for command/event names of this domain. */
export const ACCOUNT_CMD = {
  list: "accounts_list",
  addOffline: "accounts_add_offline",
  remove: "accounts_remove",
  select: "accounts_select",
  selected: "accounts_selected",
  msLoginStart: "accounts_ms_login_start",
  msLoginCancel: "accounts_ms_login_cancel",
  authlibLogin: "accounts_authlib_login",
} as const;
export const ACCOUNT_EVT = { msLogin: "accounts://ms-login" } as const;

export interface AccountsApi {
  list(): Promise<Account[]>;
  selected(): Promise<string | null>;
  addOffline(name: string): Promise<Account>;
  remove(id: string): Promise<void>;
  select(id: string): Promise<void>;
  msLoginStart(): Promise<DeviceCode>;
  msLoginCancel(): Promise<void>;
  authlibLogin(server: string, username: string, password: string): Promise<Account>;
  onMsLogin(cb: (e: MsLoginEvent) => void): Promise<() => void>;
}

const real: AccountsApi = {
  list: () => call(ACCOUNT_CMD.list),
  selected: () => call(ACCOUNT_CMD.selected),
  addOffline: (name) => call(ACCOUNT_CMD.addOffline, { name }),
  remove: (id) => call(ACCOUNT_CMD.remove, { id }),
  select: (id) => call(ACCOUNT_CMD.select, { id }),
  msLoginStart: () => call(ACCOUNT_CMD.msLoginStart),
  msLoginCancel: () => call(ACCOUNT_CMD.msLoginCancel),
  authlibLogin: (server, username, password) =>
    call(ACCOUNT_CMD.authlibLogin, { server, username, password }),
  onMsLogin: (cb) => on<MsLoginEvent>(ACCOUNT_EVT.msLogin, cb),
};

export const accountsApi: AccountsApi = useMockApi ? mock.accountsMock : real;
