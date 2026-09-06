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

## Opt-in sixfold product-ground authoring

Use this mode when the human chooses the sixfold consolidation convention. The Central pilot keeps one primary source collection at `ProjectCentral/user/central.html`, with its capability/relation matrix in companion MD and CSV. This is an optional authored arrangement, not a required ProjectCentral schema or an instruction to create human documents during init.

- **Compose before multiplying files.** Locate each determination within the six recursive composition coordinates. The thirteen document vessel types classify what a unit contains; claim standing records its authority. Neither replaces the sixfold, and product positions or developmental traversal numbers must not be silently substituted for composition coordinates.
- **Begin with the whole-account seed.** In the Central convention, the 0/1 overview is the canonical compact account: the human’s six answers to Why, What, How, Who/Whereby, Where/When and Why-For. Each answer opens into the corresponding expanded #0–#5 layer. Preserve supplied meaning and attribute editorial changes; keep generated elaboration distinct. The 0/1 is the whole anchor, not another numbered position. Record this question wording as the human-selected L0 application, rather than rewriting the shared MEF registry.
- **Reconcile from the seed.** When an overview answer changes, review its corresponding expanded layer, then follow affected capability and contract links. Record the seed revision only after reconciling the meaning. The Central validator detects seed changes against each expansion’s recorded hash; matching hashes establish a reviewed source basis, not semantic proof.
- **Open with the real vision.** The HTML’s #0 explains the need being met, why it matters and the intended human experience. Keep the capability matrix alongside the account as an addressable reference. Its proposed #0 vessel classification does not make matrix methodology the account’s opening content.
- **Recover functionality before composing.** Read wayfinders and development tickets together with later corrections in their comments. For each capability recover the need, native operation, useful result and limits; inspect current code and meaningful functional tests. Ticket closure alone does not establish implementation or acceptance. Reconcile this functional reading with the intended product before drafting.
- **Put capabilities in the matrix.** Give each capability a stable identity, need, operation, outcome, implementation status and source/code/test traces. Use [capability-matrices](../capability-matrices/SKILL.md) for the single CSV contract, declared views and seed × field profile. Reconcile existing relation records into that form; relational coverage and implementation status remain distinct. Maintain full MD/CSV parity and reciprocal links to the relevant account units. Intended or unresolved capabilities may have absent implementation/test links: mark that status explicitly, retain their real intention source and never invent code or evidence to fill the matrix.
- **Write plainly and sufficiently.** Explain what a person or Agent does, what changes and why it matters. Give workflows enough detail to direct development. Keep necessary refusals and unresolved differences in their bounded contract/review context; avoid repeating them throughout the product narrative.
- **Give each determination one home.** Use typed, stably addressable units within the collection, explicit relation meanings, `[[resource|label]]` links and useful tags. Reference another unit instead of repeating its account. Preserve these identities and relationships in any rendering.
- **Write development ground.** Articulate vision, intended UX/user stories, design decisions and architectural contracts. Code and functional tests establish executable facts; reference them for evidence instead of paraphrasing their implementation into a second specification.
- **Keep source and rendering distinct.** In this pilot the HTML collection is an editable source document, even when initially composed with agent assistance. A generated preview/export is a derived rendering and must point back to that source; do not maintain both as independent authoritative accounts.
- **Preserve actual standing.** New generated consolidation is `draft-for-review`, with generated provenance, even inside `ProjectCentral/user`. Human adoption is a separate recorded event. Authorization to draft does not ratify every resulting proposition.
- **Ground shared language by reference.** Read [O:I founding positions](../../../O-I/docs/positions/FOUNDING-POSITIONS.md) and [the Covenant's primitive relations](../../../O-I/docs/HUMAN-AGENT-COVENANT.md#7-one-covenant-many-first-class-primitives). Recover provenance from [Factory's original experienced ontology](../../../Software-Factory/docs/canon/QL-SOFTWARE-FACTORY-PRIMITIVE-RELATIONS.md), retaining its draft standing and checking newer native-owner contracts. Do not copy a competing suite ontology into Central.

Validate the Central pilot from its repository root with `python3 tools/check_product_ground.py`, then inspect the actual rendered account and its source relationships. Structural validation does not establish human adoption or prove a capability. For consolidation, preserve source meaning and identity while bringing selected carriers into the single protocol. Broader source retirement follows a successor review: Central owns the filesystem structure and AIKit owns the operations. The links and tags prepare future disclosure without making the wiki another authority in the development chain.

For continuing product development, use the capability-matrices Skill’s CLI-maintenance and directional reconciliation procedure. A completed feature change includes its discoverable command mapping or explicit composed/library boundary, dated execution reference, current code basis and reconciled account units. CI checks this maintained relationship against the actual product executable; the author reviews its meaning.
