import { create } from "zustand";
import { accountsApi, type Account } from "../api";
import { markBought } from "../lib/counters";

interface AccountsState {
  accounts: Account[];
  selectedId: string | null;
  loaded: boolean;
  refresh(): Promise<void>;
  select(id: string): Promise<void>;
  addOffline(name: string): Promise<Account>;
  remove(id: string): Promise<void>;
  /** called when an external flow (Microsoft / Yggdrasil) yields accounts; the first one becomes selected */
  adopt(...acc: Account[]): Promise<void>;
  selected(): Account | undefined;
}

export const useAccounts = create<AccountsState>((set, get) => ({
  accounts: [],
  selectedId: null,
  loaded: false,
  async refresh() {
    const [accounts, selectedId] = await Promise.all([accountsApi.list(), accountsApi.selected()]);
    set({ accounts, selectedId: accounts.some((a) => a.id === selectedId) ? selectedId : (accounts[0]?.id ?? null), loaded: true });
  },
  async select(id) {
    await accountsApi.select(id);
    set({ selectedId: id });
  },
  async addOffline(name) {
    const acc = await accountsApi.addOffline(name);
    await get().adopt(acc);
    return acc;
  },
  async remove(id) {
    await accountsApi.remove(id);
    await get().refresh();
  },
  async adopt(...accs) {
    if (!accs.length) return;
    if (accs.some((a) => a.kind === "microsoft")) markBought();
    await accountsApi.select(accs[0].id);
    await get().refresh();
    set({ selectedId: accs[0].id });
  },
  selected: () => get().accounts.find((a) => a.id === get().selectedId),
}));
