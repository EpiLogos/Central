---
role: architecture
standing: architecture-contract
scope: ledger storage
design_refs: ["design.md#state"]
diagram_refs: ["diagrams/topology.mmd"]
capability_refs: ["cap.ledger.record"]
---

# Ledger architecture

The ledger store has one writer and writes each record atomically. Readers never see a partial record.

## Contracts

`record(return) -> id | refusal`.

## State ownership

The store owns every record.
