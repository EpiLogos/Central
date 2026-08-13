export const CONNECTOR_API_VERSION = "central.connector/v1";

export function validateConnectorManifest(manifest) {
  if (!manifest || typeof manifest !== "object" || Array.isArray(manifest)) {
    throw new TypeError("Connector manifest must be an object.");
  }
  for (const field of [
    "apiVersion",
    "id",
    "version",
    "displayName",
    "ports",
    "platforms",
    "runtimeRequirements",
    "dependencyProbes",
    "configurationRequirements",
    "mutationScope",
  ]) {
    if (!(field in manifest)) throw new TypeError(`Connector manifest is missing ${field}.`);
  }
  if (manifest.apiVersion !== CONNECTOR_API_VERSION) {
    throw new TypeError(`Unsupported Connector API version: ${manifest.apiVersion}`);
  }
  if (typeof manifest.id !== "string" || manifest.id.trim() === "") {
    throw new TypeError("Connector id must be a non-empty string.");
  }
  if (typeof manifest.version !== "string" || manifest.version.trim() === "") {
    throw new TypeError("Connector version must be a non-empty string.");
  }
  if (!Array.isArray(manifest.ports) || manifest.ports.length === 0) {
    throw new TypeError("Connector ports must be a non-empty array.");
  }
  for (const port of manifest.ports) {
    if (!port || typeof port !== "object" || typeof port.id !== "string" || typeof port.version !== "string") {
      throw new TypeError("Each Connector port declaration must include id and version.");
    }
  }
  if (!Array.isArray(manifest.platforms) || manifest.platforms.length === 0) {
    throw new TypeError("Connector platforms must be a non-empty array.");
  }
  return manifest;
}

export function validateConnector(connector) {
  if (!connector || typeof connector !== "object" || Array.isArray(connector)) {
    throw new TypeError("Connector must be an object.");
  }
  validateConnectorManifest(connector.manifest);
  if (typeof connector.probe !== "function") {
    throw new TypeError(`Connector ${connector.manifest.id} must provide probe().`);
  }
  if (!connector.implementations || typeof connector.implementations !== "object") {
    throw new TypeError(`Connector ${connector.manifest.id} must provide implementations.`);
  }
  for (const port of connector.manifest.ports) {
    if (!connector.implementations[port.id]) {
      throw new TypeError(`Connector ${connector.manifest.id} declares ${port.id} without an implementation.`);
    }
  }
  return connector;
}

export function defineConnector(connector) {
  validateConnector(connector);
  return Object.freeze({
    ...connector,
    manifest: Object.freeze({
      ...connector.manifest,
      ports: Object.freeze(connector.manifest.ports.map((port) => Object.freeze({ ...port }))),
    }),
  });
}
