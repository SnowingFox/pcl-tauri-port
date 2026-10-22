import { call, useMockApi } from "./runtime";
import * as mock from "./mock/resources";
import type { ResourcePage, ResourceQuery } from "./types";

export const RESOURCE_CMD = {
  search: "resources_search",
  install: "resources_install",
} as const;

export interface ResourcesApi {
  search(q: ResourceQuery): Promise<ResourcePage>;
  /** install a resource into an instance (mods/packs/shaders folder) */
  install(source: string, id: string, instanceId: string): Promise<void>;
}

const real: ResourcesApi = {
  search: (query) => call(RESOURCE_CMD.search, { query }),
  install: (source, id, instanceId) => call(RESOURCE_CMD.install, { source, id, instanceId }),
};

export const resourcesApi: ResourcesApi = useMockApi ? mock.resourcesMock : real;
