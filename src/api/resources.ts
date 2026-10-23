import { call, useMockApi } from "./runtime";
import * as mock from "./mock/resources";
import { toResourcePage } from "./adapters";
import { describeError, normalizeError } from "./errors";
import type { RawSearchResult } from "./raw";
import type { ModpackSource, ResourceKind, ResourcePage, ResourceQuery, ResourceSource } from "./types";

export const RESOURCE_CMD = {
  search: "resource_search",
  installResource: "app_install_resource",
  installModpack: "app_install_modpack",
} as const;

export interface InstallResourceRequest {
  platform: ResourceSource;
  projectId: string;
  versionId?: string;
  kind: Exclude<ResourceKind, "modpack">;
  /** absolute instance path */
  instanceId: string;
}

export interface ResourcesApi {
  search(q: ResourceQuery): Promise<ResourcePage>;
  /** returns a download task id */
  installResource(req: InstallResourceRequest): Promise<string>;
  /** installs a modpack as a NEW instance; returns a download task id */
  installModpack(source: ModpackSource, instanceName: string): Promise<string>;
}

const PLATFORM_LABEL: Record<ResourceSource, string> = { modrinth: "Modrinth", curseforge: "CurseForge" };

async function searchOne(platform: ResourceSource, q: ResourceQuery): Promise<RawSearchResult> {
  const page = Math.floor(q.offset / q.limit);
  return call<RawSearchResult>(RESOURCE_CMD.search, {
    platform,
    kind: q.kind,
    query: {
      query: q.query,
      mc_version: q.mcVersion ?? null,
      loader: q.loader && q.loader !== "vanilla" ? q.loader : null,
      categories: [],
      page,
      page_size: q.limit,
    },
  });
}

/** The real (Tauri) implementation; exported for contract tests. */
export const realResourcesApi: ResourcesApi = {
  async search(q) {
    const platforms: ResourceSource[] = q.source === "all" ? ["modrinth", "curseforge"] : [q.source];
    const settled = await Promise.allSettled(platforms.map((p) => searchOne(p, q)));
    // A single failing platform degrades to a warning; only fail when every platform failed.
    if (settled.every((s) => s.status === "rejected")) throw normalizeError((settled[0] as PromiseRejectedResult).reason);
    return toResourcePage(
      settled.map((s, i) =>
        s.status === "fulfilled"
          ? { kind: q.kind, result: s.value }
          : { kind: q.kind, error: `${PLATFORM_LABEL[platforms[i]]}：${describeError(s.reason).text}` },
      ),
    );
  },
  async installResource(req) {
    const r = await call<{ taskId: string }>(RESOURCE_CMD.installResource, {
      platform: req.platform,
      projectId: req.projectId,
      versionId: req.versionId ?? null,
      kind: req.kind,
      instanceId: req.instanceId,
    });
    return r.taskId;
  },
  async installModpack(source, instanceName) {
    const r = await call<{ taskId: string }>(RESOURCE_CMD.installModpack, { source, instanceName });
    return r.taskId;
  },
};

export const resourcesApi: ResourcesApi = useMockApi ? mock.resourcesMock : realResourcesApi;
