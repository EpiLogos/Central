import { readdir } from "node:fs/promises";
import { join } from "node:path";

function manifest(id, displayName) {
  return Object.freeze({
    id,
    version: "0.1.0",
    displayName,
    ports: ["WorkDiscovery"],
    platforms: ["*"],
    runtimeRequirements: ["node>=22"],
    dependencyProbes: [],
    configurationRequirements: [],
    mutationScope: "read-only",
  });
}

export function createFilesystemWorkDiscoveryConnector() {
  return {
    manifest: manifest("reference.work-filesystem", "Reference filesystem Work discovery"),
    async probe() { return { available: true }; },
    implementations: {
      WorkDiscovery: {
        async list({ workRoot }) {
          let entries;
          try { entries = await readdir(workRoot, { withFileTypes: true }); }
          catch (error) { if (error && error.code === "ENOENT") return { items: [] }; throw error; }
          return { items: entries.filter((entry) => entry.isDirectory()).map((entry) => ({ name: entry.name, path: join(workRoot, entry.name) })).sort((a, b) => a.name.localeCompare(b.name)) };
        },
      },
    },
  };
}

export function createStaticWorkDiscoveryConnector(items = []) {
  return {
    manifest: manifest("reference.work-static", "Reference static Work discovery"),
    async probe() { return { available: true }; },
    implementations: { WorkDiscovery: { async list() { return { items: [...items].sort((a, b) => a.name.localeCompare(b.name)) }; } } },
  };
}
