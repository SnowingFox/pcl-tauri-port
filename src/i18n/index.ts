import zhCN from "./zh-CN";

/** Simple i18n: only zh-CN today, but keys are resolved through t() so more locales can be added. */
const locales: Record<string, unknown> = { "zh-CN": zhCN };
let current = "zh-CN";

export function setLocale(l: string): void {
  if (locales[l]) current = l;
}

export function t(key: string, vars?: Record<string, string | number>): string {
  let node: unknown = locales[current];
  for (const part of key.split(".")) {
    if (node && typeof node === "object" && part in (node as object)) node = (node as Record<string, unknown>)[part];
    else return key;
  }
  if (typeof node !== "string") return key;
  return vars ? node.replace(/\{(\w+)\}/g, (_, k) => String(vars[k] ?? `{${k}}`)) : node;
}

/** Return a raw (non-string) value such as the help array. */
export function tRaw<T>(key: string): T | undefined {
  let node: unknown = locales[current];
  for (const part of key.split(".")) {
    if (node && typeof node === "object" && part in (node as object)) node = (node as Record<string, unknown>)[part];
    else return undefined;
  }
  return node as T;
}
