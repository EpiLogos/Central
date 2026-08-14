const CONTRACT_VERSION = "1.0.0";

function assertObject(value, label) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new TypeError(`${label} must be an object.`);
  }
}

function assertText(value, label) {
  if (typeof value !== "string" || value.trim() === "") throw new TypeError(`${label} must be a non-empty string.`);
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

const PACKAGE_STATES = new Set(["present", "absent"]);
const CONFIGURATION_STATES = new Set(["present", "absent"]);
const SERVICE_STATES = new Set(["running", "stopped", "enabled", "disabled"]);

function validateStateItems(items, label, states) {
  if (!Array.isArray(items)) throw new TypeError(`${label} must be an array.`);
  const ids = new Set();
  for (const item of items) {
    assertObject(item, `${label} item`);
    assertText(item.id, `${label} item.id`);
    if (ids.has(item.id)) throw new TypeError(`${label} contains duplicate id ${item.id}.`);
    ids.add(item.id);
    if (!states.has(item.state)) throw new TypeError(`${label} item.state is invalid for ${item.id}.`);
  }
}

function validateMachineInspectionInput(input) {
  assertObject(input, "MachineInspector.inspect input");
}

function validateMachineInspectionOutput(output) {
  assertObject(output, "MachineInspector.inspect output");
  assertObject(output.host, "MachineInspector.inspect output.host");
  assertText(output.host.platform, "MachineInspector.inspect output.host.platform");
  assertText(output.host.architecture, "MachineInspector.inspect output.host.architecture");
  if (!Array.isArray(output.capabilities) || output.capabilities.some((item) => typeof item !== "string" || item.trim() === "")) {
    throw new TypeError("MachineInspector.inspect output.capabilities must be an array of non-empty strings.");
  }
  validateStateItems(output.packages, "MachineInspector.inspect output.packages", PACKAGE_STATES);
  validateStateItems(output.configurations, "MachineInspector.inspect output.configurations", CONFIGURATION_STATES);
  validateStateItems(output.services, "MachineInspector.inspect output.services", SERVICE_STATES);
}

export const MachineInspector = Object.freeze({
  id: "MachineInspector",
  version: CONTRACT_VERSION,
  purpose: "Return a structured observation of the current host while keeping observation separate from authored intent.",
  mutationClass: "read-only",
  operations: Object.freeze({
    inspect: Object.freeze({
      inputType: "MachineInspectorInput",
      outputType: "MachineObservation",
      validateInput: validateMachineInspectionInput,
      validateOutput: validateMachineInspectionOutput,
      deterministic: false,
      idempotent: true,
    }),
  }),
});

function planningPort(id, purpose) {
  return Object.freeze({ id, version: CONTRACT_VERSION, purpose, mutationClass: "locally-mutating", operations: Object.freeze({}) });
}

export const PackageManager = planningPort("PackageManager", "Satisfy package-state differences through a replaceable provider.");
export const ConfigurationManager = planningPort("ConfigurationManager", "Satisfy configuration-state differences through a replaceable provider.");
export const ServiceManager = planningPort("ServiceManager", "Satisfy service-state differences through a replaceable provider.");
