import type { ResourcesApi } from "../resources";
import type { ResourceItem, ResourceKind, ResourceSource } from "../types";
import { delay, mockEmit, mockRunTask, uid } from "./bus";

const seed: Record<ResourceKind, string[]> = {
  mod: ["Sodium", "Lithium", "Iris Shaders", "JEI", "Xaero's Minimap", "Mod Menu", "Create", "Fabric API"],
  modpack: ["Better MC", "Fabulously Optimized", "All The Mods", "Cobblemon"],
  resourcepack: ["Faithful 32x", "Bare Bones", "Vanilla Tweaks", "Soartex"],
  shader: ["BSL Shaders", "Complementary", "Photon", "Euphoria Patches"],
};

function all(kind: ResourceKind, source: ResourceSource): ResourceItem[] {
  return seed[kind].map((title, i) => ({
    id: `${source}-${kind}-${i}`,
    slug: title.toLowerCase().replace(/\W+/g, "-"),
    kind,
    source,
    title,
    summary: `${title} 的示例描述（模拟数据）`,
    author: "mock-author",
    downloads: 1_000_000 - i * 87_000,
    follows: 1000 - i * 10,
    tags: ["fabric", "forge"].slice(0, (i % 2) + 1),
    pageUrl: `https://modrinth.com/${kind}/${title.toLowerCase().replace(/\W+/g, "-")}`,
  }));
}

export const resourcesMock: ResourcesApi = {
  async search(q) {
    await delay();
    const kw = q.query.trim().toLowerCase();
    const sources: ResourceSource[] = q.source === "all" ? ["modrinth", "curseforge"] : [q.source];
    const items = sources.flatMap((s) => all(q.kind, s)).filter((x) => !kw || x.title.toLowerCase().includes(kw));
    return { total: items.length, items: items.slice(q.offset, q.offset + q.limit), warnings: [] };
  },
  async installResource() {
    await delay(100);
    const taskId = uid("res");
    mockRunTask(taskId, "resource", { kind: "resource", steps: 4, interval: 200 });
    return taskId;
  },
  async installModpack(_source, name) {
    await delay(100);
    const taskId = uid("pack");
    mockRunTask(taskId, name, { kind: "modpack", steps: 6, interval: 250 });
    setTimeout(() => mockEmit("app://modpack-report", { taskId, instanceId: `/mock/versions/${name}`, missingFiles: [] }), 6 * 250 - 50);
    return taskId;
  },
};
