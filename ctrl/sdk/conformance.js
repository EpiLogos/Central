import { WorkDiscovery } from "./ports/work-discovery.js";
import { validateConnector } from "./connector.js";

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

export async function runWorkDiscoveryConformance(connector, { workRoot, expectedNames } = {}) {
  validateConnector(connector);
  const declaration = connector.manifest.ports.find((port) => port.id === WorkDiscovery.id);
  assert(declaration, `Connector ${connector.manifest.id} does not declare ${WorkDiscovery.id}.`);
  assert(
    declaration.version === WorkDiscovery.version,
    `Connector ${connector.manifest.id} declares ${WorkDiscovery.id} ${declaration.version}; expected ${WorkDiscovery.version}.`,
  );

  const probe = await connector.probe({ port: WorkDiscovery.id, platform: process.platform });
  assert(probe && typeof probe.available === "boolean", "Connector probe must return { available: boolean }.");
  assert(probe.available, `Connector is not available for conformance: ${probe.reason ?? "no reason provided"}`);

  const implementation = connector.implementations[WorkDiscovery.id];
  assert(typeof implementation?.list === "function", `${WorkDiscovery.id}.list is required.`);

  const input = { workRoot };
  WorkDiscovery.operations.list.validateInput(input);
  const first = await implementation.list(input);
  WorkDiscovery.operations.list.validateOutput(first);
  const second = await implementation.list(input);
  WorkDiscovery.operations.list.validateOutput(second);

  assert(
    JSON.stringify(first) === JSON.stringify(second),
    `${WorkDiscovery.id}.list must be stable when the Work source is unchanged.`,
  );

  if (expectedNames) {
    assert(
      JSON.stringify(first.items.map((item) => item.name)) === JSON.stringify(expectedNames),
      `Unexpected Work item names from ${connector.manifest.id}.`,
    );
  }

  return {
    ok: true,
    port: { id: WorkDiscovery.id, version: WorkDiscovery.version },
    connector: { id: connector.manifest.id, version: connector.manifest.version },
    checks: ["manifest", "port-compatibility", "probe", "typed-output", "repeat-stability"],
  };
}
