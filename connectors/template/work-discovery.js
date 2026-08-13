import { CONNECTOR_API_VERSION, defineConnector, WorkDiscovery } from "../../ctrl/sdk/index.js";

export function createWorkDiscoveryConnectorTemplate({
  id,
  displayName,
  platforms = ["*"],
  probe,
  listWorkItems,
}) {
  return defineConnector({
    manifest: {
      apiVersion: CONNECTOR_API_VERSION,
      id,
      version: "0.1.0",
      displayName,
      ports: [{ id: WorkDiscovery.id, version: WorkDiscovery.version }],
      platforms,
      runtimeRequirements: ["node>=22"],
      dependencyProbes: [],
      configurationRequirements: [],
      mutationScope: "read-only",
    },
    probe: probe ?? (async () => ({ available: true })),
    implementations: {
      [WorkDiscovery.id]: {
        list: listWorkItems,
      },
    },
  });
}
