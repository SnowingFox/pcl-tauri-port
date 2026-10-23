import { create } from "zustand";
import type { Account } from "../api";
import { readCounters } from "../lib/counters";
import { decideLaunchPrompts, needsPrompt, type LaunchPromptDecision } from "../lib/launchPrompts";
import { reportError } from "../lib/notify";
import { useGame } from "./game";

export interface PendingPrompt {
  decision: LaunchPromptDecision;
  /** number of launches completed so far */
  count: number;
  account: Account;
}

interface PromptState {
  pending: PendingPrompt | null;
  /**
   * The only entry point the UI uses to start the game. Shows the genuine-purchase / sponsorship prompt first
   * when the schedule says so (PCL LICENCE, heavy use rule 4), otherwise launches immediately.
   */
  requestLaunch(account: Account): Promise<void>;
  /** user pressed "continue" in the prompt */
  confirm(): Promise<void>;
  cancel(): void;
}

async function doStart(account: Account): Promise<void> {
  try {
    await useGame.getState().start(account.id);
  } catch (e) {
    reportError(e);
  }
}

export const usePrompt = create<PromptState>((set, get) => ({
  pending: null,
  async requestLaunch(account) {
    if (get().pending || useGame.getState().phase !== "idle") return;
    const c = readCounters();
    const decision = decideLaunchPrompts({ launchCount: c.count, accountKind: account.kind, boughtHint: c.bought });
    if (!needsPrompt(decision)) return doStart(account);
    set({ pending: { decision, count: c.count, account } });
  },
  async confirm() {
    const p = get().pending;
    if (!p) return;
    set({ pending: null });
    await doStart(p.account);
  },
  cancel: () => set({ pending: null }),
}));
