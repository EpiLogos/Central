import { readMachineDeclaration } from "./machine-declaration.js";
import {
  ConfigurationManager,
  invokePort,
  MachineInspector,
  PackageManager,
  ServiceManager,
} from "./ports.js";
import { failure, ResultStatus, success } from "./results.js";
import { resolveCentralRoot } from "./root.js";

const REQUIREMENT_PORTS = Object.freeze({
  package: PackageManager,
  configuration: ConfigurationManager,
  service: ServiceManager,
});

const CAPABILITY_PORTS = new Map([
  MachineInspector,
  PackageManager,
  ConfigurationManager,
  ServiceManager,
].map((port) => [port.id, port]));

function descriptor({ id, title, description, inputs = [], output, requiredPorts = [] }) {
  return {
    id,
    title,
    description,
    inputs,
    output,
    mutationClass: "read-only",
    previewSupported: false,
    requiredPorts,
    availability: { available: true, reason: null },
  };
}

function item(kind, id, intended, observed, extra = {}) {
  return { kind, id, intended, observed, ...extra };
}

function stateIndex(items) {
  return new Map(items.map((entry) => [entry.id, entry]));
}

function publicPort(port) {
  return { id: port.id, version: port.version };
}

async function resolvePort(connectors, port, connectorContext, cache) {
  if (cache.has(port.id)) return cache.get(port.id);
  if (!connectors || typeof connectors.resolve !== "function") {
    const unavailable = {
      connector: null,
      diagnostics: { eligible: [], ineligible: [], selectedConnector: null },
      reason: `Connector registry is unavailable for ${port.id}.`,
    };
    cache.set(port.id, unavailable);
    return unavailable;
  }
  const resolution = await connectors.resolve(port, connectorContext);
  cache.set(port.id, resolution);
  return resolution;
}

function requirementDifferences(declaration, observation) {
  const differences = [];
  const satisfied = [];
  for (const [plural, kind] of [
    ["packages", "package"],
    ["configurations", "configuration"],
    ["services", "service"],
  ]) {
    const observed = stateIndex(observation[plural]);
    for (const requirement of declaration.requirements[plural]) {
      const current = observed.get(requirement.id);
      const entry = item(
        kind,
        requirement.id,
        requirement.state,
        current?.state ?? "missing",
        requirement.source ? { source: requirement.source } : {},
      );
      if (current?.state === requirement.state) satisfied.push(entry);
      else differences.push(entry);
    }
  }
  return { satisfied, differences };
}

export async function buildMachinePlan({ declaration, observation, connectors, connectorContext = {} }) {
  const satisfied = [];
  const missing = [];
  const changeable = [];
  const unsupported = [];
  const cache = new Map();
  const observedCapabilities = new Set(observation.capabilities);

  for (const capability of declaration.capabilities) {
    const entry = item(
      "capability",
      capability,
      "available",
      observedCapabilities.has(capability) ? "available" : "missing",
    );
    if (entry.observed === "available") {
      satisfied.push(entry);
      continue;
    }

    const port = CAPABILITY_PORTS.get(capability);
    if (!port) {
      missing.push(entry);
      unsupported.push({
        ...entry,
        reason: `No published machine Port corresponds to required capability ${capability}.`,
      });
      continue;
    }

    const resolution = await resolvePort(connectors, port, connectorContext, cache);
    if (resolution.connector) {
      satisfied.push({
        ...entry,
        observed: "available",
        via: "connector",
        port: publicPort(port),
        connector: resolution.diagnostics.selectedConnector,
      });
    } else {
      missing.push(entry);
      unsupported.push({
        ...entry,
        port: publicPort(port),
        reason: resolution.reason ?? `No eligible Connector implements ${port.id}.`,
        diagnostics: resolution.diagnostics,
      });
    }
  }

  const requirements = requirementDifferences(declaration, observation);
  satisfied.push(...requirements.satisfied);
  for (const difference of requirements.differences) {
    missing.push(difference);
    const port = REQUIREMENT_PORTS[difference.kind];
    const resolution = await resolvePort(connectors, port, connectorContext, cache);
    if (resolution.connector) {
      changeable.push({
        ...difference,
        port: publicPort(port),
        connector: resolution.diagnostics.selectedConnector,
      });
    } else {
      unsupported.push({
        ...difference,
        port: publicPort(port),
        reason: resolution.reason ?? `No eligible Connector implements ${port.id}.`,
        diagnostics: resolution.diagnostics,
      });
    }
  }

  return {
    role: declaration.role,
    complete: missing.length === 0,
    satisfied,
    missing,
    changeable,
    unsupported,
  };
}

async function inspectMachine(actionId, context) {
  if (!context.connectors || typeof context.connectors.resolve !== "function") {
    return failure(
      actionId,
      ResultStatus.UNAVAILABLE_CAPABILITY,
      `Required Port is unavailable: ${MachineInspector.id}`,
      { port: MachineInspector.id, diagnostics: { eligible: [], ineligible: [], selectedConnector: null } },
    );
  }

  const resolution = await context.connectors.resolve(MachineInspector, context.connectorContext);
  if (!resolution.connector) {
    return failure(
      actionId,
      ResultStatus.UNAVAILABLE_CAPABILITY,
      `No eligible Connector implements ${MachineInspector.id}.`,
      { port: MachineInspector.id, diagnostics: resolution.diagnostics },
    );
  }

  try {
    const observation = await invokePort(resolution, MachineInspector, "inspect", {});
    return success(actionId, {
      observation,
      source: {
        sourceClass: "observed",
        port: { id: MachineInspector.id, version: MachineInspector.version },
        connector: resolution.diagnostics.selectedConnector,
      },
      diagnostics: resolution.diagnostics,
    });
  } catch (error) {
    return failure(
      actionId,
      ResultStatus.CONNECTOR_FAILURE,
      `Connector failed while executing ${MachineInspector.id}.`,
      {
        connector: resolution.connector.manifest.id,
        message: error instanceof Error ? error.message : String(error),
        diagnostics: resolution.diagnostics,
      },
    );
  }
}

function validRole(input) {
  return typeof input.role === "string" && input.role.trim() !== "";
}

export function registerMachineActions(registry) {
  registry.register(
    descriptor({
      id: "machine.inspect",
      title: "Inspect current machine",
      description: "Collect structured current-host Observation data through MachineInspector.",
      output: { type: "machine-observation" },
      requiredPorts: [MachineInspector.id],
    }),
    async (_input, context) => inspectMachine("machine.inspect", context),
  );

  registry.register(
    descriptor({
      id: "machine.plan",
      title: "Plan machine state",
      description: "Compare one authored machine-role declaration with current Observation data without applying changes.",
      inputs: [{ name: "role", type: "string", required: true }],
      output: { type: "machine-plan" },
      requiredPorts: [MachineInspector.id],
    }),
    async (input, context) => {
      if (!validRole(input)) {
        return failure("machine.plan", ResultStatus.INVALID_INPUT, "Machine plan requires a role.");
      }
      const role = input.role.trim();
      const central = resolveCentralRoot(context.rootOptions);
      const authored = await readMachineDeclaration(central.path, role);
      if (!authored.ok) {
        return failure(
          "machine.plan",
          ResultStatus.INVALID_INPUT,
          `Machine declaration for ${role} is unavailable or invalid.`,
          { role, source: authored.source, diagnostics: authored.diagnostics },
        );
      }

      const inspected = await inspectMachine("machine.plan", context);
      if (!inspected.ok) return inspected;

      const plan = await buildMachinePlan({
        declaration: authored.declaration,
        observation: inspected.data.observation,
        connectors: context.connectors,
        connectorContext: context.connectorContext,
      });

      return success("machine.plan", {
        authored: {
          declaration: authored.declaration,
          source: authored.source,
        },
        observed: {
          observation: inspected.data.observation,
          source: inspected.data.source,
          diagnostics: inspected.data.diagnostics,
        },
        plan,
      });
    },
  );

  return registry;
}

function renderPlanItem(entry) {
  const transition = `${entry.observed} → ${entry.intended}`;
  if (entry.connector) return `- ${entry.kind} ${entry.id}: ${transition} via ${entry.port.id} (${entry.connector.id})`;
  if (entry.reason) return `- ${entry.kind} ${entry.id}: ${transition} — ${entry.reason}`;
  return `- ${entry.kind} ${entry.id}: ${transition}`;
}

export function renderMachineInspection({ observation, source }) {
  return [
    `Platform: ${observation.host.platform}`,
    `Architecture: ${observation.host.architecture}`,
    `Observation source: ${source.connector.id}`,
    `Capabilities: ${observation.capabilities.length}`,
    `Packages: ${observation.packages.length}`,
    `Configurations: ${observation.configurations.length}`,
    `Services: ${observation.services.length}`,
  ].join("\n");
}

export function renderMachinePlan({ authored, observed, plan }) {
  return [
    `Machine plan: ${plan.role}`,
    `Authored source: ${authored.source.path}`,
    `Observed via: ${observed.source.connector.id}`,
    `Satisfied: ${plan.satisfied.length}`,
    `Missing: ${plan.missing.length}`,
    `Changeable: ${plan.changeable.length}`,
    ...plan.changeable.map(renderPlanItem),
    `Unsupported: ${plan.unsupported.length}`,
    ...plan.unsupported.map(renderPlanItem),
  ].join("\n");
}
