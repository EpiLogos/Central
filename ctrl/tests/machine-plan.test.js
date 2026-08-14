import assert from "node:assert/strict";
import { mkdir, mkdtemp, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { createCoreActionRegistry } from "../core/actions.js";
import { MACHINE_DECLARATION_API_VERSION } from "../core/machine-declaration.js";
import { ConfigurationManager, MachineInspector, PackageManager, ServiceManager } from "../sdk/index.js";

function resolver(observation, available = [MachineInspector.id]) {
  const availableSet = new Set(available);
  return {
    async resolve(port) {
      if (!availableSet.has(port.id)) {
        return {
          port: port.id,
          connector: null,
          diagnostics: { eligible: [], ineligible: [], selectedConnector: null },
        };
      }
      const selectedConnector = { id: "reference.machine-test", version: "1.0.0" };
      return {
        port: port.id,
        connector: {
          manifest: selectedConnector,
          implementations: {
            [MachineInspector.id]: { async inspect() { return structuredClone(observation); } },
            [PackageManager.id]: {},
            [ConfigurationManager.id]: {},
            [ServiceManager.id]: {},
          },
        },
        diagnostics: { eligible: [selectedConnector], ineligible: [], selectedConnector },
      };
    },
  };
}

async function machineRoot(declaration) {
  const root = await mkdtemp(join(tmpdir(), "central-machine-plan-"));
  const directory = join(root, "Control", "machines");
  await mkdir(directory, { recursive: true });
  await writeFile(join(directory, `${declaration.role}.json`), JSON.stringify(declaration), "utf8");
  return root;
}

function declaration(overrides = {}) {
  return {
    apiVersion: MACHINE_DECLARATION_API_VERSION,
    role: "test-machine",
    capabilities: [MachineInspector.id],
    requirements: { packages: [], configurations: [], services: [] },
    ...overrides,
  };
}

function context(root, observation, available) {
  return {
    rootOptions: { explicitRoot: root, env: {} },
    connectorContext: { platform: "linux" },
    connectors: resolver(observation, available),
  };
}

const emptyObservation = {
  host: { platform: "linux", architecture: "x64" },
  capabilities: [MachineInspector.id],
  packages: [],
  configurations: [],
  services: [],
};

test("MachineInspector is the public read Port used by machine.inspect", () => {
  const registry = createCoreActionRegistry();
  assert.equal(MachineInspector.id, "MachineInspector");
  assert.equal(MachineInspector.version, "1.0.0");
  assert.equal(MachineInspector.mutationClass, "read-only");
  assert.deepEqual(registry.get("machine.inspect").requiredPorts, [MachineInspector.id]);
});

test("machine.inspect returns structured observed state without authored machine source", async () => {
  const intended = declaration();
  const root = await machineRoot(intended);
  const result = await createCoreActionRegistry().execute("machine.inspect", {}, context(root, emptyObservation));
  assert.equal(result.ok, true);
  assert.deepEqual(result.data.observation, emptyObservation);
  assert.equal(result.data.source.sourceClass, "observed");
  assert.equal(result.data.source.port.id, MachineInspector.id);
  assert.equal("declaration" in result.data, false);
});

test("machine.plan keeps authored intent and observed state separate when satisfied", async () => {
  const intended = declaration();
  const root = await machineRoot(intended);
  const result = await createCoreActionRegistry().execute("machine.plan", { role: intended.role }, context(root, emptyObservation));
  assert.equal(result.ok, true);
  assert.equal(result.data.authored.source.sourceClass, "authored");
  assert.deepEqual(result.data.authored.declaration, intended);
  assert.equal(result.data.observed.source.sourceClass, "observed");
  assert.deepEqual(result.data.observed.observation, emptyObservation);
  assert.equal(result.data.plan.complete, true);
  assert.deepEqual(result.data.plan.missing, []);
  assert.deepEqual(result.data.plan.changeable, []);
  assert.deepEqual(result.data.plan.unsupported, []);
});

test("machine.plan classifies several differences as changeable and names Port plus Connector", async () => {
  const intended = declaration({
    capabilities: [MachineInspector.id, PackageManager.id, ConfigurationManager.id, ServiceManager.id],
    requirements: {
      packages: [{ id: "node", state: "present" }],
      configurations: [{ id: "shell", state: "present" }],
      services: [{ id: "ssh", state: "enabled" }],
    },
  });
  const observation = {
    ...emptyObservation,
    packages: [{ id: "node", state: "absent" }],
    configurations: [{ id: "shell", state: "absent" }],
    services: [{ id: "ssh", state: "stopped" }],
  };
  const root = await machineRoot(intended);
  const available = [MachineInspector.id, PackageManager.id, ConfigurationManager.id, ServiceManager.id];
  const result = await createCoreActionRegistry().execute("machine.plan", { role: intended.role }, context(root, observation, available));
  assert.equal(result.ok, true);
  assert.equal(result.data.plan.complete, false);
  assert.deepEqual(result.data.plan.missing.filter((entry) => entry.kind !== "capability").map((entry) => entry.id), ["node", "shell", "ssh"]);
  assert.deepEqual(result.data.plan.changeable.map((entry) => entry.port.id), [PackageManager.id, ConfigurationManager.id, ServiceManager.id]);
  assert.equal(result.data.plan.changeable.every((entry) => entry.connector.id === "reference.machine-test"), true);
  assert.deepEqual(result.data.plan.unsupported, []);
});

test("machine.plan explains unsupported capability and unavailable change Port", async () => {
  const intended = declaration({
    capabilities: [MachineInspector.id, "NativeAutomation"],
    requirements: { packages: [{ id: "node", state: "present" }], configurations: [], services: [] },
  });
  const observation = { ...emptyObservation, packages: [{ id: "node", state: "absent" }] };
  const root = await machineRoot(intended);
  const result = await createCoreActionRegistry().execute("machine.plan", { role: intended.role }, context(root, observation));
  assert.equal(result.ok, true);
  assert.equal(result.data.plan.missing.length, 2);
  const capability = result.data.plan.unsupported.find((entry) => entry.kind === "capability");
  const packageDifference = result.data.plan.unsupported.find((entry) => entry.kind === "package");
  assert.match(capability.reason, /No published machine Port corresponds/);
  assert.equal(packageDifference.port.id, PackageManager.id);
  assert.match(packageDifference.reason, /No eligible Connector implements PackageManager/);
  assert.equal(packageDifference.diagnostics.selectedConnector, null);
});
