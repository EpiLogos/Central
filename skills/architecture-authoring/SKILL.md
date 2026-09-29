---
name: architecture-authoring
description: "Write or revise an Architecture document in Markdown — contracts, ownership, boundaries, components, interfaces, state ownership, data movement, lifecycle, integration, invariants, degradation and structural authority. Use when structure must be made determinate or a structural decision recorded. Not for experience or behaviour (design-authoring) or for drawing alone (diagram-authoring)."
---

# Architecture authoring

An Architecture states how the system must remain true in form: who owns what,
what crosses which boundary, which invariants hold, and what happens when a
dependency fails. Its prose must be intelligible without its diagrams;
diagrams answer visual questions for it, they do not replace it.

## Form

Start from [assets/architecture-template.md](assets/architecture-template.md).
Front matter carries `role: architecture`, `standing`, `scope`, `design_refs`,
`diagram_refs`, `capability_refs` and `updated`.

## Rules

1. **Serve a Design.** Link the Design units whose behaviour the structure
   supports. Pure infrastructure with no Design says so.
2. **One owner per state.** Every durable or shared state has exactly one
   owner; name it.
3. **Contracts are named and located.** Each contract links the code that
   implements it; a contract without implementation is marked intended.
4. **Degradation is designed.** For each dependency say what still works when
   it is absent.
5. **Structural authority is explicit.** Distinguish adopted contracts
   (`architecture-contract`), current code (`implementation-fact`) and
   proposals (`agent-inference`). A diagram that merely describes current code
   is not a contract.
6. **Diagrams by reference.** Use diagram-authoring for any picture; cite it
   in `diagram_refs`.

## Verification

- Every code link resolves; every invariant names where it is checked.
- Read the document with diagrams hidden — it must still make sense.
- `python3 tools/documentation_inventory.py <path>` reports role
  `architecture`.

## Return

Implementation that contradicts the Architecture returns either as a code fix
or, if the contract was wrong, as an Architecture revision; a structural limit
that changes experience returns to Design.
