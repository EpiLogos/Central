import assert from "node:assert/strict";
import { mkdir, mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawn } from "node:child_process";
import test from "node:test";

import { createCoreActionRegistry } from "../core/actions.js";
import { createDefaultRuntime } from "../core/runtime.js";
import { initializeCentral } from "../core/root.js";
import { ResultStatus } from "../core/results.js";

const CLI = new URL("../bin/ctrl.js", import.meta.url);

async function fixture(names = ["alpha", "alphabet", "beta"]) {
  const root = join(await mkdtemp(join(tmpdir(), "central-work-entry-")), "Central");
  await initializeCentral(root);
  for (const name of names) await mkdir(join(root, "Work", name));
  return root;
}

function context(root) {
  const runtime = createDefaultRuntime();
  return {
    registry: runtime.actions,
    context: { rootOptions: { explicitRoot: root, env: {} }, connectors: runtime.connectors },
  };
}

function runCli(args) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [CLI.pathname, ...args], { stdio: ["ignore", "pipe", "pipe"] });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk) => { stdout += chunk; });
    child.stderr.on("data", (chunk) => { stderr += chunk; });
    child.on("error", reject);
    child.on("close", (code) => resolve({ code, stdout, stderr }));
  });
}

test("work.list, work.search, and work.open complete the ordinary-directory entry path", async () => {
  const root = await fixture();
  const { registry, context: actionContext } = context(root);

  const listed = await registry.execute("work.list", {}, actionContext);
  assert.equal(listed.ok, true);
  assert.deepEqual(listed.data.items.map((item) => item.name), ["alpha", "alphabet", "beta"]);

  const searched = await registry.execute("work.search", { query: "bet" }, actionContext);
  assert.equal(searched.ok, true);
  assert.deepEqual(searched.data.matches.map((item) => item.name), ["alphabet", "beta"]);

  const opened = await registry.execute("work.open", { query: "beta" }, actionContext);
  assert.equal(opened.ok, true);
  assert.equal(opened.data.match, "exact");
  assert.deepEqual(opened.data.item, { name: "beta", path: join(root, "Work", "beta") });
});

test("work.open accepts an unambiguous search and prefers exact name over broader matches", async () => {
  const root = await fixture(["alpha", "alpha-notes", "project-beta"]);
  const { registry, context: actionContext } = context(root);

  const exact = await registry.execute("work.open", { query: "alpha" }, actionContext);
  assert.equal(exact.ok, true);
  assert.equal(exact.data.item.name, "alpha");
  assert.equal(exact.data.match, "exact");

  const search = await registry.execute("work.open", { query: "beta" }, actionContext);
  assert.equal(search.ok, true);
  assert.equal(search.data.item.name, "project-beta");
  assert.equal(search.data.match, "search");
});

test("ambiguous or absent Work searches return structured selection failures", async () => {
  const root = await fixture();
  const { registry, context: actionContext } = context(root);

  const ambiguous = await registry.execute("work.open", { query: "alp" }, actionContext);
  assert.equal(ambiguous.ok, false);
  assert.equal(ambiguous.status, ResultStatus.INVALID_INPUT);
  assert.deepEqual(ambiguous.error.details.matches.map((item) => item.name), ["alpha", "alphabet"]);

  const absent = await registry.execute("work.open", { query: "gamma" }, actionContext);
  assert.equal(absent.ok, false);
  assert.equal(absent.status, ResultStatus.INVALID_INPUT);
  assert.deepEqual(absent.error.details.matches, []);
});

test("work.open descriptor declares WorkDiscovery as its selectable source", () => {
  const descriptor = createCoreActionRegistry().get("work.open");
  assert.deepEqual(descriptor.requiredPorts, ["WorkDiscovery"]);
  assert.deepEqual(descriptor.inputs[0].selectableSource, {
    port: "WorkDiscovery",
    operation: "list",
    valueField: "name",
  });
});

test("CLI user path can discover, search, and select ordinary Work directories", async () => {
  const root = await fixture(["alpha", "beta"]);

  const listed = await runCli(["--json", "--root", root, "work", "list"]);
  assert.equal(listed.code, 0);
  assert.deepEqual(JSON.parse(listed.stdout).data.items.map((item) => item.name), ["alpha", "beta"]);

  const searched = await runCli(["--json", "--root", root, "work", "search", "bet"]);
  assert.equal(searched.code, 0);
  assert.deepEqual(JSON.parse(searched.stdout).data.matches.map((item) => item.name), ["beta"]);

  const opened = await runCli(["--json", "--root", root, "open", "alpha"]);
  assert.equal(opened.code, 0);
  const payload = JSON.parse(opened.stdout);
  assert.equal(payload.action, "work.open");
  assert.equal(payload.data.item.path, join(root, "Work", "alpha"));
});
