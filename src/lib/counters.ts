// Small persisted counters for the licence-mandated launch prompts (PCL: SystemLaunchCount, HintBuy).

const KEY = "pcl.launch";

export interface LaunchCounters {
  /** completed game launches */
  count: number;
  /** a Microsoft account was used at least once: the purchase hint is retired (PCL "HintBuy") */
  bought: boolean;
}

export function readCounters(): LaunchCounters {
  try {
    const raw = typeof localStorage !== "undefined" ? localStorage.getItem(KEY) : null;
    if (!raw) return { count: 0, bought: false };
    const p = JSON.parse(raw) as Partial<LaunchCounters>;
    return { count: Number.isInteger(p.count) && (p.count as number) >= 0 ? (p.count as number) : 0, bought: p.bought === true };
  } catch {
    return { count: 0, bought: false };
  }
}

function write(c: LaunchCounters): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(c));
  } catch {
    /* storage unavailable: prompts keep firing from the in-memory default, which is the safe direction */
  }
}

export function incrementLaunchCount(): number {
  const c = readCounters();
  c.count += 1;
  write(c);
  return c.count;
}

export function markBought(): void {
  const c = readCounters();
  if (!c.bought) write({ ...c, bought: true });
}
