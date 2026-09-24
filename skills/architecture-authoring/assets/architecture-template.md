---
role: architecture
standing: agent-inference
scope: "[system / subsystem this Architecture covers]"
design_refs: ["[design.md#unit-id]"]
diagram_refs: ["[diagrams/topology.mmd]"]
capability_refs: ["[cap.id]"]
updated: "[YYYY-MM-DD]"
---

# [System] Architecture

[One sentence: which structure this makes determinate and which Design it serves. The prose below must be intelligible without the diagrams.]

## Contracts

[Each interface, schema or protocol other parts rely on: its name, owner, shape and version. Link the code that implements it.]

## Ownership and boundaries

[Which component or product owns which concern, and what crosses each boundary.]

## Components

| Component | Owns | Depends on | Code |
| --- | --- | --- | --- |
| [name] | [responsibility] | [components] | [path] |

## Interfaces

[How components call each other: commands, APIs, files, events. Direction and failure behaviour for each.]

## State ownership

[Every durable or shared state, its single owner, and who may read or change it.]

## Data movement

[How data flows from input to durable state to output. Name transformations and where they happen.]

## Lifecycle

[Creation, update, retirement and recovery of the main objects.]

## Integration

[How this fits neighbouring systems; what they must provide; what this promises them.]

## Invariants

1. [A statement that must always hold, and where it is checked.]

## Degradation

[What happens when each dependency is slow, absent or wrong. What still works; what is refused; what is reported.]

## Structural authority

[Which decisions here are adopted contracts, which are current implementation fact, which are proposals. State the standing of each.]

## Open questions

[Undecided structure, left open.]
