import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  CONNECTOR_API_VERSION,
  runWorkDiscoveryConformance,
  validateConnectorManifest,
  WorkDiscovery,
} from "../sdk/index.js";
import {
  createFilesystemWorkDiscoveryConnector,
  createStaticWorkDiscoveryConnector,
} from "../../connectors/reference/work-discovery.js";
import { createWorkDiscoveryConnectorTemplate } from "../../connectors/template/work-discovery.js";
import { ConnectorRegistry } from "../core/connectors.js";
import { createCoreActionRegistry } from "../core/actions.js";
import { initializeCentral } from "../core/root.js";

async function fixtureRoot() {
  const base = await mkdtemp(join(tmpdir(), "central-sdk-"));
  const root = join(base, "Central");
  await initializeCentral(root);
  await mkdir(join(root, "Work", "alpha"));
  await mkdir(join(root, "Work", "beta"));
  return root;
}

test("public WorkDiscovery contract exposes compatibility identity and typed operation metadata", () => {
  assert.equal(WorkDiscovery.id, "WorkDiscovery");
  assert.equal(WorkDiscovery.version, "1.0.0");
  assert.equal(WorkDiscovery.operations.list.inputType, "WorkDiscoveryListInput");
  assert.equal(WorkDiscovery.operations.list.outputType, "WorkDiscoveryListOutput");
  assert.equal(WorkDiscovery.operations.list.deterministic, true);
});

test("public manifest validation describes extension and Port compatibility", () => {
  const connector = createFilesystemWorkDiscoveryConnector();
  assert.equal(connector.manifest.apiVersion, CONNECTOR_API_VERSION);
  assert.deepEqual(connector.manifest.ports, [{ id: WorkDiscovery.id, version: WorkDiscovery.version }]);
  assert.equal(validateConnectorManifest(connector.manifest), connector.manifest);
});

test("shared WorkDiscovery conformance suite passes for both reference implementations", async () => {
  const root = await fixtureRoot();
  const filesystem = await runWorkDiscoveryConformance(createFilesystemWorkDiscoveryConnector(), {
    workRoot: join(root, "Work"),
    expectedNames: ["alpha", "beta"],
  });
  const staticConnector = await runWorkDiscoveryConformance(createStaticWorkDiscoveryConnector([
    { name: "alpha", path: "/work/alpha" },
    { name: "beta", path: "/work/beta" },
  ]), {
    workRoot: join(root, "Work"),
    expectedNames: ["alpha", "beta"],
  });
  assert.equal(filesystem.ok, true);
  assert.equal(staticConnector.ok, true);
  assert.deepEqual(filesystem.checks, ["manifest", "port-compatibility", "probe", "typed-output", "repeat-stability"]);
});

test("minimal Connector template can satisfy the published Port without core changes", async () => {
  const connector = createWorkDiscoveryConnectorTemplate({
    id: "example.work-discovery",
    displayName: "Example Work discovery",
    listWorkItems: async ({ workRoot }) => ({ items: [{ name: "example", path: join(workRoot, "example") }] }),
  });
  const report = await runWorkDiscoveryConformance(connector, {
    workRoot: "/tmp/work",
    expectedNames: ["example"],
  });
  assert.equal(report.ok, true);
});

test("core Action accepts a Connector defined only through the public SDK", async () => {
  const root = await fixtureRoot();
  const connector = createWorkDiscoveryConnectorTemplate({
    id: "external.work-discovery",
    displayName: "External Work discovery",
    listWorkItems: async ({ workRoot }) => ({ items: [{ name: "external", path: join(workRoot, "external") }] }),
  });
  const connectors = new ConnectorRegistry().register(connector);
  const result = await createCoreActionRegistry().execute("work.list", {}, {
    rootOptions: { explicitRoot: root, env: {} },
    connectors,
  });
  assert.equal(result.ok, true);
  assert.equal(result.data.items[0].name, "external");
  assert.equal(result.data.diagnostics.selectedConnector.id, "external.work-discovery");
});

test("incompatible Port contract version is ineligible with an explicit diagnostic", async () => {
  const connector = createWorkDiscoveryConnectorTemplate({
    id: "old.work-discovery",
    displayName: "Old Work discovery",
    listWorkItems: async () => ({ items: [] }),
  });
  const incompatible = {
    ...connector,
    manifest: {
      ...connector.manifest,
      ports: [{ id: WorkDiscovery.id, version: "0.9.0" }],
    },
  };
  const resolution = await new ConnectorRegistry().register(incompatible).resolve(WorkDiscovery);
  assert.equal(resolution.connector, null);
  assert.match(resolution.diagnostics.ineligible[0].reason, /incompatible WorkDiscovery contract/);
});

test("Connector authoring Skill preserves the public boundary and names the executable reference proof", async () => {
  const skill = await readFile(new URL("../../skills/connector-authoring/SKILL.md", import.meta.url), "utf8");
  assert.match(skill, /Port contract as authoritative/);
  assert.match(skill, /Action → Port → Connector → target/);
  assert.match(skill, /public SDK/);
  assert.match(skill, /conformance/);
  assert.match(skill, /Failure discipline/);
  assert.match(skill, /createWorkDiscoveryConnectorTemplate/);
  assert.match(skill, /canonical `work\.list`/);
});
