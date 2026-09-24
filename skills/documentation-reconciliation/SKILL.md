---
name: documentation-reconciliation
description: "METHOD: After a source or evidence change, discover which documents and capability records depend on it, revise them with the relevant authoring Skills, reconcile the matrix and Wiki, verify and Return. Use when something upstream moved and its dependents must follow. Not for authoring new meaning."
---

# Documentation reconciliation

Oriented by [docs-methodology](../docs-methodology/SKILL.md). A composable
path, not mandatory stages: skip a step whose layer is not actually touched,
and say that you skipped it.

```text
changed source/evidence → affected-relation discovery → relevant authoring Skill(s) → capability/Wiki reconciliation → verification → Return
```

**Entry.** A document, code path, capability record or piece of evidence changed revision, and other documents cite it.

**Near-miss.** A new product decision (product-development); a contradiction found by evidence (evidence-led-repair).

## Composes

- **changed source/evidence** — note the old and new revision
- **affected-relation discovery** — `python3 tools/documentation_inventory.py` over the docs tree; follow relations and `extensions.documentation` refs; a Jev question per [jev-document-attention](../docs-methodology/references/jev-document-attention.md) when the set is large
- **relevant authoring Skill(s)** — only the form Skills for the affected roles
- **capability/Wiki reconciliation** — [capability-matrices](../capability-matrices/SKILL.md); Wiki learning through the NOW promotion path, never by editing `wiki.json`
- **verification** — structural validators plus a read of each revised unit against the changed source
- **Return** — list what was revised and what was deliberately left

Retrieve with the compact inventory and a Jev attention question before
loading document bodies ([docs-methodology](../docs-methodology/SKILL.md#attention-and-retrieval)).
Agent-authored Vision, Design, Mockup or Architecture candidates stay at
`agent-inference` standing until a human recognises them.

## Verification

Every dependent found by the inventory is either revised or recorded as unaffected with a reason; validators pass; recorded revisions match the new source.

## Return

Revisions to Agent-standing documents land directly; revisions touching authored meaning go to the human as proposals.
