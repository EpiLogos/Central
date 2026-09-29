---
name: reverse-recovery
description: "METHOD: Recover a missing or thin documentation field from authored fragments, history, implementation and evidence — through product understanding to candidate Vision, Design and Architecture, Recognition, and matrix/Wiki reconciliation. Use when a project has code but little trustworthy documentation. Not when adequate documents already exist."
---

# Reverse recovery

Oriented by [docs-methodology](../docs-methodology/SKILL.md). A composable
path, not mandatory stages: skip a step whose layer is not actually touched,
and say that you skipped it.

```text
authored fragments + history + implementation + evidence → product-understanding → candidate Vision/Design/Architecture → appropriate Recognition → established source field → matrix/Wiki reconciliation
```

**Entry.** The documentation field is absent, stale beyond use, or scattered, and work needs it.

**Near-miss.** Documents exist but lag one change (documentation-reconciliation).

## Composes

- **authored fragments + history + implementation + evidence** — tickets with their later comments, commit history, code, tests and receipts
- **product-understanding** — `skill/aikit/product-understanding` at the smallest sufficient depth
- **candidate Vision/Design/Architecture** — [vision-authoring](../vision-authoring/SKILL.md), [design-authoring](../design-authoring/SKILL.md), [architecture-authoring](../architecture-authoring/SKILL.md), [diagram-authoring](../diagram-authoring/SKILL.md) — every candidate at `agent-inference`
- **appropriate Recognition** — the human recognises Vision and Design; Architecture contracts are adopted by their owner
- **established source field** — only recognised material changes standing
- **matrix/Wiki reconciliation** — [capability-matrices](../capability-matrices/SKILL.md); Wiki learning through returns

Retrieve with the compact inventory and a Jev attention question before
loading document bodies ([docs-methodology](../docs-methodology/SKILL.md#attention-and-retrieval)).
Agent-authored Vision, Design, Mockup or Architecture candidates stay at
`agent-inference` standing until a human recognises them.

## Verification

Every recovered claim cites the fragment, commit, code or test it came from; nothing is marked above `agent-inference` without a recorded Recognition.

## Return

Candidates return to the human for Recognition; recovered understanding returns to the Wiki.
