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
Earlier implementation baseline (retained): `abdb67f3e0b091617ca485f2352b6e26c62e99ec`. Active repair source may advance
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
| Source binding / file map | `central.file-map.resolve`, `locate` | `ctrl/src/file_map.rs`; `file_map_catalog.rs` | Native ownership, requesting context and body permission are distinct; qualified metadata mode is described below. |
| World / AgentSet record reader | Native read/list/effective-sources Actions; `central.world.here` consumer | `ctrl/src/agent_set_store.rs`; `agent_set_actions.rs`; `world_here.rs` | Store owns identity/material admission; projection preserves its refusal and owning-root source location. Scoped hosted qualification is recorded below; personal installation is separate. |

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

## Native ownership and World disclosure — pending source cut

This retained unit was originally a pending source cut. Its native relations
now have [scoped hosted qualification](#scoped-native-qualification--2-october-2026);
personal installation and later consumer cuts remain separate.

For ownership routing, follow `central.file-map.resolve` / `locate` to
[`file_map.rs`](../ctrl/src/file_map.rs) and
[`file_map_catalog.rs`](../ctrl/src/file_map_catalog.rs). The
[binding-only companion](integrations/BKMR-FILE-MAP.md#binding-ownership-metadata--source-candidate)
separates native ownership metadata from body permission, declared identity,
payload revision and healthy nonownership. Registered bkmr data remains
acceleration material; it cannot replace Source roles or provenance.

For World/AgentSet material, follow
[`RelationRecordStore`](../ctrl/src/agent_set_store.rs) through the
[same held native reader, capacity and complete-membership checkpoints](NATIVE-FILESYSTEM-READING.md#native-relation-records--source-candidate).
For `central.world.here`, follow
[`world_record`](../ctrl/src/world_here.rs) to that owner, then to the declared
World ref and the actual record at its owning root. The
[World projection](NATIVE-FILESYSTEM-READING.md#world-disclosure) preserves that
owner-relative source location; declared record revision, physical content
revision, projection state and runtime verification remain distinct.

The original source-only capture is retained as the design/implementation basis.
The later scoped hosted results are recorded below; they do not establish
personal installation or human acceptance. Original basis: Central `7a0e2c21505ad7cf6590db8701fc473102fd399b`
preimages, native owner v4 packet SHA-256
`390656e902c866959304094a9b92c46f52434ba04140e17fe10a683bb8f7ca79`,
unchanged response handover
`15ebcf6037f3e0f5cf43b896ff97cdc0a5460d0363d13aa35dec96f35a0568c8`,
and World consumer v1.3 packet
`c5652f528091f7952c0b77076c7084f8c2033f16417dc9fae74417f8c3d868e5`.
At that original capture the 58 joined, 13 binding, 18 Store and five
World-consumer definitions were UNRUN. At Central
`452525d45fce20c2c667b87ee9590e7f5a3883c6`, the normal Linux/macOS
run executed the native suites and the pinned joined run executed all 58
cases. Follow the exact cuts and limits below rather than carrying the
earlier UNRUN label forward as live guidance.

The remaining questions are how a qualified current AIKit owner admission
reaches selected agent context with the original failure cause, and how
ordinary root Return producer registration and lifecycle complete their missing
join. Hosted receipt recovery is a different operation from that ordinary
Agent ledger and does not close it. Initial-allocation authority and human Recognition keep their
separate standing. The existing diagrams retain their recorded arrow bases;
no implemented or verified join is inferred from these proposals.

## Scoped native qualification — 2 October 2026

The producer, consumer and test basis must be named together. These results
update execution standing for their selected relations; they do not adopt
new product architecture or prove the current personal installation.

| Source cut | Executed evidence | Scope and limit |
| --- | --- | --- |
| Central `452525d45fce20c2c667b87ee9590e7f5a3883c6`; AIKit `338232e5fca8d1df81ac81a37fd9d610278e2e43` | [Normal Linux/macOS run 37061163439](https://github.com/EpiLogos/Central/actions/runs/37061163439); [joined run 37061204754](https://github.com/EpiLogos/Central/actions/runs/37061204754): 58 unique cases, 58 passed, zero failures/skips. The normal log includes 18 Store, 13 binding and five World-consumer definitions on each host. | Normal merge `cadc4942a5aa64dd1e00216b5203dea2ff7052d7` has the same tree `4ffb02d0f04684c1b974ebd50a5d014d45043cfe`. The campaign compares 16 distinct source paths; that is not 18 duplicate-counted paths or all 34 PR paths. |
| Central `5eb41c635f2978f5adb34f0c947f774eda498827`; the same AIKit cut | [Joined run 37066338855](https://github.com/EpiLogos/Central/actions/runs/37066338855): all 58 unique cases passed in 9.380 seconds, including strengthened case 28. | Actual missing owner executable returns no retained unique body; source, inode and provider database stay unchanged and reconnection recovers the same body. AIKit338 still classifies the native owner IO failure as `knowledge.source_missing`; preservation of its original unavailable cause remains AIKit R4 work. |
| Central `7d38080dba70161e6f2d507b49c2b8488a14fba8` | [Normal run 37063035252](https://github.com/EpiLogos/Central/actions/runs/37063035252), [CAW run 37063035195](https://github.com/EpiLogos/Central/actions/runs/37063035195), [root run 37063035193](https://github.com/EpiLogos/Central/actions/runs/37063035193): eight unique receipt-lookup definitions, 32 repeated pass observations. Actual closed stdout exits 101 on Linux and macOS after one native Submit; original-producer lookup finds the same one receipt without resend. | Seven source files have 21 head/merge/artifact byte comparisons. Normal merge is `4d27d164ee372aee82675776e75edd236966dbe5`. This qualifies authenticated original-producer recovery in the current receiving scope, not Factory/Gateway consumers, inclusion error transport, human Return, ordinary NOW registration or Recognition. |

The joined binaries use Rust 1.98.1, bkmr 7.6.7 and locked builds; normal
checks use Rust 1.99.0. Normal source installation is an ephemeral Linux smoke
with a separate unlocked dependency resolution and no published installed
binary hash. [Normal run 37066154057](https://github.com/EpiLogos/Central/actions/runs/37066154057)
is separately reported successful for the 5eb cut; its new install dependency
and binary cut were not independently reconstructed in the accepted proof.

Parent acceptance receipts have SHA-256
`e297eca7f99e50f3d417415557e4e22b0d37d504993161a29e6202bfdcd971a2` (452/7d)
and `24a836a8bf1f5e1915eae2f93d4c7d2b7fc0864a65e319f8e3debf94ad1388f7`
(5eb). The retained raw logs and native FileRef readbacks are their evidence
depth. A null Source binding on an evidence FileRef is not healthy
`ownership:"unregistered"` or a SourcePool publication claim.

Earlier run 37055908205 remains 54 passes and four failures. Case 06 was a
production import refresh repair replayed through its original case; cases
55/56 repaired incomplete inherited-World fixtures with actual linked Project
and exclusion paths. The former case 37 invoked a retired Flow writer. Its
successor verifies explicit retired-action refusal, current Source move and
rollback, and byte-preserved legacy registry/history. It is a corrected
fixture classification, not revival or successful replay of the obsolete API.

The existing capability catalogue has no exact file-map metadata or native
Receiving-recovery command mapping. Scoped Store read/list evidence belongs
with bounded relations; adjacent Source records point to this proof as
pressure on their mapping. The World-map record already maps
`central.world.here`; this reconciliation pairs that existing identity with
its actual World-here module, tests and scoped native projection evidence.
It does not verify every world-map/context relation. Receiving recovery does
not verify the different ordinary NOW operation. Keep those mapping questions with O:I #65/#220's
APD15/APD20 obligations, alongside current AIKit R4, Gateway/Factory consumer,
ordinary Root Return/lifecycle and installed/human experience qualifications.
