---
name: ui-development
description: "METHOD: Develop an experienced surface from the relevant Vision through Design and Mockup to implementation and experiential verification. Use when the change is chiefly what a person sees and does. Not for backend-only structure or a cosmetic fix with no Design consequence."
---

# UI development

Oriented by [docs-methodology](../docs-methodology/SKILL.md). A composable
path, not mandatory stages: skip a step whose layer is not actually touched,
and say that you skipped it.

```text
relevant Vision → Design → Mockup → capability/Architecture where required → Implementation → experiential verification → Return
```

**Entry.** A surface, screen, state or interaction is being added or changed.

**Near-miss.** A pixel or copy fix that changes no behaviour (no Method); a change whose substance is structure (architecture-development).

## Composes

- **relevant Vision** — read only the Vision units the surface serves; do not revise Vision here unless the surface exposes a meaning gap
- **Design** — [design-authoring](../design-authoring/SKILL.md) — journeys and every state
- **Mockup** — [ui-mockup-authoring](../ui-mockup-authoring/SKILL.md) — all states traced
- **capability/Architecture where required** — [capability-matrices](../capability-matrices/SKILL.md), [architecture-authoring](../architecture-authoring/SKILL.md) when the surface needs new state or contracts
- **Implementation** — the product's UI code
- **experiential verification** — `skill/aikit/verification` — operate the real surface in each Mockup state, phone and desktop width
- **Return** — to Design when the built surface diverges from it

Retrieve with the compact inventory and a Jev attention question before
loading document bodies ([docs-methodology](../docs-methodology/SKILL.md#attention-and-retrieval)).
Agent-authored Vision, Design, Mockup or Architecture candidates stay at
`agent-inference` standing until a human recognises them.

## Verification

Each Mockup state is reachable in the implemented surface; degraded and failure states behave as designed; the check was performed on the running product, not only by tests.

## Return

Surface divergence returns to Design; any meaning gap returns to Vision as a proposal.
