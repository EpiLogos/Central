# Source transfer

```yaml
standing: architecture-contract
register: episteme
provenance: design-commitment
```

Source transfer moves authored source between two grounds that share one
world identity — for example the same Project on two machines — through
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

```text
projectcentral.source.transfer.export
    project, source_refs[], to_world_ref, from_ground, to_ground,
    since_cursor?, actor, actor_kind, agent_session_ref?
    → one central.source-transfer/v1 bundle (a JSON document)

projectcentral.source.transfer.apply
    project, bundle | bundle_file, actor, actor_kind, agent_session_ref?,
    accept_unestablished_lineage?
    → central.source-transfer-apply-receipt/v1, recorded under
      .central/source-transfer/records/

projectcentral.source.transfer.conflicts
    project, status?            → recorded conflicts, open and resolved

projectcentral.source.transfer.resolve
    project, source_ref, disposition (keep-local | accept-incoming),
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
  `since_cursor`). Export refuses anything it is not explicitly asked for,
  and refuses sources masked by `.no-agent-retrieval` — masking is not
  missing, on the way out either.
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
  acknowledgement, never by silent bootstrap.
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

## What transfer is not

- Not a sync system: there is no daemon, no schedule, no automatic
  transfer. The operator exports, moves the file, and applies.
- Not a merge engine: content is whole-file; convergence is a human
  decision taken through an explicit resolution.
- Not cross-world: a bundle applies only on the world its direction names;
  renaming a project is a different world and needs its own decision.
- Not a replication of `.central` state: horizons, records and cursors are
  each ground's own derived state and are never carried in a bundle.
