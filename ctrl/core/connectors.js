function validateManifest(manifest) {
  if (!manifest || typeof manifest !== "object") throw new TypeError("Connector manifest must be an object.");
  for (const field of ["id", "version", "displayName", "ports", "platforms", "mutationScope"]) {
    if (!(field in manifest)) throw new TypeError(`Connector manifest is missing ${field}.`);
  }
  if (typeof manifest.id !== "string" || manifest.id === "") throw new TypeError("Connector id must be a non-empty string.");
  if (!Array.isArray(manifest.ports) || manifest.ports.length === 0) throw new TypeError("Connector ports must be non-empty.");
  if (!Array.isArray(manifest.platforms) || manifest.platforms.length === 0) throw new TypeError("Connector platforms must be non-empty.");
}

function validateConnector(connector) {
  if (!connector || typeof connector !== "object") throw new TypeError("Connector must be an object.");
  validateManifest(connector.manifest);
  if (typeof connector.probe !== "function") throw new TypeError(`Connector ${connector.manifest.id} must provide probe().`);
  if (!connector.implementations || typeof connector.implementations !== "object") {
    throw new TypeError(`Connector ${connector.manifest.id} must provide implementations.`);
  }
  for (const portId of connector.manifest.ports) {
    if (!connector.implementations[portId]) {
      throw new TypeError(`Connector ${connector.manifest.id} declares ${portId} without an implementation.`);
    }
  }
}

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
    return [...this.#connectors.values()]
      .map(({ manifest }) => manifest)
      .sort((a, b) => a.id.localeCompare(b.id));
  }

  async resolve(port, { platform = process.platform } = {}) {
    const eligible = [];
    const ineligible = [];

    for (const connector of this.#connectors.values()) {
      const { manifest } = connector;
      if (!manifest.ports.includes(port.id)) continue;
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
