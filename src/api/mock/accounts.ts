import type { AccountsApi } from "../accounts";
import type { Account, DeviceCode } from "../types";
import { AppError } from "../errors";
import { delay, mockEmit, mockOn, uid } from "./bus";

const accounts: Account[] = [];
let selected: string | null = null;
let cancel: (() => void) | null = null;
let clientId: string | null = null;

function fakeUuid(name: string): string {
  let h = 0;
  for (const c of name) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  const hex = h.toString(16).padStart(8, "0");
  return `${hex}-0000-4000-8000-${hex}${hex.slice(0, 4)}`;
}

export function resetAccountsMock(): void {
  accounts.length = 0;
  selected = null;
  cancel = null;
  clientId = null;
}

export const mockDeviceCode: DeviceCode = {
  userCode: "A1B2C3D4",
  verificationUri: "https://www.microsoft.com/link",
  expiresIn: 900,
  interval: 5,
};

export const accountsMock: AccountsApi = {
  async list() {
    await delay();
    return accounts.map((a) => ({ ...a }));
  },
  async selected() {
    return selected;
  },
  async addOffline(name) {
    await delay();
    if (!/^\w{1,16}$/.test(name)) throw new AppError("INVALID_ARGUMENT", "invalid input: name");
    const acc: Account = { id: uid("acc"), kind: "offline", name, uuid: fakeUuid(name) };
    accounts.push(acc);
    selected ??= acc.id;
    return { ...acc };
  },
  async remove(id) {
    await delay();
    const i = accounts.findIndex((a) => a.id === id);
    if (i >= 0) accounts.splice(i, 1);
    if (selected === id) selected = accounts[0]?.id ?? null;
  },
  async select(id) {
    if (!accounts.some((a) => a.id === id)) throw new AppError("ACCOUNT_NOT_FOUND", "account not found");
    selected = id;
  },
  microsoftLogin() {
    return new Promise<Account>((resolve, reject) => {
      cancel = () => reject(new AppError("CANCELLED", "login was cancelled"));
      setTimeout(() => mockEmit("accounts://device-code", mockDeviceCode), 150);
      const t = setTimeout(() => {
        cancel = null;
        const acc: Account = { id: uid("acc"), kind: "microsoft", name: "Steve_MS", uuid: fakeUuid("Steve_MS") };
        accounts.push(acc);
        selected = acc.id;
        resolve({ ...acc });
      }, 2500);
      const prev = cancel;
      cancel = () => { clearTimeout(t); prev?.(); };
    });
  },
  async cancelLogin() {
    cancel?.();
    cancel = null;
  },
  onDeviceCode: (cb) => mockOn("accounts://device-code", cb),
  async serverInfo(url) {
    await delay();
    return { url: url.startsWith("http") ? url : `https://${url}`, name: "Mock Yggdrasil" };
  },
  async addYggdrasil(server, identifier, password) {
    await delay();
    if (!password) throw new AppError("INVALID_ARGUMENT", "invalid input: password");
    const acc: Account = { id: uid("acc"), kind: "authlib", name: identifier, uuid: fakeUuid(identifier), server, serverName: "Mock Yggdrasil" };
    accounts.push(acc);
    selected = acc.id;
    return [{ ...acc }];
  },
  async skin() {
    return null;
  },
  async msClientId() {
    return clientId;
  },
  async setMsClientId(id) {
    clientId = id;
  },
};
