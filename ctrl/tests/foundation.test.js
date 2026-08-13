import assert from "node:assert/strict";
import { mkdtemp, readdir, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawn } from "node:child_process";
import test from "node:test";

import { ActionRegistry, createCoreActionRegistry } from "../core/actions.js";
import { inspectCentral, initializeCentral, REQUIRED_CENTRAL_DIRECTORIES, resolveCentralRoot } from "../core/root.js";
import { ResultStatus, success } from "../core/results.js";

const CLI = new URL("../bin/ctrl.js", import.meta.url);

async function temporaryDirectory(prefix) { return mkdtemp(join(tmpdir(), prefix)); }

function runProcess(args, env = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [CLI.pathname, ...args], {
      env: { ...process.env, ...env },
      stdio: ["ignore", "pipe", "pipe"],
    });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk) => { stdout += chunk; });
    child.stderr.on("data", (chunk) => { stderr += chunk; });
    child.on("error", reject);
    child.on("close", (code) => resolve({ code, stdout, stderr }));
  });
}

test("root discovery prefers explicit override, then CENTRAL_ROOT, then HOME/Central", async () => {
  const base = await temporaryDirectory("central-root-");
  const explicit = join(base, "explicit");
  const configured = join(base, "configured");
  const home = join(base, "home");
  assert.deepEqual(resolveCentralRoot({ explicitRoot: explicit, env: { CENTRAL_ROOT: configured }, home }), { path: explicit, source: "explicit" });
  assert.deepEqual(resolveCentralRoot({ env: { CENTRAL_ROOT: configured }, home }), { path: configured, source: "environment" });
  assert.deepEqual(resolveCentralRoot({ env: {}, home }), { path: join(home, "Central"), source: "default" });
});

test("initialization creates only the required roots and is safe to repeat", async () => {
  const root = join(await temporaryDirectory("central-init-"), "Central");
  await initializeCentral(root);
  await initializeCentral(root);
  const report = await inspectCentral(root);
  assert.equal(report.valid, true);
  assert.deepEqual(report.checks.map((check) => check.path), [...REQUIRED_CENTRAL_DIRECTORIES]);
  for (const controlRoot of ["user", "agents", "machines"]) assert.deepEqual(await readdir(join(root, "Control", controlRoot)), []);
});

test("doctor detects an invalid structure and accepts an initialized structure", async () => {
  const root = join(await temporaryDirectory("central-doctor-"), "Central");
  const registry = createCoreActionRegistry();
  const before = await registry.execute("central.doctor", {}, { rootOptions: { explicitRoot: root, env: {} } });
  assert.equal(before.ok, false);
  assert.equal(before.status, ResultStatus.INVALID_CENTRAL_STRUCTURE);
  await registry.execute("central.init", {}, { rootOptions: { explicitRoot: root, env: {} } });
  const after = await registry.execute("central.doctor", {}, { rootOptions: { explicitRoot: root, env: {} } });
  assert.equal(after.ok, true);
  assert.equal(after.data.valid, true);
});

test("init reports an existing non-directory root as invalid Central structure", async () => {
  const base = await temporaryDirectory("central-file-root-");
  const root = join(base, "Central");
  await writeFile(root, "not a directory\n");
  const result = await createCoreActionRegistry().execute("central.init", {}, { rootOptions: { explicitRoot: root, env: {} } });
  assert.equal(result.ok, false);
  assert.equal(result.status, ResultStatus.INVALID_CENTRAL_STRUCTURE);
  assert.equal(result.error.details.rootState, "not_directory");
});

test("Action registry preserves stable canonical IDs and complete descriptors", () => {
  const actions = createCoreActionRegistry().list();
  assert.deepEqual(actions.map((action) => action.id), ["action.list", "central.doctor", "central.init", "central.root", "work.list"]);
  for (const action of actions) {
    assert.equal(typeof action.title, "string");
    assert.equal(typeof action.description, "string");
    assert.ok(Array.isArray(action.inputs));
    assert.equal(typeof action.output, "object");
    assert.ok(["read-only", "locally-mutating", "externally-mutating"].includes(action.mutationClass));
    assert.equal(typeof action.previewSupported, "boolean");
    assert.ok(Array.isArray(action.requiredPorts));
    assert.equal(typeof action.availability.available, "boolean");
  }
});

test("action.list returns the same registry descriptors through a structured Action result", async () => {
  const registry = createCoreActionRegistry();
  const result = await registry.execute("action.list");
  assert.equal(result.ok, true);
  assert.equal(result.status, ResultStatus.SUCCESS);
  assert.deepEqual(result.data.actions, registry.list());
});

test("registry converts unexpected executor exceptions into structured internal failures", async () => {
  const registry = new ActionRegistry();
  registry.register({
    id: "test.fail", title: "Fail", description: "Test failure conversion.", inputs: [], output: { type: "test" },
    mutationClass: "read-only", previewSupported: false, requiredPorts: [], availability: { available: true, reason: null },
  }, async () => { throw new Error("boom"); });
  const result = await registry.execute("test.fail");
  assert.equal(result.ok, false);
  assert.equal(result.status, ResultStatus.INTERNAL_FAILURE);
  assert.equal(result.error.details.message, "boom");
});

test("CLI provides structured root and Action-list output", async () => {
  const root = join(await temporaryDirectory("central-cli-"), "Central");
  const rootResult = await runProcess(["--json", "--root", root, "root"]);
  assert.equal(rootResult.code, 0);
  assert.equal(rootResult.stderr, "");
  const rootPayload = JSON.parse(rootResult.stdout);
  assert.equal(rootPayload.ok, true);
  assert.equal(rootPayload.action, "central.root");
  assert.equal(rootPayload.data.path, root);
  const listResult = await runProcess(["actions"]);
  assert.equal(listResult.code, 0);
  assert.match(listResult.stdout, /action\.list/);
  assert.match(listResult.stdout, /central\.doctor/);
});

test("CLI structured failures distinguish invalid input and invalid Central structure", async () => {
  const invalidInput = await runProcess(["--json", "no-such-command"]);
  assert.equal(invalidInput.code, 2);
  assert.equal(JSON.parse(invalidInput.stdout).status, ResultStatus.INVALID_INPUT);
  const root = join(await temporaryDirectory("central-invalid-"), "Central");
  const invalidStructure = await runProcess(["--json", "--root", root, "doctor"]);
  assert.equal(invalidStructure.code, 3);
  const payload = JSON.parse(invalidStructure.stdout);
  assert.equal(payload.status, ResultStatus.INVALID_CENTRAL_STRUCTURE);
  assert.equal(payload.error.details.valid, false);
});

test("direct filesystem changes remain ordinary and visible to doctor", async () => {
  const root = join(await temporaryDirectory("central-direct-"), "Central");
  await initializeCentral(root);
  await writeFile(join(root, "Control", "user", "notes.md"), "human-authored\n");
  assert.equal((await inspectCentral(root)).valid, true);
});

test("ActionRegistry can register independent Actions without changing registry internals", async () => {
  const registry = new ActionRegistry();
  registry.register({
    id: "example.read", title: "Example", description: "Example extension point for later core Actions.", inputs: [], output: { type: "example" },
    mutationClass: "read-only", previewSupported: false, requiredPorts: [], availability: { available: true, reason: null },
  }, async () => success("example.read", { value: 1 }));
  assert.equal((await registry.execute("example.read")).data.value, 1);
});

test("first Port slice resolves two valid reference Connectors deterministically", async () => {
  const { ConnectorRegistry } = await import("../core/connectors.js");
  const { WorkDiscovery } = await import("../core/ports.js");
  const { createFilesystemWorkDiscoveryConnector, createStaticWorkDiscoveryConnector } = await import("../../connectors/reference/work-discovery.js");
  const first = new ConnectorRegistry().register(createStaticWorkDiscoveryConnector([])).register(createFilesystemWorkDiscoveryConnector());
  const second = new ConnectorRegistry().register(createFilesystemWorkDiscoveryConnector()).register(createStaticWorkDiscoveryConnector([]));
  const a = await first.resolve(WorkDiscovery, { platform: "linux" });
  const b = await second.resolve(WorkDiscovery, { platform: "linux" });
  assert.deepEqual(a.diagnostics.eligible.map((item) => item.id), ["reference.work-filesystem", "reference.work-static"]);
  assert.equal(a.diagnostics.selectedConnector.id, "reference.work-filesystem");
  assert.equal(b.diagnostics.selectedConnector.id, "reference.work-filesystem");
});

test("work.list depends on WorkDiscovery and reports selected Connector diagnostics", async () => {
  const { ConnectorRegistry } = await import("../core/connectors.js");
  const { createStaticWorkDiscoveryConnector } = await import("../../connectors/reference/work-discovery.js");
  const root = join(await temporaryDirectory("central-work-list-"), "Central");
  await initializeCentral(root);
  const connectors = new ConnectorRegistry().register(createStaticWorkDiscoveryConnector([
    { name: "beta", path: join(root, "Work", "beta") }, { name: "alpha", path: join(root, "Work", "alpha") },
  ]));
  const registry = createCoreActionRegistry();
  assert.deepEqual(registry.get("work.list").requiredPorts, ["WorkDiscovery"]);
  const result = await registry.execute("work.list", {}, { rootOptions: { explicitRoot: root, env: {} }, connectors });
  assert.equal(result.ok, true);
  assert.deepEqual(result.data.items.map((item) => item.name), ["alpha", "beta"]);
  assert.equal(result.data.diagnostics.selectedConnector.id, "reference.work-static");
});

test("work.list returns unavailable_capability when WorkDiscovery has no eligible Connector", async () => {
  const { ConnectorRegistry } = await import("../core/connectors.js");
  const root = join(await temporaryDirectory("central-work-unavailable-"), "Central");
  await initializeCentral(root);
  const result = await createCoreActionRegistry().execute("work.list", {}, {
    rootOptions: { explicitRoot: root, env: {} }, connectors: new ConnectorRegistry(),
  });
  assert.equal(result.ok, false);
  assert.equal(result.status, ResultStatus.UNAVAILABLE_CAPABILITY);
  assert.equal(result.error.details.port, "WorkDiscovery");
  assert.equal(result.error.details.diagnostics.selectedConnector, null);
});

test("CLI work.list uses the default reference Connector and structured result path", async () => {
  const root = join(await temporaryDirectory("central-work-cli-"), "Central");
  await initializeCentral(root);
  const { mkdir } = await import("node:fs/promises");
  await mkdir(join(root, "Work", "zeta"));
  await mkdir(join(root, "Work", "alpha"));
  const result = await runProcess(["--json", "--root", root, "work.list"]);
  assert.equal(result.code, 0);
  const payload = JSON.parse(result.stdout);
  assert.equal(payload.action, "work.list");
  assert.deepEqual(payload.data.items.map((item) => item.name), ["alpha", "zeta"]);
  assert.equal(payload.data.diagnostics.selectedConnector.id, "reference.work-filesystem");
});
