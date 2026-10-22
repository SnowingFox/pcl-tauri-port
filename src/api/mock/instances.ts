import type { InstancesApi } from "../instances";
import type { Instance, InstanceSettings, ModEntry, SaveEntry } from "../types";
import { defaultInstanceSettings } from "../defaults";
import { AppError } from "../errors";
import { delay } from "./bus";

export const MOCK_ROOT = "/Users/demo/Library/Application Support/minecraft";
const ROOT = MOCK_ROOT;
const mk = (name: string, mc: string, loader: Instance["loader"], loaderVersion?: string, extra: Partial<Instance> = {}): Instance => ({
  id: `${ROOT}/versions/${name}`, name, root: ROOT, path: `${ROOT}/versions/${name}`, mcVersion: mc, loader, loaderVersion,
  loaders: loader === "vanilla" ? [] : [{ kind: loader, version: loaderVersion }],
  description: loader === "vanilla" ? `正式版 ${mc}` : `正式版 ${mc}, ${loader} ${loaderVersion}`,
  kind: loader === "vanilla" ? "release" : loader, starred: false, hidden: false, modable: loader !== "vanilla", isolated: true,
  gameDir: `${ROOT}/versions/${name}`, ...extra,
});

const seed = (): Instance[] => [
  mk("1.21.1-Fabric", "1.21.1", "fabric", "0.16.5", { starred: true }),
  mk("1.20.1-Forge", "1.20.1", "forge", "47.3.0"),
  mk("1.21.4", "1.21.4", "vanilla"),
];
let instances = seed();
const settings = new Map<string, InstanceSettings>();
const mods = new Map<string, ModEntry[]>();

export function resetInstancesMock(): void {
  instances = seed();
  settings.clear();
  mods.clear();
}

function modsFor(id: string): ModEntry[] {
  let m = mods.get(id);
  if (!m) {
    const d = (file: string, name: string, version: string, enabled: boolean, size: number): ModEntry => ({
      path: `${id}/mods/${file}${enabled ? "" : ".disabled"}`, fileName: `${file}${enabled ? "" : ".disabled"}`, name, version, enabled, size, authors: [], loaders: ["fabric"],
    });
    m = [d("sodium-0.6.0.jar", "Sodium", "0.6.0", true, 1_200_000), d("lithium-0.14.0.jar", "Lithium", "0.14.0", true, 800_000), d("iris-1.8.0.jar", "Iris Shaders", "1.8.0", false, 2_400_000)];
    mods.set(id, m);
  }
  return m;
}

const find = (i: Pick<Instance, "name">) => {
  const found = instances.find((x) => x.name === i.name);
  if (!found) throw new AppError("INSTANCE_NOT_FOUND", `instance not found: ${i.name}`);
  return found;
};

export const instancesMock: InstancesApi = {
  async folders() { return [{ name: "默认文件夹", path: ROOT }]; },
  async addFolder(name, path) { return [{ name: "默认文件夹", path: ROOT }, { name, path }]; },
  async removeFolder() { return [{ name: "默认文件夹", path: ROOT }]; },
  async selectFolder() {},
  async scan() {
    await delay();
    return instances.map((i) => ({ ...i }));
  },
  async select() {},
  async rename(i, newName) {
    await delay(40);
    if (/[\\/:*?"<>|]/.test(newName) || !newName.trim()) throw new AppError("INVALID_ARGUMENT", "invalid instance name");
    if (instances.some((x) => x.name === newName)) throw new AppError("INVALID_ARGUMENT", "instance already exists");
    const src = find(i);
    const next = mk(newName, src.mcVersion, src.loader, src.loaderVersion, { starred: src.starred });
    instances = instances.map((x) => (x === src ? next : x));
    return { ...next };
  },
  async duplicate(i, newName) {
    await delay(40);
    if (/[\\/:*?"<>|]/.test(newName) || !newName.trim()) throw new AppError("INVALID_ARGUMENT", "invalid instance name");
    if (instances.some((x) => x.name === newName)) throw new AppError("INVALID_ARGUMENT", "instance already exists");
    const src = find(i);
    const next = mk(newName, src.mcVersion, src.loader, src.loaderVersion);
    instances.push(next);
    return { ...next };
  },
  async remove(i) {
    const src = find(i);
    instances = instances.filter((x) => x !== src);
    return "~/.Trash/" + src.name;
  },
  async openIn() {},
  async getSettings(i) {
    return structuredClone(settings.get(i.path) ?? defaultInstanceSettings());
  },
  async setSettings(i, s) {
    settings.set(i.path, structuredClone(s));
  },
  async mods(i) {
    await delay();
    return modsFor(i.id).map((m) => ({ ...m }));
  },
  async setModEnabled(mod, enabled) {
    const list = [...mods.values()].find((l) => l.some((m) => m.path === mod.path));
    const m = list?.find((x) => x.path === mod.path);
    if (!m) throw new AppError("IO", "mod not found");
    const base = m.path.replace(/\.disabled$/, "");
    m.enabled = enabled;
    m.path = enabled ? base : `${base}.disabled`;
    m.fileName = m.path.split("/").pop()!;
    return m.path;
  },
  async saves(): Promise<SaveEntry[]> {
    await delay();
    return [
      { folder: "New World", name: "新的世界", path: "/mock/saves/New World", lastPlayed: Date.now() - 7200_000, size: 52_000_000 },
      { folder: "Survival", name: "生存存档", path: "/mock/saves/Survival", lastPlayed: Date.now() - 5 * 86400_000, size: 310_000_000 },
    ];
  },
};
