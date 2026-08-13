import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

async function text(relative) {
  return readFile(new URL(relative, import.meta.url), "utf8");
}

test("Connector authoring Skill preserves public extension boundaries", async () => {
  const skill = await text("../../skills/connector-authoring/SKILL.md");
  assert.match(skill, /Port contract as authoritative/);
  assert.match(skill, /Action → Port → Connector → target/);
  assert.match(skill, /public SDK/);
  assert.match(skill, /conformance/);
  assert.match(skill, /Failure discipline/);
  assert.match(skill, /canonical `work\.list`/);
});

test("source maintenance Skill distinguishes durable source, scope, procedure, and accepted mutation", async () => {
  const skill = await text("../../skills/source-maintenance/SKILL.md");
  assert.match(skill, /durable human-owned source/);
  assert.match(skill, /Observed state and generated advice/);
  assert.match(skill, /project-specific facts, CI workflows, test commands, gates/);
  assert.match(skill, /Skill or Action/);
  assert.match(skill, /explicit human acceptance/);
});

test("source maintenance fixtures cover required audit and verification cases", async () => {
  const fixtures = JSON.parse(await text("../../skills/source-maintenance/fixtures.json"));
  const byId = new Map(fixtures.cases.map((item) => [item.id, item]));
  for (const id of ["clean-control-tree", "stale-content", "conflicting-content", "misplaced-procedure", "verification-preference-dialogue"]) {
    assert.equal(byId.has(id), true, `missing fixture ${id}`);
  }
  assert.equal(byId.get("conflicting-content").expected.silentMerge, false);
  assert.equal(byId.get("clean-control-tree").expected.mutationRequiresAcceptance, true);
  assert.equal(byId.get("verification-preference-dialogue").expected.providerMechanicsRemainProjectLocal, true);
  assert.match(byId.get("verification-preference-dialogue").expected["Control/agents"], /executed evidence/);
  assert.match(byId.get("verification-preference-dialogue").expected.projectLocal, /GitHub Actions/);
});
