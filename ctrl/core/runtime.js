import { createFilesystemWorkDiscoveryConnector, createStaticWorkDiscoveryConnector } from "../../connectors/reference/work-discovery.js";
import { createCoreActionRegistry } from "./actions.js";
import { ConnectorRegistry } from "./connectors.js";

export function createDefaultRuntime() {
  const connectors = new ConnectorRegistry()
    .register(createFilesystemWorkDiscoveryConnector())
    .register(createStaticWorkDiscoveryConnector());

  return { actions: createCoreActionRegistry(), connectors };
}
