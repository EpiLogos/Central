---
role: architecture
standing: agent-inference
scope: Central native operations and composed O:I consumer boundaries
updated: 2026-10-02
---
# Central architecture navigation

This is an implementation-facing navigation companion for O:I #65/#220 and the
existing documentation programme. It recovers native owners and successors;
it does not adopt a new design or claim the whole running experience complete.
Inspected native checkout revision: `abdb67f3e0b091617ca485f2352b6e26c62e99ec`. Active repair source may advance
that cut; identify the file revision before relying on its returned result.

## Governing source and successors

- [CENTRAL-SYSTEM-SPEC](CENTRAL-SYSTEM-SPEC.md)
- [CONTROL-CONTENT-PROTOCOL](CONTROL-CONTENT-PROTOCOL.md)
- [CAW-NATIVE-CONSUMER-INTERFACES](CAW-NATIVE-CONSUMER-INTERFACES.md)
- [SOURCE-RETURN](SOURCE-RETURN.md) — current Source proposal and acceptance.
- [PROJECTCENTRAL-FLOW](PROJECTCENTRAL-FLOW.md) — retained predecessor and retirement notice.


Directory names are routes, not authority. Target design, amendment, historical
baseline, current implementation and observed result keep their own standing.

| Concern | Public operation / entry | Native source | Boundary and lifecycle |
| --- | --- | --- | --- |
| World / authored source | `central.world.here`, source reading/history | `ctrl/src/world_here.rs`; `ctrl/src/source_horizon.rs` | Personal root source; product checkout is separate. |
| Workcell-root / child NOW / Day | `central.now.workcell-root`, `central.now.allocate`, native Day actions | `ctrl/src/continuous_work/placement.rs`; `temporal.rs` | Native refs, placement/time revisions; Day closure and ongoing NOW lifecycle are distinct. |
| Ordinary plural Flow | `central.flow.read`, `central.flow.append` through `ctrl action run` | `ctrl/src/flow_append.rs`; `ctrl/tests/flow_append.rs` | CAS source append with request digest and departed-caller gate; AIKit owns dispatch, not Central. |
| Receiving | `central.receiving.*` | `ctrl/src/continuous_work/receiving.rs` | Native receipt readback is distinct from human Recognition. |

## Diagram and consumer relation

The maintained suite companion is
`source:project:O-I:docs/architecture/plural-flow.md`; its editable diagram is
`source:project:O-I:docs/architecture/plural-flow.mmd`. The O:I architecture entry
contains six question-specific companions, full-size rendered SVGs, an indexed
basis for every arrow, exact source hashes and independent navigation evidence.
Resolve that source in the current O:I checkout before substituting a cached or
historical copy. Its solid arrows are inspected relations, not installed
acceptance; proposed joins remain explicitly proposed.

Existing capability records link this companion through the optional
`extensions.documentation` protocol. These links change discoverability, not
capability IDs, coordinate placements, implementation status or source authority.

## Verification and open joins

Read each native test and its actual runner conditions, then the corresponding
dated Return. A test definition is not an executed result; a process/receipt is
not human Recognition. The suite's architecture verification records real
Mermaid rendering, source/link checks and the fresh-agent navigation task.
The four repair lanes continue to own their code, installed replay and open
architectural decisions. Preserve a missing join as missing until that proof
or decision is returned.

## Publication candidate and verification boundary — 2 October 2026

Central owns the ProjectCentral source bindings, source horizon and native
initialise/adopt/migrate operations. Its register-scoped physical Wiki publisher
retains exact filesystem basis and effect receipts. A publication lock is
material coordination, excluded from source counting; it supplies no source
identity or retrieval permission. See the native owner
[publication module](https://github.com/EpiLogos/Central/blob/28e514ef17e5e81508c3ba22429df73ecdb0482a/ctrl/src/wiki_publication.rs)
and [source horizon correction](https://github.com/EpiLogos/Central/blob/bba5899a871a8c4733be09e9f435d9028dc7bad3/ctrl/src/source_horizon.rs).

The original inspection above remains a dated baseline. At
`a2732db075e345521e8d18d1b71356384e9c7f28`, the named
[Verify](https://github.com/EpiLogos/Central/actions/runs/36965054799),
[controlled World](https://github.com/EpiLogos/Central/actions/runs/36965054765),
[root / Day](https://github.com/EpiLogos/Central/actions/runs/36965054753) and
[documentation](https://github.com/EpiLogos/Central/actions/runs/36965054752)
gates pass. The preceding `bba5899a871a8c4733be09e9f435d9028dc7bad3` cut
repairs register-scoped source counting; the a273 successor repairs actual
neighbour-preserving fixture cleanup. These executed results do not ratify the
intended guided-authoring journey, install a personal World or establish human
Recognition. Earlier held source candidates keep their original standing.

The live source-change doorway is [SOURCE-RETURN](SOURCE-RETURN.md).
`projectcentral.flow.inspect/read/write`, the former private Flow register and
its revision store are retired; [PROJECTCENTRAL-FLOW](PROJECTCENTRAL-FLOW.md)
records that predecessor. Ordinary authored Flow uses `central.flow.read` and
`central.flow.append`; AIKit owns participant dispatch and interruption
readback. A Source proposal/acceptance preserves exact basis and proposed bytes;
an Agent Report or task completion does not supply Source acceptance.

These boundaries exist because native source ownership, physical persistence,
derived material, transport, continuation and human presentation carry different
authority. A post-publication readback failure retains the effect and original
error; it cannot be represented as an unchanged pre-publication refusal.
Follow the dated native receipt before replaying a partially completed action.
Current source admission, independent promotion, live-origin withdrawal and
delivered agent context require their own owners and evidence.
