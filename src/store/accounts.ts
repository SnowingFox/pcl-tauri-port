import { create } from "zustand";
import { accountsApi, type Account } from "../api";

interface AccountsState {
  accounts: Account[];
  selectedId: string | null;
  loaded: boolean;
  refresh(): Promise<void>;
  select(id: string): Promise<void>;
  addOffline(name: string): Promise<Account>;
  remove(id: string): Promise<void>;
  /** called when an external flow (MS / authlib) yields an account */
  adopt(acc: Account): void;
  selected(): Account | undefined;
}

export const useAccounts = create<AccountsState>((set, get) => ({
  accounts: [],
  selectedId: null,
  loaded: false,
  async refresh() {
    const [accounts, selectedId] = await Promise.all([accountsApi.list(), accountsApi.selected()]);
    set({ accounts, selectedId: selectedId ?? accounts[0]?.id ?? null, loaded: true });
  },
  async select(id) {
    await accountsApi.select(id);
    set({ selectedId: id });
  },
  async addOffline(name) {
    const acc = await accountsApi.addOffline(name);
    await get().refresh();
    set({ selectedId: acc.id });
    await accountsApi.select(acc.id);
    return acc;
  },
  async remove(id) {
    await accountsApi.remove(id);
    await get().refresh();
  },
  adopt(acc) {
    set((s) => ({
      accounts: s.accounts.some((a) => a.id === acc.id) ? s.accounts : [...s.accounts, acc],
      selectedId: acc.id,
    }));
  },
  selected: () => get().accounts.find((a) => a.id === get().selectedId),
}));
