import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, mkdir, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { MACHINE_DECLARATION_API_VERSION, readMachineDeclaration, renderMachineDeclaration, validateMachineDeclaration } from "../core/machine-declaration.js";
import { runCli } from "../core/cli.js";

const workstation = {
  apiVersion: MACHINE_DECLARATION_API_VERSION,
  role: "primary-workstation",
  capabilities: ["NativeOpen", "Automation", "PackageManager", "ConfigurationManager"],
  requirements: {
    packages: [{ id: "git", state: "present" }, { id: "node", state: "present" }],
    configurations: [{ id: "shell", state: "present", source: { kind: "control", ref: "machines/config/shell" } }],
    services: [{ id: "ssh-agent", state: "running" }],
  },
};

const server = {
  apiVersion: MACHINE_DECLARATION_API_VERSION,
  role: "home-server",
  capabilities: ["MachineInspector", "PackageManager", "ConfigurationManager"],
  requirements: {
    packages: [{ id: "git", state: "present" }],
    configurations: [{ id: "server-base", state: "present", source: { kind: "path", ref: "config/server" } }],
    services: [{ id: "ssh", state: "enabled" }],
  },
};

async function writeDeclaration(role, value) {
  const root = await mkdtemp(join(tmpdir(), "central-machine-"));
  const directory = join(root, "Control", "machines");
  await mkdir(directory, { recursive: true });
  await writeFile(join(directory, `${role}.json`), typeof value === "string" ? value : JSON.stringify(value), "utf8");
  return root;
}

test("workstation declaration captures role, capabilities, package, configuration, and service intent", () => {
  assert.deepEqual(validateMachineDeclaration(workstation), { valid: true, errors: [] });
  assert.equal("provider" in workstation.requirements.packages[0], false);
});

test("server declaration uses the same provider-neutral versioned schema", () => {
  assert.deepEqual(validateMachineDeclaration(server), { valid: true, errors: [] });
  assert.equal(server.apiVersion, workstation.apiVersion);
});

test("authored machine declaration is read from Control and retains source provenance", async () => {
  const root = await writeDeclaration(workstation.role, workstation);
  const loaded = await readMachineDeclaration(root, workstation.role);
  assert.equal(loaded.ok, true);
  assert.deepEqual(loaded.declaration, workstation);
  assert.equal(loaded.source.sourceClass, "authored");
  assert.match(loaded.source.path, /Control\/machines\/primary-workstation\.json$/);
});

test("invalid JSON returns precise diagnostics instead of becoming machine intent", async () => {
  const root = await writeDeclaration("home-server", "{ nope");
  const loaded = await readMachineDeclaration(root, "home-server");
  assert.equal(loaded.ok, false);
  assert.equal(loaded.diagnostics[0].code, "invalid_json");
});

test("invalid declaration reports structural diagnostics", async () => {
  const invalid = structuredClone(server);
  invalid.apiVersion = "central.machine/v99";
  invalid.requirements.services[0].state = "magic";
  const root = await writeDeclaration(invalid.role, invalid);
  const loaded = await readMachineDeclaration(root, invalid.role);
  assert.equal(loaded.ok, false);
  assert.deepEqual(loaded.diagnostics.map(({ code }) => code), ["unsupported_version", "invalid_state"]);
});

test("human explanation makes role and intended requirements inspectable", () => {
  const output = renderMachineDeclaration({ declaration: workstation, source: { path: "/Central/Control/machines/primary-workstation.json" } });
  assert.match(output, /Role: primary-workstation/);
  assert.match(output, /Schema: central\.machine\/v1/);
  assert.match(output, /- PackageManager/);
  assert.match(output, /- git: present/);
  assert.match(output, /- shell: present \(control: machines\/config\/shell\)/);
  assert.match(output, /- ssh-agent: running/);
});

test("ctrl explains a machine declaration through the canonical machine.declaration Action", async () => {
  const root = await writeDeclaration(workstation.role, workstation);
  const { result, output, exitCode } = await runCli(["--root", root, "machine", "declaration", workstation.role]);
  assert.equal(exitCode, 0);
  assert.equal(result.action, "machine.declaration");
  assert.equal(result.data.source.sourceClass, "authored");
  assert.match(output, /Role: primary-workstation/);
  assert.match(output, /Packages:/);
});

test("ctrl exposes the same declaration as structured output", async () => {
  const root = await writeDeclaration(server.role, server);
  const { output, exitCode } = await runCli(["--json", "--root", root, "machine.declaration", server.role]);
  assert.equal(exitCode, 0);
  const result = JSON.parse(output);
  assert.equal(result.action, "machine.declaration");
  assert.deepEqual(result.data.declaration, server);
  assert.equal(result.data.source.sourceClass, "authored");
});
