---
name: docs-methodology
description: "METHODOLOGY: Orient in a project's documentation field — which kinds of document exist (Vision, Design, Mockup, Architecture, diagrams, capability matrix), what standing each claim carries, which documentation Method fits the act, how attention narrows before retrieval, and where Return goes. Use when work touches what a product is for, how it should feel or how it is structured. A small local repair selects none of this and loads no Vision."
---

# Documentation Field

This Methodology orients an Agent inside a project's documentation field. It
names the kinds of determination that exist there, how they relate, which
Method applies when, and how Return moves back through the field. It composes
and points; the form Skills and Methods it names carry their own bodies.

`documentation-standing` audits claims that already exist. This Methodology
governs how new and revised material is written and related so that audit has
something honest to audit.

## Field vocabulary

```text
authored ground     human-authored or human-adopted meaning (founding positions, seed answers)
Vision              why the product exists, what it is, the intended experience, deliberate distinctions, direction
Design              experience, behaviour, interaction, information relations, journeys, state, human/Agent encounter
Mockup              a standalone HTML rendering of surfaces and states, each state linked back to Design/Vision/capability
Architecture        contracts, ownership, boundaries, components, interfaces, state, data movement, invariants, degradation
diagram             a `.mmd` source answering one visual question for a companion document
capability field    the ql-capability-matrix/1 records: need, operation, outcome, status, evidence, relations
implementation      what the code does
evidence            dated observation that a claim holds
Return              what changed, and which earlier determination it pressures
```

A filename is a discovery hint, never standing. `VISION.md` is not Vision
because of its name; a unit is Vision because it answers Vision's questions at
a recognised standing.

## Constitutive relations

```text
Authored ground → Vision → Design → { Mockup, Architecture } ↔ Diagrams
                → Capability field → Implementation → Evidence → Return ↺
```

These are relational roles, not a file count. One HTML account may carry
ground and Vision together; one Design may have no Mockup; a diagram belongs to
whichever document asks its question. Missing roles are disclosed as absent,
never manufactured to complete the chain. Return re-enters at whichever role
it pressures.

Capability records connect the documents to code through the optional
`extensions.documentation` object (`vision_refs`, `design_refs`,
`mockup_refs`, `architecture_refs`, `diagram_refs`, `praxis_refs`, typed
`relations`) — see [the matrix protocol](../../docs/CAPABILITY-MATRIX-PROTOCOL.md#documentation-relations).
Capabilities stay capability-centred; there is no second documentation graph.

## Source and standing grammar

Every claim carries one standing on the six-rung ladder owned by
[documentation-standing](../documentation-standing/SKILL.md):
`authored-human-position`, `design-commitment`, `architecture-contract`,
`implementation-fact`, `observed-evidence`, `agent-inference`. Read that Skill
for meanings; do not restate them locally.

Standing, provenance, scope, lifecycle and runtime activation are separate
dimensions. Agent-authored Vision, Design, Mockup or Architecture candidates
stay `agent-inference` until a human recognises them. Promotion is an event —
evidence by observation, implementation by landing, contracts by adoption,
authored meaning by human adoption — never a change of wording.

### Claim discipline

1. **Present-tense capability claims need receipts.** "The suite verifies" requires a dated receipt (CI run, test output, acceptance record). Without one, write the claim at its actual standing: design commitment, or inference.
2. **Closures are timestamp-bound.** "Closed as complete on \<date\>" survives; "is complete" does not. Never write a standing property for a closure — write the closure *and* the date.
3. **Aggregate counts are not set-completeness.** "269/269 tests pass" does not prove the selection was complete. A verification report states what was selected and what was not.
4. **Implementation language is part of acceptance.** A claim inherits the programme that produced it; a voided programme voids its claims (asd#116).
5. **Physical acceptance is no parking reason** (O-I#97, 2026-08-23). "Needs physical acceptance" does not justify parking otherwise-complete work off main.
6. Laws 2–5 are the corpus's recovered receipt laws — cite them, don't paraphrase them into something weaker.

### Authoring laws

1. **Receive before determining.** Recover the smallest sufficient actual source set before strong claims; follow provenance upward only as far as the task needs.
2. **Filename is not standing.**
3. **Do not manufacture layers.** Missing intermediates are disclosed as absent, never invented to complete the ladder.
4. **Drift is a relation, not a verdict.** Report drift with both sides at their standing and revision; do not collapse it into "latest wins."
5. **Promotion is an event, not prose.** When proposing a change, target the owner-native layer actually under pressure.

## Methods and when each applies

Select at most the one Method the act calls for. Methods are composable paths,
not mandatory stages.

| Method | Select when |
| --- | --- |
| [product-development](../product-development/SKILL.md) | New or changed product meaning must travel from ground to code. |
| [ui-development](../ui-development/SKILL.md) | The change is chiefly an experienced surface. |
| [architecture-development](../architecture-development/SKILL.md) | The change is chiefly structure: contracts, ownership, boundaries. |
| [evidence-led-repair](../evidence-led-repair/SKILL.md) | Observed behaviour contradicts a documented determination. |
| [documentation-reconciliation](../documentation-reconciliation/SKILL.md) | A source or evidence changed and its dependent documents must follow. |
| [reverse-recovery](../reverse-recovery/SKILL.md) | The documents are missing or thin and must be recovered from code, history and fragments. |
| [experimental-development](../experimental-development/SKILL.md) | A bounded question is best answered by building a prototype first. |

**A small local repair selects none of these and loads no Vision.** A typo,
a failing test with an obvious local cause, a dependency bump or a rename that
changes no meaning goes straight to the code and its own verification.

## Skills and SkillSets

Form Skills (one per document role): [vision-authoring](../vision-authoring/SKILL.md),
[design-authoring](../design-authoring/SKILL.md),
[ui-mockup-authoring](../ui-mockup-authoring/SKILL.md),
[architecture-authoring](../architecture-authoring/SKILL.md),
[diagram-authoring](../diagram-authoring/SKILL.md).

Supporting Skills: [documentation-standing](../documentation-standing/SKILL.md),
[capability-matrices](../capability-matrices/SKILL.md); from AIKit
`skill/aikit/knowledge-navigation`, `skill/aikit/projection-authoring`,
`skill/aikit/verification`, and through the `aikit:account-authoring` child
set `skill/aikit/product-understanding`,
`skill/aikit/structured-account-authoring`, `skill/aikit/html-account`.

All of these are carried by the `central:documentation` SkillSet
([skillsets/index.toml](../../skillsets/index.toml)). Carrying a set means the
descriptions are reachable; bodies load only when a Method selects them.

## Attention and retrieval

Progressive disclosure is the default. Never load every document to find the
relevant one.

1. **Compact inventory first.** Run `python3 tools/documentation_inventory.py <paths>`
   for a `central.documentation-inventory/v1` listing: per source its path,
   sha256 revision, role, standing, determination (≤240 chars), relations,
   capability refs and unit ids. No bodies.
2. **Typed Jev question over the inventory.** Ask which sources are
   *required*, *supporting*, *jointly required* (only useful together) or
   *irrelevant*, and whether the catalogue is *insufficient* for the question.
   Question shapes are in
   [references/jev-document-attention.md](references/jev-document-attention.md).
   A Jev answer may name several complementary sources or report
   insufficiency; it must not manufacture a capability or collapse need,
   operation, outcome, standing, implementation status and readiness.
3. **Exact retrieval.** AIKit retrieves only the selected grains (unit ids) at
   the recorded revisions.
4. **Prepared context in NOW.** The prepared view lives in the existing Redis
   participant NOW via `aikit now-context prepare`; a later act on an
   unchanged basis reuses it warm. It is invalidated when a source revision,
   a dependency, a capability record or a Return changes. Redis holds a hot
   participant copy, never a new document owner.

For a full declared account or matrix scope, enumerate the declared inventory
before asking relevance questions. For smaller work, select only the needed
units while keeping explicit dependencies and jointly required capabilities.

## Transition grammar

Pressure moves to the layer that owns what it contradicts.

```text
new or changed intent                  → Vision (human recognition needed)
experience or behaviour question       → Design
surface or state not yet visible       → Mockup
contract, ownership or boundary change → Architecture (+ diagram if a picture answers it)
new operation or result                → capability field
code disagrees with Architecture       → Implementation, or Architecture if the contract was wrong
evidence contradicts a claim           → the nearest pressured layer (evidence-led-repair)
```

Reverse traversal is ordinary: implementation or a capability record may
pressure Architecture, Architecture may pressure Design, Design may pressure
Vision. Follow `extensions.documentation` refs upward from the capability
record; stop at the first layer whose claim actually changes.

## Verification

- Structural: `python3 tools/capability_matrix.py <manifest>` for matrix and
  documentation extensions; `python3 tools/check_product_ground.py` for a
  product account.
- Per document: every unit id referenced by a capability or Mockup state
  resolves; standing is stated; absent layers are stated as absent.
- Behavioural claims: verify with `skill/aikit/verification` against the
  original whole, not against a document's own summary.
- Structural validation never establishes human adoption or proves runtime
  behaviour.

## Return

A useful return states the compact relation:

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

`unresolved` rows stay `unresolved`. Classify the actual result only to decide
the next native operation:

- task-local → nothing durable;
- understanding → Wiki return through the NOW promotion path;
- reusable practice → revise the Skill or Method;
- document → propose or perform the scoped change in the owning document,
  preserving unknown fields and companion-file parity;
- authored meaning → a proposal for human Recognition.

A Jev determination is evidence for that decision, not authorship.

## Composition with Wayfinder

Wayfinder and this Methodology compose in parallel over one developmental act;
neither sits above the other. Wayfinder determines the developmental field —
destination, frontier, ownership, evidence, closure. This Methodology
determines the representation field — Vision, Design, Mockup, Architecture,
diagrams, capability relations. A Wayfinder frontier may call for a Method
here; a documentation Return may move a Wayfinder frontier. Each keeps its own
source.

## Ownership

Central owns durable source identity, provenance and standing relations. AIKit
owns runtime source selection and precedence. Factory consumes the resolved
developmental condition. O:I surfaces composition. This Methodology does not
move ownership of any product's docs into Central.

## Opt-in sixfold product-ground authoring

When the human has chosen the sixfold consolidation convention (one seed-led
HTML account plus companion capability matrix, as in
`ProjectCentral/user/central.html`), follow
[references/sixfold-product-ground.md](references/sixfold-product-ground.md).
It is an optional authored arrangement, not a required schema.
