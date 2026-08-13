import { inspectCentral, initializeCentral, resolveCentralRoot } from "./root.js";
import { failure, ResultStatus, success } from "./results.js";
import { invokePort, WorkDiscovery } from "./ports.js";
import { join } from "node:path";
import { CONTROL_ROOTS, locateControlRoot, searchControl } from "./control-source.js";
import { readMachineDeclaration } from "./machine-declaration.js";

const REQUIRED_DESCRIPTOR_FIELDS = Object.freeze([
  "id",
  "title",
  "description",
  "inputs",
  "output",
  "mutationClass",
  "previewSupported",
  "requiredPorts",
  "availability",
]);

function validateDescriptor(descriptor) {
  if (!descriptor || typeof descriptor !== "object") {
    throw new TypeError("Action descriptor must be an object.");
  }
  for (const field of REQUIRED_DESCRIPTOR_FIELDS) {
    if (!(field in descriptor)) {
      throw new TypeError(`Action descriptor is missing ${field}.`);
    }
  }
  if (!/^[a-z][a-z0-9-]*\.[a-z][a-z0-9-]*$/.test(descriptor.id)) {
    throw new TypeError(`Invalid Action id: ${descriptor.id}`);
  }
}

export class ActionRegistry {
  #actions = new Map();

  register(descriptor, execute) {
    validateDescriptor(descriptor);
    if (typeof execute !== "function") {
      throw new TypeError(`Action ${descriptor.id} must provide an executor.`);
    }
    if (this.#actions.has(descriptor.id)) {
      throw new TypeError(`Action already registered: ${descriptor.id}`);
    }
    this.#actions.set(descriptor.id, { descriptor: Object.freeze(descriptor), execute });
    return this;
  }

  get(id) {
    return this.#actions.get(id)?.descriptor;
  }

  list() {
    return [...this.#actions.values()]
      .map(({ descriptor }) => descriptor)
      .sort((a, b) => a.id.localeCompare(b.id));
  }

  async execute(id, input = {}, context = {}) {
    const action = this.#actions.get(id);
    if (!action) {
      return failure(id, ResultStatus.INVALID_INPUT, `Unknown Action: ${id}`);
    }
    try {
      return await action.execute(input, context);
    } catch (error) {
      return failure(id, ResultStatus.INTERNAL_FAILURE, "Action execution failed unexpectedly.", {
        message: error instanceof Error ? error.message : String(error),
      });
    }
  }
}

function descriptor({ id, title, description, mutationClass, output, requiredPorts = [], inputs = [] }) {
  return {
    id,
    title,
    description,
    inputs,
    output,
    mutationClass,
    previewSupported: false,
    requiredPorts,
    availability: { available: true, reason: null },
  };
}

async function discoverWork(actionId, context) {
  const resolvedRoot = resolveCentralRoot(context.rootOptions);
  if (!context.connectors || typeof context.connectors.resolve !== "function") {
    return failure(
      actionId,
      ResultStatus.UNAVAILABLE_CAPABILITY,
      `Required Port is unavailable: ${WorkDiscovery.id}`,
      { port: WorkDiscovery.id, diagnostics: { eligible: [], ineligible: [], selectedConnector: null } },
    );
  }
  const resolution = await context.connectors.resolve(WorkDiscovery, context.connectorContext);
  if (!resolution.connector) {
    return failure(
      actionId,
      ResultStatus.UNAVAILABLE_CAPABILITY,
      `No eligible Connector implements ${WorkDiscovery.id}.`,
      { port: WorkDiscovery.id, diagnostics: resolution.diagnostics },
    );
  }
  try {
    const output = await invokePort(resolution, WorkDiscovery, "list", { workRoot: join(resolvedRoot.path, "Work") });
    return success(actionId, {
      ...output,
      root: resolvedRoot.path,
      diagnostics: resolution.diagnostics,
    });
  } catch (error) {
    return failure(
      actionId,
      ResultStatus.CONNECTOR_FAILURE,
      `Connector failed while executing ${WorkDiscovery.id}.`,
      {
        connector: resolution.connector.manifest.id,
        message: error instanceof Error ? error.message : String(error),
        diagnostics: resolution.diagnostics,
      },
    );
  }
}

function workMatches(items, query) {
  const needle = query.trim().toLocaleLowerCase();
  return items.filter((item) => item.name.toLocaleLowerCase().includes(needle));
}

export function createCoreActionRegistry() {
  const registry = new ActionRegistry();

  registry.register(
    descriptor({
      id: "central.root",
      title: "Show Central root",
      description: "Resolve the active Central root.",
      mutationClass: "read-only",
      output: { type: "central-root" },
    }),
    async (_input, context) => {
      const resolved = resolveCentralRoot(context.rootOptions);
      return success("central.root", resolved);
    },
  );

  registry.register(
    descriptor({
      id: "central.init",
      title: "Initialize Central",
      description: "Create the required Central root structure without adding a schema below Control roots.",
      mutationClass: "locally-mutating",
      output: { type: "central-initialization" },
    }),
    async (_input, context) => {
      const resolved = resolveCentralRoot(context.rootOptions);
      const current = await inspectCentral(resolved.path);
      if (current.rootState === "not_directory") {
        return failure(
          "central.init",
          ResultStatus.INVALID_CENTRAL_STRUCTURE,
          "Central root exists but is not a directory.",
          { ...current, rootSource: resolved.source },
        );
      }
      const initialized = await initializeCentral(resolved.path);
      return success("central.init", { ...initialized, rootSource: resolved.source });
    },
  );

  registry.register(
    descriptor({
      id: "central.doctor",
      title: "Diagnose Central",
      description: "Check the validity of the basic Central filesystem structure.",
      mutationClass: "read-only",
      output: { type: "central-health" },
    }),
    async (_input, context) => {
      const resolved = resolveCentralRoot(context.rootOptions);
      const report = await inspectCentral(resolved.path);
      if (!report.valid) {
        return failure(
          "central.doctor",
          ResultStatus.INVALID_CENTRAL_STRUCTURE,
          "Central structure is incomplete or invalid.",
          { ...report, rootSource: resolved.source },
        );
      }
      return success("central.doctor", { ...report, rootSource: resolved.source });
    },
  );

  registry.register(
    descriptor({
      id: "action.list",
      title: "List Actions",
      description: "List canonical Action descriptors.",
      mutationClass: "read-only",
      output: { type: "action-descriptor-list" },
    }),
    async () => success("action.list", { actions: registry.list() }),
  );

  registry.register(
    descriptor({
      id: "control.open",
      title: "Locate Control source root",
      description: "Resolve one stable authored Control source root.",
      mutationClass: "read-only",
      inputs: [{ name: "target", type: "string", required: true, choices: [...CONTROL_ROOTS] }],
      output: { type: "control-source-root" },
    }),
    async (input, context) => {
      if (!CONTROL_ROOTS.includes(input.target)) {
        return failure("control.open", ResultStatus.INVALID_INPUT, `Control root must be one of: ${CONTROL_ROOTS.join(", ")}.`);
      }
      const central = resolveCentralRoot(context.rootOptions);
      const source = await locateControlRoot(central.path, input.target);
      if (!source.exists) {
        return failure("control.open", ResultStatus.INVALID_CENTRAL_STRUCTURE, `Control/${input.target} is missing.`, source);
      }
      return success("control.open", source);
    },
  );

  registry.register(
    descriptor({
      id: "control.search",
      title: "Search Control source",
      description: "Search ordinary authored text below the three stable Control roots.",
      mutationClass: "read-only",
      inputs: [{ name: "query", type: "string", required: true }],
      output: { type: "control-source-search" },
    }),
    async (input, context) => {
      if (typeof input.query !== "string" || input.query.trim() === "") {
        return failure("control.search", ResultStatus.INVALID_INPUT, "Control search requires a non-empty query.");
      }
      const central = resolveCentralRoot(context.rootOptions);
      const result = await searchControl(central.path, input.query);
      return success("control.search", result);
    },
  );

  registry.register(
    descriptor({
      id: "work.list",
      title: "List Work items",
      description: "Discover ordinary directories in the active Central Work root.",
      mutationClass: "read-only",
      output: { type: "work-item-list" },
      requiredPorts: [WorkDiscovery.id],
    }),
    async (_input, context) => discoverWork("work.list", context),
  );

  registry.register(
    descriptor({
      id: "work.search",
      title: "Search Work items",
      description: "Search ordinary Work directory names through WorkDiscovery.",
      mutationClass: "read-only",
      inputs: [{ name: "query", type: "string", required: true }],
      output: { type: "work-item-search" },
      requiredPorts: [WorkDiscovery.id],
    }),
    async (input, context) => {
      if (typeof input.query !== "string" || input.query.trim() === "") {
        return failure("work.search", ResultStatus.INVALID_INPUT, "Work search requires a non-empty query.");
      }
      const discovered = await discoverWork("work.search", context);
      if (!discovered.ok) return discovered;
      return success("work.search", {
        query: input.query.trim(),
        matches: workMatches(discovered.data.items, input.query),
        root: discovered.data.root,
        diagnostics: discovered.data.diagnostics,
      });
    },
  );

  registry.register(
    descriptor({
      id: "work.open",
      title: "Enter Work item",
      description: "Resolve one ordinary Work directory by exact name or unambiguous search.",
      mutationClass: "read-only",
      inputs: [{
        name: "query",
        type: "string",
        required: true,
        selectableSource: { action: "work.list", collection: "items", valueField: "name" },
      }],
      output: { type: "work-item-selection" },
      requiredPorts: [WorkDiscovery.id],
    }),
    async (input, context) => {
      if (typeof input.query !== "string" || input.query.trim() === "") {
        return failure("work.open", ResultStatus.INVALID_INPUT, "Work entry requires a non-empty name or search.");
      }
      const discovered = await discoverWork("work.open", context);
      if (!discovered.ok) return discovered;
      const query = input.query.trim();
      const normalized = query.toLocaleLowerCase();
      const exact = discovered.data.items.find((item) => item.name.toLocaleLowerCase() === normalized);
      const matches = exact ? [exact] : workMatches(discovered.data.items, query);
      if (matches.length === 0) {
        return failure("work.open", ResultStatus.INVALID_INPUT, `No Work item matches: ${query}`, { query, matches: [] });
      }
      if (matches.length > 1) {
        return failure("work.open", ResultStatus.INVALID_INPUT, `Work search is ambiguous: ${query}`, { query, matches });
      }
      return success("work.open", {
        query,
        match: exact ? "exact" : "search",
        item: matches[0],
        root: discovered.data.root,
        diagnostics: discovered.data.diagnostics,
      });
    },
  );

  registry.register(
    descriptor({
      id: "machine.declaration",
      title: "Explain machine declaration",
      description: "Read one versioned authored machine-role declaration from Control.",
      mutationClass: "read-only",
      inputs: [{ name: "role", type: "string", required: true }],
      output: { type: "machine-declaration" },
    }),
    async (input, context) => {
      if (typeof input.role !== "string" || input.role.trim() === "") {
        return failure("machine.declaration", ResultStatus.INVALID_INPUT, "Machine declaration requires a role.");
      }
      const central = resolveCentralRoot(context.rootOptions);
      const loaded = await readMachineDeclaration(central.path, input.role.trim());
      if (!loaded.ok) {
        return failure(
          "machine.declaration",
          ResultStatus.INVALID_INPUT,
          `Machine declaration for ${input.role.trim()} is unavailable or invalid.`,
          { role: input.role.trim(), source: loaded.source, diagnostics: loaded.diagnostics },
        );
      }
      return success("machine.declaration", {
        declaration: loaded.declaration,
        source: loaded.source,
      });
    },
  );

  return registry;
}
