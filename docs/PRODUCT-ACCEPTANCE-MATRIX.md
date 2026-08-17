# Central product acceptance matrix

**Scope:** #19 full remote/product acceptance against the normative criteria in `docs/CENTRAL-SYSTEM-SPEC.md`.

This matrix distinguishes **hosted product evidence** from the separately named **physical deployment evidence** still required by #14–#17. A GitHub macOS or Ubuntu runner is valid evidence for code, public contracts, real provider binaries, harmless reconciliation, and portability. It is not evidence that the user's `primary-workstation` or `home-server` has been exercised.

The #19 workflow reruns the live feature heads rather than trusting their issue text:

```text
build/central-ticket-19       core + SDK + Skills + #18 + missing acceptance regressions
build/central-ticket-16       macOS native + Homebrew + chezmoi + recovery
build/central-ticket-15       Shortcuts + macOS host Surface + Raycast
build/central-ticket-17       Ubuntu providers + headless host + recovery
```

## Normative criteria

| # | Product criterion | Hosted evidence | Status |
|---:|---|---|---|
| 1 | Create or recover a Central root and inspect it without special software. | `ctrl/tests/foundation.rs`; `central.init`, `central.doctor`; recovery suites. | accepted remotely |
| 2 | `Work` remains an ordinary directory tree independent of Central repository history. | Work discovery/action tests use ordinary directories; no Work metadata requirement. | accepted remotely |
| 3 | Control supports user, agent-governance, and machine intent without a forced ontology. | `CONTROL-CONTENT-PROTOCOL.md`; Control source/action tests; machine declarations are a specific machine contract rather than a universal Control schema. | accepted remotely |
| 4 | CLI discovers and invokes canonical Actions. | `action.list`; `ctrl/tests/foundation.rs`; #19 adds complete `action run <id> [json]` and restores `work.reveal` CLI reachability in `ctrl/tests/cli_contract.rs`. | accepted remotely |
| 5 | CLI supports structured input/output. | `--json` ActionResult tests plus generic JSON-object `action run` regression. | accepted remotely |
| 6 | Guided Action and selectable-input interaction works. | picker tests and descriptor input-selection metadata. | accepted remotely |
| 7 | Core Actions depend on Ports rather than preferred products. | machine/Work/recovery architecture; dependency guards on #15/#16/#17/#18; no provider IDs in canonical Action selection. | accepted remotely |
| 8 | Connectors register without a core source edit per Connector. | `ConnectorRegistry`; reference/template consumers; #18 `personal.git-sync` registers and is selected without changing a canonical Action. | accepted remotely |
| 9 | SDK lets a new Connector implement a Port and pass conformance. | #18 fresh-session Git `Synchronizer`; shared conformance + target-specific real Git acceptance. | accepted remotely |
| 10 | Personal extensions use only public SDK seams. | Runtime provider crates depend on `central-connector-sdk`; core-dependency guards fail on personal extension leakage. | accepted remotely |
| 11 | An external Surface invokes canonical Actions without duplicating them. | #15 `ctrl-macos`/Raycast descriptor-driven Surface and host-surface tests. | accepted remotely |
| 12 | Native OS automation can connect to canonical Actions. | #15 Shortcuts Connector plus Shortcut → `ctrl-macos action run` contract; real `/usr/bin/shortcuts` capability probe on hosted macOS. | accepted remotely |
| 13 | Machine inspection keeps authored and observed state distinct. | machine declaration/inspection/plan tests; Machine-declaration Skill workstation/server fixtures. | accepted remotely |
| 14 | Machine plan/apply/verify runs through replaceable Ports. | reference, macOS, Ubuntu and recovery tests exercise the same canonical Actions through different Connectors. | accepted remotely |
| 15 | A second materially different host preserves Action identity while using different Connectors. | hosted macOS and Ubuntu jobs; action-catalog equivalence/dependency guards. | accepted remotely; named machines still reserved |
| 16 | An agent can use the Connector-authoring Skill and SDK to create a conforming Connector. | #18 fresh-session black-box proof; no private integration guidance; Git target; #18 closed only after real mutation + canonical Action evidence. | accepted remotely |
| 17 | Control-maintenance procedure proposes durable source changes with provenance and human review. | #12 `control_maintenance_skill` tests + fixtures: clean/stale/conflict/misplaced-procedure/verification-preference. | accepted remotely |
| 18 | Restricted Control has a path excluded from agent retrieval. | #19 `.no-agent-retrieval` subtree treatment and `product_acceptance` regression; excluded subtree appears in `skipped_sources`. | accepted remotely |
| 19 | Deleting rebuildable `.central/` state does not delete authored source. | #19 `product_acceptance` regression deletes `.central/`, verifies Control/Work unchanged, then rebuilds local state. | accepted remotely |
| 20 | Removing an optional Connector does not break unrelated core functionality. | core package has no personal-provider dependencies; #15/#16/#17/#18 dependency guards; current core suite runs without those provider crates. | accepted remotely |

## Required Skill workflows

| Skill | Real workflow proof |
|---|---|
| `control-maintenance` | #12: realistic audit fixtures, source/diff/review contract, merged and green. |
| `machine-declaration` | #13: workstation + server environment review, eligible/missing Port handling, merged and green. |
| `connector-authoring` | #18: fresh agent/session boundary, public `Synchronizer` contract, new Git Connector, real provider and canonical recovery. |
| `connector-hardening` | #20 controlled Connector failure proof, then #18 independently re-used the Skill on a real shared-conformance gap and produced public regressions. |

## Documentation contract

`docs/CLI-REFERENCE.md` is the stock CLI contract. `ctrl/tests/public_docs.rs` requires every registered core Action to appear in it.

`docs/CONNECTOR-SDK-RUST.md` is the current executable Rust SDK reference. The same test requires every published core Port and its version to appear in it.

The broader normative architecture remains in `CENTRAL-SYSTEM-SPEC.md`, `CONNECTOR-SDK-SPEC.md`, `CONTROL-CONTENT-PROTOCOL.md`, `PERSONAL-EXTENSION-SPEC.md`, and `RECOVERY-PROTOCOL.md`.

## Physical/local evidence deliberately not claimed here

The following remain separate feature-ticket gates and are intentionally **not** converted into hosted evidence:

- #14: native open/reveal/tag behavior on the actual `primary-workstation`;
- #15: Raycast + Shortcuts two-direction workflow and configured hotkeys on the actual `primary-workstation`;
- #16: harmless Homebrew/chezmoi package/configuration acceptance on the actual `primary-workstation`;
- #17: harmless package/configuration/recovery acceptance on the actual `home-server`.

These should be run later as one O:I/local proving pass where practical. Their absence does not invalidate remote implementation/conformance work that can be proved on hosted real platforms, and #19 must not manufacture them.
