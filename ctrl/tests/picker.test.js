import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, mkdir } from "node:fs/promises";
import { spawn } from "node:child_process";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { createCoreActionRegistry } from "../core/actions.js";
import { ConnectorRegistry } from "../core/connectors.js";
import { runGuidedActionPicker, searchActionDescriptors } from "../core/picker.js";
import { ResultStatus } from "../core/results.js";
import { createStaticWorkDiscoveryConnector } from "../../connectors/reference/work-discovery.js";

function scriptedPrompt(values) {
  const queue = [...values];
  return async () => queue.shift();
}

function runtime(items = [{ name: "alpha", path: "/work/alpha" }, { name: "beta", path: "/work/beta" }]) {
  return {
    registry: createCoreActionRegistry(),
    context: {
      rootOptions: { explicitRoot: "/Central" },
      connectors: new ConnectorRegistry().register(createStaticWorkDiscoveryConnector(items)),
    },
  };
}

test("picker search ranks canonical registry Action descriptors", () => {
  const { registry } = runtime();
  assert.deepEqual(searchActionDescriptors(registry.list(), "work.open").map(({ id }) => id), ["work.open"]);
  assert.equal(searchActionDescriptors(registry.list(), "enter")[0], registry.get("work.open"));
});

test("guided work.open resolves its selectable value through work.list and returns the direct Action result", async () => {
  const guidedRuntime = runtime();
  const directRuntime = runtime();
  const guided = await runGuidedActionPicker({
    registry: guidedRuntime.registry,
    context: guidedRuntime.context,
    prompt: scriptedPrompt(["work.open", "1", "2"]),
  });
  const direct = await directRuntime.registry.execute("work.open", { query: "beta" }, directRuntime.context);
  assert.deepEqual(guided, direct);
  assert.equal(guided.action, "work.open");
});

test("picker cancellation at search is a structured normal result", async () => {
  const { registry, context } = runtime();
  const result = await runGuidedActionPicker({ registry, context, prompt: scriptedPrompt(["q"]) });
  assert.equal(result.status, ResultStatus.CANCELLED);
  assert.equal(result.action, null);
});

test("picker cancellation at Action selection is a structured normal result", async () => {
  const { registry, context } = runtime();
  const result = await runGuidedActionPicker({ registry, context, prompt: scriptedPrompt(["work", "cancel"]) });
  assert.equal(result.status, ResultStatus.CANCELLED);
  assert.equal(result.action, null);
});

test("picker cancellation at selectable value is attributed to the selected canonical Action", async () => {
  const { registry, context } = runtime();
  const result = await runGuidedActionPicker({ registry, context, prompt: scriptedPrompt(["work.open", "1", "/cancel"]) });
  assert.equal(result.status, ResultStatus.CANCELLED);
  assert.equal(result.action, "work.open");
});

test("ctrl pick works as a real terminal Surface and exits successfully", async () => {
  const root = await mkdtemp(join(tmpdir(), "central-picker-process-"));
  await mkdir(join(root, "Work", "alpha"), { recursive: true });
  const bin = fileURLToPath(new URL("../bin/ctrl.js", import.meta.url));
  const child = spawn(process.execPath, [bin, "--root", root, "pick"], { stdio: ["pipe", "pipe", "pipe"] });
  let stdout = "";
  child.stdout.setEncoding("utf8");
  child.stdout.on("data", (chunk) => { stdout += chunk; });
  child.stdin.end("work.open\n1\n1\n");
  const exitCode = await new Promise((resolve, reject) => {
    child.once("error", reject);
    child.once("close", resolve);
  });
  assert.equal(exitCode, 0);
  assert.match(stdout, /^alpha\t/);
});
