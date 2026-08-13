import assert from "node:assert/strict";
import { mkdir, mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { createCoreActionRegistry } from "../core/actions.js";
import { runCli } from "../core/cli.js";
import { createDefaultRuntime } from "../core/runtime.js";
import { initializeCentral } from "../core/root.js";
import { ResultStatus } from "../core/results.js";

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
    actionContext: { rootOptions: { explicitRoot: root, env: {} }, connectors: runtime.connectors },
  };
}

test("Work Actions complete ordinary-directory discovery and entry", async () => {
  const root = await fixture();
  const { registry, actionContext } = context(root);
  const listed = await registry.execute("work.list", {}, actionContext);
  assert.deepEqual(listed.data.items.map((item) => item.name), ["alpha", "alphabet", "beta"]);
  const searched = await registry.execute("work.search", { query: "bet" }, actionContext);
  assert.deepEqual(searched.data.matches.map((item) => item.name), ["alphabet", "beta"]);
  const selected = await registry.execute("work.open", { query: "beta" }, actionContext);
  assert.equal(selected.ok, true);
  assert.equal(selected.data.match, "exact");
  assert.deepEqual(selected.data.item, { name: "beta", path: join(root, "Work", "beta") });
});

test("Work entry preserves exact, ambiguous, absent, and unambiguous-search semantics", async () => {
  const root = await fixture(["alpha", "alpha-notes", "project-beta"]);
  const { registry, actionContext } = context(root);
  assert.equal((await registry.execute("work.open", { query: "alpha" }, actionContext)).data.match, "exact");
  assert.equal((await registry.execute("work.open", { query: "beta" }, actionContext)).data.item.name, "project-beta");
  const ambiguous = await registry.execute("work.open", { query: "alpha-" }, actionContext);
  assert.equal(ambiguous.ok, true);
  const absent = await registry.execute("work.open", { query: "gamma" }, actionContext);
  assert.equal(absent.status, ResultStatus.INVALID_INPUT);
});

test("work.open selectable input is sourced from the canonical work.list Action", () => {
  const descriptor = createCoreActionRegistry().get("work.open");
  assert.deepEqual(descriptor.requiredPorts, ["WorkDiscovery"]);
  assert.deepEqual(descriptor.inputs[0].selectableSource, {
    action: "work.list",
    collection: "items",
    valueField: "name",
  });
});

test("CLI projection returns the same canonical Work Action results", async () => {
  const root = await fixture(["alpha", "beta"]);
  const listed = await runCli(["--json", "--root", root, "work", "list"]);
  assert.equal(listed.exitCode, 0);
  assert.deepEqual(JSON.parse(listed.output).data.items.map((item) => item.name), ["alpha", "beta"]);
  const selected = await runCli(["--json", "--root", root, "open", "alpha"]);
  assert.equal(selected.exitCode, 0);
  assert.equal(JSON.parse(selected.output).action, "work.open");
}
);
