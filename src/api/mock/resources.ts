import type { ResourcesApi } from "../resources";
import type { ResourceItem, ResourceKind } from "../types";
import { delay } from "./bus";

const seed: Record<ResourceKind, string[]> = {
  mod: ["Sodium", "Lithium", "Iris Shaders", "JEI", "Xaero's Minimap", "Mod Menu", "Create", "Fabric API"],
  modpack: ["Better MC", "Fabulously Optimized", "All The Mods", "Cobblemon"],
  resourcepack: ["Faithful 32x", "Bare Bones", "Vanilla Tweaks", "Soartex"],
  shader: ["BSL Shaders", "Complementary", "Photon", "Euphoria Patches"],
};

function all(kind: ResourceKind): ResourceItem[] {
  return seed[kind].map((title, i) => ({
    id: `${kind}-${i}`,
    kind,
    source: "modrinth",
    title,
    summary: `${title} 的示例描述（模拟数据）`,
    author: "mock-author",
    downloads: 1_000_000 - i * 87_000,
    tags: ["fabric", "forge"].slice(0, (i % 2) + 1),
  }));
}

export const resourcesMock: ResourcesApi = {
  async search(q) {
    await delay();
    const kw = q.query.trim().toLowerCase();
    const items = all(q.kind).filter((x) => !kw || x.title.toLowerCase().includes(kw));
    return { total: items.length, items: items.slice(q.offset, q.offset + q.limit) };
  },
  async install() {
    await delay(300);
  },
};
