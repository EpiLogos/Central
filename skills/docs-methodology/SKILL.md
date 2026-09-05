---
name: docs-methodology
description: "Author and organise durable documentation per the six-rung standing ladder — claim discipline (receipts for present-tense claims), drift detection, layered return — so generated prose never silently becomes the person's ground."
---

# Docs methodology

Use this Skill when authoring or reorganising durable documentation across the suite — README, ADRs, wayfinders, canon, ledger reports — so that what is written carries its actual standing instead of silently promoting itself.

This is the **authoring side** of the ladder. `documentation-standing` audits and compares claims that already exist; this Skill governs writing new material so the audit has something honest to audit.

## The ladder (author against it)

```text
authored position      why this is worth doing and what must not be lost
design commitment      what the product is intended to become and which distinctions are deliberate
architecture contract  how the present system must remain true in form
implementation fact    what is actually implemented
observed evidence      dated receipts for present-tense claims
agent inference        generated synthesis — stays inference until separately established
```

## Claim discipline

1. **Present-tense capability claims need receipts.** "The suite verifies" requires a dated receipt (CI run, test output, acceptance record). Without one, write the claim at its actual standing: design commitment, or inference.
2. **Closures are timestamp-bound.** "Closed as complete on \<date\>" survives; "is complete" does not. Never write a standing property for a closure — write the closure *and* the date.
3. **Aggregate counts are not set-completeness.** "269/269 tests pass" does not prove the selection was complete. A verification report states what was selected and what was not.
4. **Implementation language is part of acceptance.** A claim inherits the programme that produced it; a voided programme voids its claims (asd#116).
5. **Physical acceptance is no parking reason** (O-I#97, 2026-08-23). "Needs physical acceptance" does not justify parking otherwise-complete work off main.
6. Laws 2–5 are the corpus's recovered receipt laws — cite them, don't paraphrase them into something weaker.

## Authoring laws

1. **Receive before determining.** Recover the smallest sufficient actual source set before strong claims; follow provenance upward only as far as the task needs.
2. **Filename is not standing.** `VISION.md`, `ARCHITECTURE.md`, `LEDGER.md` gain no standing from their names.
3. **Do not manufacture layers.** Missing intermediates are disclosed as absent, never invented to complete the ladder.
4. **Drift is a relation, not a verdict.** Report drift with both sides at their standing and revision; do not collapse it into "latest wins."
5. **Promotion is an event, not prose.** No generated text promotes itself: evidence exists by observation, implementation by landing, contracts by adoption, authored meaning by human adoption. When proposing a change, target the owner-native layer actually under pressure.

## Layered return

A useful doc (or audit comment) returns the compact relation:

```text
SUBJECT
  Authored position / design commitment / architecture contract /
  implementation fact / observed evidence / agent inference
CURRENT DEVELOPMENT STATE
  branch / PR / issue / main state
DRIFT / PRESSURE
  only relations actually established by the recovered field
RETURN
  owner-native consequence or proposal
```

`unresolved` rows stay `unresolved` — do not fill absent rows with invented artifacts.

## Ownership

Central owns durable source identity, provenance and standing relations. AIKit owns runtime source selection and precedence. Factory consumes the resolved developmental condition. O:I surfaces composition. This Skill teaches authoring discipline; it does not move ownership of any product's docs into Central.
