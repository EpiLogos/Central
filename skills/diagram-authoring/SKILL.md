---
name: diagram-authoring
description: "Write a Mermaid `.mmd` diagram source that answers one visual question for a companion document — context, topology, sequence, state, data flow or dependency — with stable node ids and labelled edges. Use when a relation is easier to inspect as a picture. Not for the document the diagram serves, and not for hand-drawn SVG that has no `.mmd` source."
---

# Diagram authoring

A diagram answers **one visual question** for a companion document. The `.mmd`
file is the source; SVG, PNG or HTML renders are projections of it and are
regenerated, never edited.

## Form

Pick the template that fits the question:

| Template | Question |
| --- | --- |
| [context.mmd](assets/context.mmd) | Who and what surrounds the system? |
| [topology.mmd](assets/topology.mmd) | Which components exist and how are they connected? |
| [sequence.mmd](assets/sequence.mmd) | In what order do participants exchange messages? |
| [state.mmd](assets/state.mmd) | Which states can an object be in? |
| [dataflow.mmd](assets/dataflow.mmd) | How does data move to durable state and out? |
| [dependency.mmd](assets/dependency.mmd) | What depends on what? |

Every file begins with a `%%` header block:

```text
%% source_id:        stable id of this diagram
%% visual_question:  the one question it answers
%% companion_doc:    the document unit it serves (file#unit)
%% revision:         date or sha256 of the companion unit it was drawn against
%% provenance:       author / Agent and standing
```

## Rules

1. **Stable semantic node ids** (`comp_core`, `store_ledger`), never `A`, `B`.
   Node ids are unit ids that other documents and the inventory can cite.
2. **Every edge labelled** with the relation it asserts.
3. **One question per file.** A second question is a second diagram.
4. **The companion doc must stand without it.** The diagram illustrates; the
   prose carries the claim.
5. **Revision tracks the companion.** When the companion unit changes, review
   the diagram and update `revision`.

## Verification

Render it (`mmdc -i file.mmd -o file.svg` where available, or any Mermaid
preview) and check it parses; check the header fields are filled;
`python3 tools/documentation_inventory.py <path>` reports role `diagram` and
the node ids.

## Return

A diagram that no longer matches its companion returns to the companion's
owner; the diagram follows the document, never the reverse.
