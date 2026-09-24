# Source transfer

```yaml
standing: architecture-contract
register: episteme
provenance: design-commitment
```

Source transfer moves authored source between two grounds that share one
world identity — for example the same Project on two machines, or the
Central root register (`control:root`) on two machines — through
Central's native source mechanisms, with explicit scope, direction and
authority, and with divergence handled as an explicit, recorded conflict.
It is the receive/apply path the Source Change Horizon implies and does not
implement itself. Nothing here widens any other product's control plane into
source semantics: material reconnection belongs to Workcell, and the bundle
moves as an ordinary file the operator carries.

## The primitives that already held

- Every participating source has a world-relative `SourceRef`
  (`central:source:<world_ref>:<path>`) and a deterministic content revision
  (`central.content-fnv1a64/v1:<len>:<hash>`). Two grounds that hold the same
  content revision hold the same source state; divergence is detectable by
  content alone, with no clocks and no machine identity.
- `projectcentral.source.write` is compare-and-swap on that revision, with
  per-binding write authority and declared attribution. The transfer applies
  through exactly that seam; it introduces no second write path.
- The horizon's change log gives a transfer its base: the revision an entry
  supersedes. A receiver fast-forwards only from a base it actually holds.

## The actions

`project` names a Project ground. Omit it (or pass `null`) and the same
Action works on the Central root register, `control:root` — the same native
Actions at both registers. An empty or malformed `project` is refused, never
read as the root.

```text
projectcentral.source.transfer.export
    project?, source_refs[], to_world_ref, from_ground, to_ground,
    since_cursor?, actor, actor_kind, agent_session_ref?
    → one central.source-transfer/v1 bundle (a JSON document)

projectcentral.source.transfer.apply
    project?, bundle | bundle_file, actor, actor_kind, agent_session_ref?,
    accept_unestablished_lineage? (array of source refs),
    accept_unestablished_identity? (root register only)
    → central.source-transfer-apply-receipt/v1, recorded under
      .central/source-transfer/records/ of the receiving ground

projectcentral.source.transfer.conflicts
    project?, status?           → recorded conflicts, open and resolved

projectcentral.source.transfer.resolve
    project?, source_ref, disposition (keep-local | accept-incoming),
    expected_local_revision? (required for accept-incoming),
    actor, actor_kind, agent_session_ref?
```

## What a bundle carries — and what it must never carry

A bundle names worlds, ground labels, world-relative paths, content
revisions and source payloads. It must never carry machine identities,
credentials, absolute local paths or generated runtime state:

- **Direction** is `from_world_ref`/`to_world_ref` plus operator-declared
  `from_ground`/`to_ground` labels. Two grounds of one shared world carry
  the same world ref, so the labels are human-chosen names — the same
  discipline as a Workcell connection label — never system-derived machine
  identity. The receiving ground verifies the world identity and nothing
  else; the labels are provenance.
- **Scope** is exactly the named `source_refs` (optionally windowed by
  `since_cursor`). There is no directory form. Export refuses anything it
  is not explicitly asked for, and refuses sources masked by
  `.no-agent-retrieval` — masking is not missing, on the way out either.
- **Natively owned sources never travel.** A Day, a NOW clearing, a
  protected contribution document, the placement and civil-time policies and
  the native action authority grants change only through their own
  authenticated owner operations, so export refuses them, and a payload that
  is a native action authority document (credential digests) is refused at
  export and at apply whatever path it arrives under.
- **Authority** is enforced by the receiving ground's own law at apply:
  every entry is checked with the same write-authority rule as
  `projectcentral.source.write` before anything is mutated. Human-authored
  ground refuses declared non-human transfers, exactly as it refuses their
  writes. The origin's binding travels as provenance, never as authority.
- **Payloads** are bounded UTF-8 source text, verified at apply against the
  recorded content revision before any mutation. A tampered bundle fails
  without touching the receiving ground.

## Outcomes, and the conflict law

Each entry lands as exactly one of:

- `applied` — the receiver holds the entry's base revision; the payload is
  written through compare-and-swap and the horizon change carries the
  transfer's declared attribution.
- `applied-added` — the origin created the source within the transferred
  range and the receiver lacks it; the creation fast-forwards.
- `established` — the receiver lacks the source and the entry has no
  retained base; the caller must name the source in
  `accept_unestablished_lineage`. Lineage begins by explicit, recorded
  acknowledgement, never by silent bootstrap. The receiver's horizon records
  the creation with the incoming revision, which is the base the next
  transfer fast-forwards from. Establishing a source also settles any open
  conflict an earlier unacknowledged apply recorded for it (disposition
  `established`, retained as evidence).
- `already-applied` — the receiver already holds the payload revision;
  replay is idempotent.
- `conflicted` — the receiver holds a different revision: a divergence.
  The transfer records an open conflict naming the base, incoming and local
  revisions, the origin ground and cursor, and the resolution path; both
  sides of the divergence are snapshotted byte-exactly beside the record.
  The source is not overwritten in either direction, and a transfer never
  deletes: removal of authored ground is an owner act on each ground, not a
  side effect of a sync.

A conflicted apply still reports every other entry's outcome honestly in
its receipt. A refusal — direction mismatch, foreign ref, authority,
masking, non-participating path, bad hash — leaves the receiving ground
byte-identical. A mutation that does not confirm marks the receipt
`uncertain` and names the error.

Resolution is explicit and recorded: `keep-local` retains the receiving
content and closes the record; `accept-incoming` writes the snapshotted
incoming content through compare-and-swap and requires
`expected_local_revision` to equal the exact revision the conflict recorded
— a source that moved since the conflict is refused, and the next apply
re-surfaces the divergence. Resolved records are retained evidence.

## The root register

The root register is the owner's Central root, `Control/`, on each machine.
Its participating sources are the Control horizon's: the owner's personal
ground under `Control/user/`, the Agent governance
(`Control/agents/governance/`), the root Wiki (`Control/agents/wiki/`), and
the root agent ground an agent host needs — AgentProfiles
(`Control/agents/profiles/`), agent expressions
(`Control/agents/expressions/`) and AgentSets (`Control/agents/agent-sets/`),
plus anything the root ground relations declare. The agent ground is agent
ground, not the owner's authored human ground, so a declared agent may carry
it. Its conflict records and receipts live under the root ground's own
`.central/source-transfer/`.

Four rules hold at the root that a Project transfer does not need:

- **Personal ground moves only for a declared human.** A ref under
  `Control/user/` — identity, placement, civil time, native action
  authority, the owner's own source — is refused at export, apply and
  resolve for any caller whose `actor_kind` is not `human`, with a
  three-part refusal (the fact, what did not happen, what to do instead).
  Agents do not carry the owner's personal ground between machines.
- **Machine ground never moves.** A ref under `Control/machines/` names one
  machine; it is refused for every caller. Each machine is declared on its
  own ground.
- **Every creation is acknowledged.** A root ground that lacks a source
  creates it only when the caller names it in
  `accept_unestablished_lineage` — even when the origin's history calls the
  entry `added`. A bootstrap root shares no lineage with its origin, and the
  origin's `added` history for the agent ground dates from when that ground
  began to participate, not from the file's creation. Without the
  acknowledgement the entry is a recorded conflict and nothing is created.
- **Identity is verified by subject, or accepted explicitly.** Both grounds
  are `control:root`, so the world ref cannot tell two owners apart. A root
  bundle carries the origin's identity manifest subject ref
  (`Control/user/identity/manifest.json`, e.g. `central:pasu:nara:local`) —
  a ref naming the owner, never a machine. When both grounds have an
  identity manifest and the subjects match, the receipt records
  `verified`; when they differ the apply is refused whatever the flags say.
  When either ground has no manifest, identity is unestablished: the apply
  is refused unless the caller passes `accept_unestablished_identity: true`,
  and the receipt records `accepted-unestablished`. Identity is never
  inferred.

AgentProfile and AgentSet payloads are also checked against the receiving
root's own stores before anything is written: a profile must be a root
profile whose ref names its path, and an AgentSet must carry the AgentSet
schema and a ref that names its path. A record the store could not read back
would break the listing of every record beside it.

Profile acceptance is not transferred. Acceptance receipts are generated
state under `.central/agent-profile-acceptances/` of each ground; a
transferred profile is accepted on the receiving ground by that ground's own
`agent-profile.accept`, if it needs to be.

## Operator runbook: root agent ground to a second machine

Both machines must run a `ctrl` that includes root transfer. The example
moves the root agent ground from the Mac (`mac`) to the agent host
(`omarchy`); the ground labels are names you choose. Commands use `jq`.
Run the export as yourself (`actor_kind` `human`), or as an agent with
`actor_kind` `agent` and an `agent_session_ref`.

1. On the origin, collect the agent ground refs from the root horizon
   (hidden files such as `.DS_Store` are left out; they are not UTF-8
   source):

   ```sh
   ctrl --json action run projectcentral.change.horizon '{}' \
     | jq '[.data.sources[].binding.ref
            | select(test("^central:source:control:root:Control/agents/(profiles|expressions|agent-sets)/"))
            | select(test("/\\.[^/]*$") | not)]' > /tmp/root-agent-refs.json
   ```

2. Export the bundle:

   ```sh
   ctrl --json action run projectcentral.source.transfer.export "$(jq -n \
     --slurpfile refs /tmp/root-agent-refs.json \
     '{source_refs: $refs[0], to_world_ref: "control:root",
       from_ground: "mac", to_ground: "omarchy",
       actor: "owner", actor_kind: "human"}')" \
     | jq '.data' > /tmp/root-agent-ground.bundle.json
   jq '{origin_cursor, sources: (.sources | length)}' /tmp/root-agent-ground.bundle.json
   ```

   Keep `origin_cursor`: it windows the next transfer. One bundle names at
   most 256 refs; if the agent ground grows past that, export each of the
   three directories as its own bundle.

3. Move `/tmp/root-agent-ground.bundle.json` to the receiving machine as an
   ordinary file (for example `scp`).

4. On the receiver, apply. The first transfer to a bootstrap root names
   every entry's lineage, and — while the receiver has no identity
   manifest — accepts the unestablished identity:

   ```sh
   B=/tmp/root-agent-ground.bundle.json
   ctrl --json action run projectcentral.source.transfer.apply "$(jq -n \
     --arg file "$B" --slurpfile b "$B" \
     '{bundle_file: $file, actor: "owner", actor_kind: "human",
       accept_unestablished_identity: true,
       accept_unestablished_lineage: [$b[0].sources[].source_ref]}')" \
     | jq '.data | {status, applied_count, conflicted_count, identity}'
   ```

   Expect `status: "applied"` and `identity.verification:
   "accepted-unestablished"`.

5. Check that the receiving Central sees the records:

   ```sh
   ctrl --json action run central.position.list '{"project":"O-I"}' | jq '.data.invalid'
   ctrl --json action run agent-profile.list '{"scope":"root"}' | jq '.data'
   ```

   The Positions whose `profile_ref` named a missing profile are no longer
   listed as invalid.

Later transfers export with `since_cursor` set to the previous bundle's
`origin_cursor` (the receiver's receipt records it too); entries then carry
their base and fast-forward. A source that is new since the last transfer
lands only when named in `accept_unestablished_lineage`; without it the
receipt reports it `conflicted` with no local revision and nothing is
created. To stop accepting identity unestablished, the owner carries the
identity manifest itself as a declared human — export
`central:source:control:root:Control/user/identity/manifest.json` with
`actor_kind` `human`, apply it with both acknowledgements — after which both
grounds name the same subject and every later apply records `verified`.

## What transfer is not

- Not a sync system: there is no daemon, no schedule, no automatic
  transfer. The operator exports, moves the file, and applies.
- Not a merge engine: content is whole-file; convergence is a human
  decision taken through an explicit resolution.
- Not cross-world: a bundle applies only on the world its direction names;
  renaming a project is a different world and needs its own decision.
- Not a replication of `.central` state: horizons, records and cursors are
  each ground's own derived state and are never carried in a bundle.
