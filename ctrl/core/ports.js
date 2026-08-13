function assertObject(value, label) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new TypeError(`${label} must be an object.`);
  }
}

function validateWorkDiscoveryInput(input) {
  assertObject(input, "WorkDiscovery.list input");
  if (typeof input.workRoot !== "string" || input.workRoot.trim() === "") {
    throw new TypeError("WorkDiscovery.list input.workRoot must be a non-empty path.");
  }
}

function validateWorkDiscoveryOutput(output) {
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
  operations: Object.freeze({
    list: Object.freeze({
      validateInput: validateWorkDiscoveryInput,
      validateOutput: validateWorkDiscoveryOutput,
    }),
  }),
});

export async function invokePort(resolution, port, operation, input) {
  const contract = port.operations[operation];
  if (!contract) throw new TypeError(`Port ${port.id} does not define operation ${operation}.`);
  contract.validateInput(input);
  const implementation = resolution.connector.implementations[port.id]?.[operation];
  if (typeof implementation !== "function") {
    throw new TypeError(`Connector ${resolution.connector.manifest.id} does not implement ${port.id}.${operation}.`);
  }
  const output = await implementation(input);
  contract.validateOutput(output);
  return output;
}
