# NEXT-SESSION PROMPT — Central: the T/T' thoughts system; retire the legacy flow store (2026-09-13)

You are continuing Central (ctrl) and the personal ground. The owner has
ratified the flow carrier change (O-I #267 proposal, O-I #271 desktop lock,
Central #174 create door) and now directs the founding of the T/T' thoughts
system and the full retirement of the legacy flow store. Read this file, the
two landing records it names, then work the queue in order.

## The owner's model — this is the founding shape, not a suggestion

- **A NOW is a task boundary**: a horizon or opening for activity. Bounded by
  allocation, closed by lifecycle. This part exists (`central.now.*`).
- **The T/T' folder within a NOW tracks the inner contemplative stream.**
  `T/` exists today in each clearing (dated markdown fixtures — e.g.
  `Control/agents/now/clearings/<id>/T/legacy-field-active-review-2026-09-12.md`)
  but is ad-hoc: no schema, no actions, no linkage across NOWs. `T'` does not
  exist anywhere yet.
- **The contemplate action is the process that turns T fixtures into T'
  learnings** — parsing the signal out of the raw stream. Its output today
  lands in Central knowledge nodes (the `projectcentral.flow.*`
  contemplate/commission family). That is the wrong home: contemplate outputs
  belong to the NOW's T/T' system.
- **T/T' must be coherent relative to all bounded NOWs** — the same shape in
  every clearing, queryable across them, integrated with the day/now
  archiving (`day/<date>.sources/**` snapshots; contemplate outputs are
  agent-authored fixtures and ride that snapshot like everything else).

## Queue (in order; one cell = one branch = one PR)

1. **Found T/T' natively.** Give each clearing's `T/` a law and a `T'` a
   home: `T/` holds raw contemplative fixtures (dated, attributed);
   `T'` holds learnings distilled from T — each learning naming the T
   fixture(s) it was parsed from. Native actions (read/append/distill, root
   scope, the explicit-null project convention) so any harness can walk a
   NOW's inner stream and its distilled learnings. Bounded, attributed,
   day-close-snapshotted like every fixture.
2. **Re-aim contemplate.** The contemplate action's execution becomes: read a
   NOW's T fixtures, parse signal, write T' learnings. Its preflight/execute
   contract may keep its shape but must address T/T', not knowledge nodes.
   Whatever the O-I desktop's FlowCognition surface expects — coordinate the
   contract change (the desktop renders owner refusals verbatim, so honest
   transition states are acceptable landing states).
3. **Retire the legacy flow store.** `projectcentral_flow.rs`, the
   `projectcentral.flow.*` actions, and the `.central/flows.json` /
   `flow-revisions` registries. The human writing path no longer uses them
   (O-I #271 moved it to `central.files.write` instances — see
   `desktop/cradle/src/flow/{instance,instances}.ts`). Commission, if it
   still needs a flow-shaped subject, addresses the instance carrier or T/T',
   not the store. Strip the tracked flow fixtures in the sibling repos as
   part of this cell: Actuation (35 tracked `ProjectCentral/now/flows/*.md`),
   Central (1), ai-kit (1) — tiny coordinated PRs, custody in each repo's
   history.
4. **Live-ground residue.** The untracked residue is already removed
   (root `Control/agents/now/flows/`, root `.central/flows.json` +
   `flow-revisions/`, per-repo untracked `.central/flows.json`). After cell 3,
   verify nothing recreates them: `central.init`/projectcentral init must not
   materialise flows registries any more.

## Machine facts (re-verify; re-survey beats memory)

- ctrl `d488dfe69a3a` installed at ~/.cargo/bin (Central main `c6d86c9`,
  #174 — the `central.files.write` create door for
  `Control/user/flows/**`). O-I main `5efb1a55` (#271 — the instance
  carrier, flow-canvas 26/26; floor 20/20).
- aikit `586b85eaf77c`; the O-I walk pins:
  `OI_CENTRAL_CTRL_BIN=~/.cargo/bin/ctrl OI_AIKIT_BIN=~/.cargo/bin/aikit
  OI_AIKIT_SESSION_SPACE_BIN=~/.cargo/bin/aikit-session-space
  OI_CAW_ACTUATION_BIN=~/Central/Work/Actuation/target/release/actuation
  OI_CAW_WORKCELL_BIN=~/Central/Work/Workcell/target/release/workcell-write-boundary
  OI_FACTORY_BIN=~/.local/bin/factory` then `node walk/run.mjs <suites>` in
  `Work/O-I/desktop/cradle` (floor: 20 suites, all green at last landing).
- The O-I flow contracts you must not break without coordinating:
  `flow-<local-stamp>.html` instances in `Control/user/flows/`, in-file
  `meta.documentId`/`created`/`revision`, append-entry as the desktop's
  write contract, `central.files.write` create door.

## Enforced loop

BRIEF (≤60 lines) → BUILD (fresh worktree from main; never a primary
checkout) → WALK/test (receipts are the acceptance) → RECEIPT (progress/notes
row) → push + PR → land on green. Claim before building: push the branch and
open the draft PR first.

## Stop conditions

Run out of queue, not out of energy. Where an owner decision is genuinely
open (e.g. T' learning schema fields beyond source-fixture linkage, or
whether commission survives on the instance carrier), propose in the PR and
name it in the receipt — do not invent silently.
