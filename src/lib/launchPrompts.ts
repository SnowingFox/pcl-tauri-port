import type { AccountKind } from "../api/types";

/**
 * Genuine-purchase and sponsorship prompts shown before launching.
 *
 * Required by the Plain Craft Launcher LICENCE (heavy use, rule 4): the launcher must nag at least as
 * persistently as PCL's `McLaunchPrecheck`. PCL shows
 *  - a purchase hint for non-Microsoft accounts on launches #3, 8, 15, 30, 50, ... (until a Microsoft login happened),
 *  - a sponsorship hint for every account on launches #10, 20, 40, 60, ... (release builds).
 * Here both are blocking dialogs (stricter than PCL's non-blocking message box), and the purchase hint also
 * fires on the very first offline launch. The schedule below must only ever be made denser, never sparser.
 */
export const BUY_URL = "https://www.minecraft.net/";
export const SPONSOR_URL = "https://meloong.com/afd/a/LTCat";

/** PCL: launches (counted before the current one) at which the purchase hint appears. 0 is our addition. */
export const BUY_COUNTS: readonly number[] = [
  0, 3, 8, 15, 30, 50, 70, 90, 110, 130, 180, 220, 280, 330, 380, 450, 550, 660, 750, 880, 950, 1100, 1300, 1500, 1700, 1900,
];
/** PCL: launches at which the sponsorship hint appears. */
export const SPONSOR_COUNTS: readonly number[] = [
  10, 20, 40, 60, 80, 100, 120, 150, 200, 250, 300, 350, 400, 500, 600, 700, 800, 900, 1000, 1200, 1400, 1600, 1800, 2000,
];

export interface LaunchPromptInput {
  /** number of launches completed so far */
  launchCount: number;
  accountKind: AccountKind;
  /** true once a Microsoft account has been used (PCL `HintBuy`): the purchase hint is retired */
  boughtHint: boolean;
}
export interface LaunchPromptDecision {
  buy: boolean;
  sponsor: boolean;
}

export function decideLaunchPrompts(i: LaunchPromptInput): LaunchPromptDecision {
  return {
    buy: i.accountKind !== "microsoft" && !i.boughtHint && BUY_COUNTS.includes(i.launchCount),
    sponsor: SPONSOR_COUNTS.includes(i.launchCount),
  };
}

export const needsPrompt = (d: LaunchPromptDecision): boolean => d.buy || d.sponsor;
