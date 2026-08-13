import { validateConnector } from "../sdk/index.js";

function platformMatches(manifest, platform) {
  return manifest.platforms.includes("*") || manifest.platforms.includes(platform);
}

export class ConnectorRegistry {
  #connectors = new Map();

  register(connector) {
    validateConnector(connector);
    const id = connector.manifest.id;
    if (this.#connectors.has(id)) throw new TypeError(`Connector already registered: ${id}`);
    this.#connectors.set(id, connector);
    return this;
  }

  list() {
    return [...this.#connectors.values()].map(({ manifest }) => manifest).sort((a, b) => a.id.localeCompare(b.id));
  }

  async resolve(port, { platform = process.platform } = {}) {
    const eligible = [];
    const ineligible = [];
    for (const connector of this.#connectors.values()) {
      const { manifest } = connector;
      const declaration = manifest.ports.find((candidate) => candidate.id === port.id);
      if (!declaration) continue;
      if (declaration.version !== port.version) {
        ineligible.push({ id: manifest.id, reason: `incompatible ${port.id} contract: ${declaration.version}` });
        continue;
      }
      if (!platformMatches(manifest, platform)) {
        ineligible.push({ id: manifest.id, reason: `unsupported platform: ${platform}` });
        continue;
      }
      let probe;
      try {
        probe = await connector.probe({ port: port.id, platform });
      } catch (error) {
        ineligible.push({ id: manifest.id, reason: `capability probe failed: ${error instanceof Error ? error.message : String(error)}` });
        continue;
      }
      if (!probe || probe.available !== true) {
        ineligible.push({ id: manifest.id, reason: probe?.reason ?? "capability unavailable" });
        continue;
      }
      eligible.push(connector);
    }
    eligible.sort((a, b) => a.manifest.id.localeCompare(b.manifest.id));
    ineligible.sort((a, b) => a.id.localeCompare(b.id));
    const selected = eligible[0] ?? null;
    return {
      port: port.id,
      connector: selected,
      diagnostics: {
        eligible: eligible.map(({ manifest }) => ({ id: manifest.id, version: manifest.version })),
        ineligible,
        selectedConnector: selected ? { id: selected.manifest.id, version: selected.manifest.version } : null,
      },
    };
  }
}
