---
name: design-authoring
description: "Write or revise a Design document in Markdown — experience, behaviour, interaction, information relations, journeys, state, visual relation and the human/Agent encounter — mediating from Vision toward Mockup and Architecture. Use when how something should behave or feel must be made determinate. Not for why it exists (vision-authoring) or for contracts and ownership (architecture-authoring)."
---

# Design authoring

A Design turns Vision into determinate experience and behaviour. It mediates
in two directions: toward a **Mockup** that renders surfaces and states, and
toward an **Architecture** that owns the structure the behaviour requires.

## Form

Start from [assets/design-template.md](assets/design-template.md). Front
matter carries `role: design`, `standing`, `scope`, `vision_refs`,
`capability_refs` and `updated`. Sections cover experience, journeys,
behaviour and interaction, information relations, state, visual relation, the
human and Agent encounter, and the handoff toward Mockup and Architecture.
Omit a section that genuinely does not apply; do not pad it.

## Rules

1. **Answer a Vision.** Each Design unit links the Vision unit ids it realises.
   If no Vision exists, say so; do not invent one to cite.
2. **Behaviour, not code.** State what happens on each action and in each
   state. How it is built belongs to Architecture.
3. **Every state named.** Include empty, loading, degraded and failure states
   where they can occur, not only the happy path.
4. **Stable headings.** Heading slugs are unit ids that Mockup states
   (`data-design-ref`) and capability records (`design_refs`) cite.
5. **Standing.** Agent drafts are `agent-inference`; adopted designs are
   `design-commitment` only after recorded adoption.
6. **Human and Agent parity.** Say what each participant sees and can do;
   keep attribution visible.

## Verification

- Every `vision_refs` target resolves; every state in the table appears in the
  Mockup when one exists.
- `python3 tools/documentation_inventory.py <path>` reports role `design` and
  the expected unit ids.

## Return

Design changes return to the Mockup and Architecture they feed and, when they
reveal a gap in meaning, to Vision as a proposal.
