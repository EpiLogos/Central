---
name: product-development
description: "METHOD: Carry new or changed product meaning from authored ground through Vision and Design to Mockup and/or Architecture, capability reconciliation, implementation, evidence and Return. Use when what the product is for or how it should work is changing. Not for a local fix or a purely structural change."
---

# Product development

Oriented by [docs-methodology](../docs-methodology/SKILL.md). A composable
path, not mandatory stages: skip a step whose layer is not actually touched,
and say that you skipped it.

```text
Ground → Vision → Design → Mockup and/or Architecture → Capability reconciliation → Implementation → Evidence → Return
```

**Entry.** Product meaning is new or changing: a new capability, a changed intended experience, or a Vision question the human has raised.

**Near-miss.** A change that alters no product meaning (use evidence-led-repair or no Method); a change that is only a surface (ui-development) or only structure (architecture-development).

## Composes

- **Ground** — recover with `skill/aikit/product-understanding`; do not write new ground yourself
- **Vision** — [vision-authoring](../vision-authoring/SKILL.md) — only the units that change
- **Design** — [design-authoring](../design-authoring/SKILL.md)
- **Mockup and/or Architecture** — [ui-mockup-authoring](../ui-mockup-authoring/SKILL.md), [architecture-authoring](../architecture-authoring/SKILL.md), [diagram-authoring](../diagram-authoring/SKILL.md) where a picture helps
- **Capability reconciliation** — [capability-matrices](../capability-matrices/SKILL.md) — add or revise records and `extensions.documentation` refs
- **Implementation** — the project's own engineering practice
- **Evidence** — `skill/aikit/verification` against the Vision/Design, not the code's own summary
- **Return** — as in [docs-methodology](../docs-methodology/SKILL.md#return)

Retrieve with the compact inventory and a Jev attention question before
loading document bodies ([docs-methodology](../docs-methodology/SKILL.md#attention-and-retrieval)).
Agent-authored Vision, Design, Mockup or Architecture candidates stay at
`agent-inference` standing until a human recognises them.

## Verification

Every new capability record cites its Design (and Vision where relevant); implementation links resolve; evidence is dated; `python3 tools/capability_matrix.py` passes.

## Return

Vision and Design changes go to the human for Recognition; capability and code changes land through the repository's review.
