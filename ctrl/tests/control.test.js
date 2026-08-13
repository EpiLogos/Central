import assert from "node:assert/strict";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawn } from "node:child_process";
import test from "node:test";

import { createCoreActionRegistry } from "../core/actions.js";
import { initializeCentral } from "../core/root.js";
import { ResultStatus } from "../core/results.js";

const CLI = new URL("../bin/ctrl.js", import.meta.url);

async function fixture() {
  const root = join(await mkdtemp(join(tmpdir(), "central-control-")), "Central");
  await initializeCentral(root);
  return root;
}

function runCli(args) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [CLI.pathname, ...args], { stdio: ["ignore", "pipe", "pipe"] });
    let stdout = ""; let stderr = "";
    child.stdout.on("data", (chunk) => { stdout += chunk; });
    child.stderr.on("data", (chunk) => { stderr += chunk; });
    child.on("error", reject);
    child.on("close", (code) => resolve({ code, stdout, stderr }));
  });
}

test("control.open resolves only the three stable authored source roots", async () => {
  const root = await fixture();
  const registry = createCoreActionRegistry();
  for (const target of ["user", "agents", "machines"]) {
    const result = await registry.execute("control.open", { target }, { rootOptions: { explicitRoot: root, env: {} } });
    assert.equal(result.ok, true);
    assert.equal(result.data.sourceClass, "authored");
    assert.equal(result.data.path, join(root, "Control", target));
  }
  const invalid = await registry.execute("control.open", { target: "profiles" }, { rootOptions: { explicitRoot: root, env: {} } });
  assert.equal(invalid.status, ResultStatus.INVALID_INPUT);
});

test("control.search sees direct Markdown and text edits immediately without import or database state", async () => {
  const root = await fixture();
  await writeFile(join(root, "Control", "user", "about.md"), "I prefer working candidates.\n");
  await writeFile(join(root, "Control", "agents", "style.txt"), "Evidence should support completion claims.\n");
  const registry = createCoreActionRegistry();
  const first = await registry.execute("control.search", { query: "working candidates" }, { rootOptions: { explicitRoot: root, env: {} } });
  assert.deepEqual(first.data.matches.map((match) => match.sourcePath), ["user/about.md"]);
  await writeFile(join(root, "Control", "user", "about.md"), "I prefer direct visual review.\n");
  const second = await registry.execute("control.search", { query: "visual review" }, { rootOptions: { explicitRoot: root, env: {} } });
  assert.deepEqual(second.data.matches.map((match) => match.sourcePath), ["user/about.md"]);
});

test("control.search stays inside Control and reports unsupported source formats", async () => {
  const root = await fixture();
  await mkdir(join(root, ".central", "index"), { recursive: true });
  await writeFile(join(root, ".central", "index", "derived.md"), "needle derived state\n");
  await writeFile(join(root, "Control", "user", "visible.md"), "needle authored source\n");
  await writeFile(join(root, "Control", "user", "image.bin"), "needle unsupported\n");
  const result = await createCoreActionRegistry().execute("control.search", { query: "needle" }, { rootOptions: { explicitRoot: root, env: {} } });
  assert.deepEqual(result.data.matches.map((match) => match.sourcePath), ["user/visible.md"]);
  assert.deepEqual(result.data.unsupported, [{ target: "user", sourcePath: "user/image.bin", format: ".bin" }]);
});

test("missing Control roots are explicit in open and search results", async () => {
  const root = await fixture();
  await rm(join(root, "Control", "machines"), { recursive: true, force: true });
  const registry = createCoreActionRegistry();
  const open = await registry.execute("control.open", { target: "machines" }, { rootOptions: { explicitRoot: root, env: {} } });
  assert.equal(open.status, ResultStatus.INVALID_CENTRAL_STRUCTURE);
  const search = await registry.execute("control.search", { query: "anything" }, { rootOptions: { explicitRoot: root, env: {} } });
  assert.deepEqual(search.data.missingRoots.map((item) => item.target), ["machines"]);
});

test("CLI exposes structured Control source behavior without a profile database", async () => {
  const root = await fixture();
  await writeFile(join(root, "Control", "agents", "engineering.md"), "Executed evidence matters.\n");
  const opened = await runCli(["--json", "--root", root, "control", "open", "agents"]);
  assert.equal(opened.code, 0);
  assert.equal(JSON.parse(opened.stdout).data.path, join(root, "Control", "agents"));
  const searched = await runCli(["--json", "--root", root, "control", "search", "executed evidence"]);
  assert.equal(searched.code, 0);
  assert.deepEqual(JSON.parse(searched.stdout).data.matches.map((match) => match.sourcePath), ["agents/engineering.md"]);
});
