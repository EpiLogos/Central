---
name: evidence-led-repair
description: "METHOD: Start from evidence that contradicts a documented determination, find the nearest layer it actually pressures, repair that layer (implementation, Architecture, Design or Vision), verify and Return. Use when observed behaviour and documentation disagree. Not for a bug with an obvious local cause and no documentation relation."
---

# Evidence-led repair

Oriented by [docs-methodology](../docs-methodology/SKILL.md). A composable
path, not mandatory stages: skip a step whose layer is not actually touched,
and say that you skipped it.

```text
Evidence → nearest pressured layer → repair Implementation/Architecture/Design/Vision as warranted → verification → Return
```

**Entry.** Observed evidence (a failing check, a user report, a runtime trace) contradicts something a document or capability record claims.

**Near-miss.** A defect whose cause and fix are local and touch no documented claim — fix it directly with no Method and no Vision load.

## Composes

- **Evidence** — preserve the failing observation first; [documentation-standing](../documentation-standing/SKILL.md) to read what the contradicted claim's standing actually is
- **nearest pressured layer** — walk `extensions.documentation` refs upward from the capability record; stop at the first layer whose claim is actually wrong
- **repair** — code, or the matching form Skill: [architecture-authoring](../architecture-authoring/SKILL.md), [design-authoring](../design-authoring/SKILL.md), [vision-authoring](../vision-authoring/SKILL.md)
- **verification** — replay the original observation; `skill/aikit/verification`
- **Return** — name which layer was wrong and why

Retrieve with the compact inventory and a Jev attention question before
loading document bodies ([docs-methodology](../docs-methodology/SKILL.md#attention-and-retrieval)).
Agent-authored Vision, Design, Mockup or Architecture candidates stay at
`agent-inference` standing until a human recognises them.

## Verification

The original failing observation now passes when replayed; the repaired layer's claim and the evidence agree; no assertion was weakened to turn red green.

## Return

Code repairs land through review; Design/Vision repairs go to the human as proposals with the evidence attached.
