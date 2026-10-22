import type { AccountsApi } from "../accounts";
import type { Account } from "../types";
import { delay, mockEmit, mockOn, uid } from "./bus";

const accounts: Account[] = [];
let selected: string | null = null;
let cancelled = false;

function fakeUuid(name: string): string {
  let h = 0;
  for (const c of name) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  const hex = h.toString(16).padStart(8, "0");
  return `${hex}-0000-4000-8000-${hex}${hex.slice(0, 4)}`;
}

export function resetAccountsMock(): void {
  accounts.length = 0;
  selected = null;
  cancelled = false;
}

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
    if (!/^\w{3,16}$/.test(name)) throw new Error("invalid_name");
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
    if (!accounts.some((a) => a.id === id)) throw new Error("not_found");
    selected = id;
  },
  async msLoginStart() {
    await delay();
    cancelled = false;
    setTimeout(() => {
      if (cancelled) return;
      mockEmit("accounts://ms-login", { state: "pending" });
      setTimeout(() => {
        if (cancelled) return;
        const acc: Account = { id: uid("acc"), kind: "microsoft", name: "Steve_MS", uuid: fakeUuid("Steve_MS") };
        accounts.push(acc);
        selected = acc.id;
        mockEmit("accounts://ms-login", { state: "success", account: acc });
      }, 2500);
    }, 200);
    return {
      userCode: "A1B2C3D4",
      verificationUri: "https://www.microsoft.com/link",
      expiresIn: 900,
      interval: 5,
    };
  },
  async msLoginCancel() {
    cancelled = true;
  },
  async authlibLogin(server, username, password) {
    await delay();
    if (!password) throw new Error("bad_credentials");
    const acc: Account = { id: uid("acc"), kind: "authlib", name: username, uuid: fakeUuid(username), server };
    accounts.push(acc);
    selected = acc.id;
    return { ...acc };
  },
  onMsLogin: (cb) => mockOn("accounts://ms-login", cb),
};
