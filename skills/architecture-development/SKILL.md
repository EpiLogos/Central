---
name: architecture-development
description: "METHOD: Develop structure — contracts, ownership, boundaries, invariants — from the relevant Vision/Design through Architecture, diagrams where useful, capability reconciliation, implementation and verification. Use when a structural decision is being made or changed. Not for a local refactor with no contract change."
---

# Architecture development

Oriented by [docs-methodology](../docs-methodology/SKILL.md). A composable
path, not mandatory stages: skip a step whose layer is not actually touched,
and say that you skipped it.

```text
relevant Vision/Design → Architecture → Diagram where useful → capability reconciliation → Implementation → verification → Return
```

**Entry.** A contract, ownership boundary, state owner, integration or invariant is being introduced or changed.

**Near-miss.** A refactor inside one owner that changes no contract (no Method); an experience change (ui-development).

## Composes

- **relevant Vision/Design** — read the units the structure serves
- **Architecture** — [architecture-authoring](../architecture-authoring/SKILL.md)
- **Diagram where useful** — [diagram-authoring](../diagram-authoring/SKILL.md)
- **capability reconciliation** — [capability-matrices](../capability-matrices/SKILL.md) — `architecture_refs`, `diagram_refs`, relations such as `implements`, `requires`
- **Implementation** — the owning repository
- **verification** — `skill/aikit/verification`; contract tests at the boundary
- **Return** — as in [docs-methodology](../docs-methodology/SKILL.md#return)

Retrieve with the compact inventory and a Jev attention question before
loading document bodies ([docs-methodology](../docs-methodology/SKILL.md#attention-and-retrieval)).
Agent-authored Vision, Design, Mockup or Architecture candidates stay at
`agent-inference` standing until a human recognises them.

## Verification

Each named contract links implementing code and a test at its boundary; invariants name where they are checked; the Architecture reads without its diagrams.

## Return

Adopted contracts are recorded as `architecture-contract` only on adoption; structural limits that change experience return to Design.
