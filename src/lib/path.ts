/** `<root>/versions/<name>` -> { root, name }; null when the path has another shape. */
export function splitInstancePath(path: string): { root: string; name: string } | null {
  const m = /^(.*)\/versions\/([^/]+)\/?$/.exec(path);
  return m ? { root: m[1], name: m[2] } : null;
}

const INVALID_NAME = /[\\/:*?"<>|]/;
/** Mirrors the backend rules for instance names (no separators or `: * ? " < > |`, not `.`/`..`). */
export function validInstanceName(name: string): boolean {
  const n = name.trim();
  return n.length > 0 && n.length <= 100 && n !== "." && n !== ".." && !INVALID_NAME.test(n) && n === name;
}

/** Backend rule for offline names: 1-16 characters of [A-Za-z0-9_]. */
export function validPlayerName(name: string): boolean {
  return /^[A-Za-z0-9_]{1,16}$/.test(name);
}
