import { join } from "node:path";
import { inspectCentral, initializeCentral, resolveCentralRoot } from "./root.js";
import { failure, ResultStatus, success } from "./results.js";
import { invokePort, WorkDiscovery } from "./ports.js";

const REQUIRED_DESCRIPTOR_FIELDS = Object.freeze([
  "id", "title", "description", "inputs", "output", "mutationClass",
  "previewSupported", "requiredPorts", "availability",
]);

function validateDescriptor(descriptor) {
  if (!descriptor || typeof descriptor !== "object") throw new TypeError("Action descriptor must be an object.");
  for (const field of REQUIRED_DESCRIPTOR_FIELDS) {
    if (!(field in descriptor)) throw new TypeError(`Action descriptor is missing ${field}.`);
  }
  if (!/^[a-z][a-z0-9-]*\.[a-z][a-z0-9-]*$/.test(descriptor.id)) {
    throw new TypeError(`Invalid Action id: ${descriptor.id}`);
  }
}

export class ActionRegistry {
  #actions = new Map();

  register(descriptor, execute) {
    validateDescriptor(descriptor);
    if (typeof execute !== "function") throw new TypeError(`Action ${descriptor.id} must provide an executor.`);
    if (this.#actions.has(descriptor.id)) throw new TypeError(`Action already registered: ${descriptor.id}`);
    this.#actions.set(descriptor.id, { descriptor: Object.freeze(descriptor), execute });
    return this;
  }

  get(id) { return this.#actions.get(id)?.descriptor; }

  list() {
    return [...this.#actions.values()].map(({ descriptor }) => descriptor).sort((a, b) => a.id.localeCompare(b.id));
  }

  async execute(id, input = {}, context = {}) {
    const action = this.#actions.get(id);
    if (!action) return failure(id, ResultStatus.INVALID_INPUT, `Unknown Action: ${id}`);
    try { return await action.execute(input, context); }
    catch (error) {
      return failure(id, ResultStatus.INTERNAL_FAILURE, "Action execution failed unexpectedly.", {
        message: error instanceof Error ? error.message : String(error),
      });
    }
  }
}

function descriptor({ id, title, description, mutationClass, output, requiredPorts = [] }) {
  return {
    id,
    title,
    description,
    inputs: [],
    output,
    mutationClass,
    previewSupported: false,
    requiredPorts,
    availability: { available: true, reason: null },
  };
}

export function createCoreActionRegistry() {
  const registry = new ActionRegistry();

  registry.register(descriptor({
    id: "central.root",
    title: "Show Central root",
    description: "Resolve the active Central root.",
    mutationClass: "read-only",
    output: { type: "central-root" },
  }), async (_input, context) => success("central.root", resolveCentralRoot(context.rootOptions)));

  registry.register(descriptor({
    id: "central.init",
    title: "Initialize Central",
    description: "Create the required Central root structure without adding a schema below Control roots.",
    mutationClass: "locally-mutating",
    output: { type: "central-initialization" },
  }), async (_input, context) => {
    const resolved = resolveCentralRoot(context.rootOptions);
    const current = await inspectCentral(resolved.path);
    if (current.rootState === "not_directory") {
      return failure("central.init", ResultStatus.INVALID_CENTRAL_STRUCTURE, "Central root exists but is not a directory.", {
        ...current,
        rootSource: resolved.source,
      });
    }
    const initialized = await initializeCentral(resolved.path);
    return success("central.init", { ...initialized, rootSource: resolved.source });
  });

  registry.register(descriptor({
    id: "central.doctor",
    title: "Diagnose Central",
    description: "Check the validity of the basic Central filesystem structure.",
    mutationClass: "read-only",
    output: { type: "central-health" },
  }), async (_input, context) => {
    const resolved = resolveCentralRoot(context.rootOptions);
    const report = await inspectCentral(resolved.path);
    if (!report.valid) {
      return failure("central.doctor", ResultStatus.INVALID_CENTRAL_STRUCTURE, "Central structure is incomplete or invalid.", {
        ...report,
        rootSource: resolved.source,
      });
    }
    return success("central.doctor", { ...report, rootSource: resolved.source });
  });

  registry.register(descriptor({
    id: "action.list",
    title: "List Actions",
    description: "List canonical Action descriptors.",
    mutationClass: "read-only",
    output: { type: "action-descriptor-list" },
  }), async () => success("action.list", { actions: registry.list() }));

  registry.register(descriptor({
    id: "work.list",
    title: "List Work items",
    description: "Discover ordinary directories in the active Central Work root.",
    mutationClass: "read-only",
    output: { type: "work-item-list" },
    requiredPorts: [WorkDiscovery.id],
  }), async (_input, context) => {
    const resolvedRoot = resolveCentralRoot(context.rootOptions);
    if (!context.connectors || typeof context.connectors.resolve !== "function") {
      return failure("work.list", ResultStatus.UNAVAILABLE_CAPABILITY, `Required Port is unavailable: ${WorkDiscovery.id}`, {
        port: WorkDiscovery.id,
        diagnostics: { eligible: [], ineligible: [], selectedConnector: null },
      });
    }
    const resolution = await context.connectors.resolve(WorkDiscovery, context.connectorContext);
    if (!resolution.connector) {
      return failure("work.list", ResultStatus.UNAVAILABLE_CAPABILITY, `No eligible Connector implements ${WorkDiscovery.id}.`, {
        port: WorkDiscovery.id,
        diagnostics: resolution.diagnostics,
      });
    }
    try {
      const output = await invokePort(resolution, WorkDiscovery, "list", { workRoot: join(resolvedRoot.path, "Work") });
      return success("work.list", { ...output, root: resolvedRoot.path, diagnostics: resolution.diagnostics });
    } catch (error) {
      return failure("work.list", ResultStatus.CONNECTOR_FAILURE, `Connector failed while executing ${WorkDiscovery.id}.`, {
        connector: resolution.connector.manifest.id,
        message: error instanceof Error ? error.message : String(error),
        diagnostics: resolution.diagnostics,
      });
    }
  });

  return registry;
}
