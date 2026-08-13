const CONTRACT_VERSION = "1.0.0";

function assertObject(value, label) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new TypeError(`${label} must be an object.`);
  }
}

function validateInput(input) {
  assertObject(input, "WorkDiscovery.list input");
  if (typeof input.workRoot !== "string" || input.workRoot.trim() === "") {
    throw new TypeError("WorkDiscovery.list input.workRoot must be a non-empty path.");
  }
}

function validateOutput(output) {
  assertObject(output, "WorkDiscovery.list output");
  if (!Array.isArray(output.items)) {
    throw new TypeError("WorkDiscovery.list output.items must be an array.");
  }
  for (const item of output.items) {
    assertObject(item, "WorkDiscovery item");
    if (typeof item.name !== "string" || item.name === "") {
      throw new TypeError("WorkDiscovery item.name must be a non-empty string.");
    }
    if (typeof item.path !== "string" || item.path === "") {
      throw new TypeError("WorkDiscovery item.path must be a non-empty string.");
    }
  }
}

export const WorkDiscovery = Object.freeze({
  id: "WorkDiscovery",
  version: CONTRACT_VERSION,
  purpose: "Discover and resolve ordinary Work items without requiring a Central-specific project format.",
  mutationClass: "read-only",
  operations: Object.freeze({
    list: Object.freeze({
      inputType: "WorkDiscoveryListInput",
      outputType: "WorkDiscoveryListOutput",
      validateInput,
      validateOutput,
      deterministic: true,
      idempotent: true,
    }),
  }),
});
