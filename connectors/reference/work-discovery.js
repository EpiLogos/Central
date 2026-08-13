import { readdir } from "node:fs/promises";
import { join } from "node:path";
import { CONNECTOR_API_VERSION, defineConnector, WorkDiscovery } from "../../ctrl/sdk/index.js";

const metadata = (id, displayName) => ({
  apiVersion: CONNECTOR_API_VERSION,
  id,
  version: "0.1.0",
  displayName,
  ports: [{ id: WorkDiscovery.id, version: WorkDiscovery.version }],
  platforms: ["*"],
  runtimeRequirements: ["node>=22"],
  dependencyProbes: [],
  configurationRequirements: [],
  mutationScope: "read-only",
});

export function createFilesystemWorkDiscoveryConnector() {
  return defineConnector({
    manifest: metadata("reference.work-filesystem", "Reference filesystem Work discovery"),
    async probe() { return { available: true }; },
    implementations: { [WorkDiscovery.id]: { async list({ workRoot }) {
      let entries;
      try { entries = await readdir(workRoot, { withFileTypes: true }); }
      catch (error) { if (error?.code === "ENOENT") return { items: [] }; throw error; }
      return { items: entries.filter((entry) => entry.isDirectory()).map((entry) => ({ name: entry.name, path: join(workRoot, entry.name) })).sort((a, b) => a.name.localeCompare(b.name)) };
    } } },
  });
}

export function createStaticWorkDiscoveryConnector(items = []) {
  return defineConnector({
    manifest: metadata("reference.work-static", "Reference static Work discovery"),
    async probe() { return { available: true }; },
    implementations: { [WorkDiscovery.id]: { async list() { return { items: [...items].sort((a, b) => a.name.localeCompare(b.name)) }; } } },
  });
}
