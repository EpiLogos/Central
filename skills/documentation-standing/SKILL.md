---
name: documentation-standing
description: Recover, compare, audit and manage Project documentation through the canonical authored-position → design-commitment → architecture-contract → implementation-fact → observed-evidence → Agent-inference standing ladder without collapsing it into the P0–P5 Project-act cycle.
---

# Documentation Standing Steward

Use this Skill when an Agent is recovering, comparing, explaining, auditing or proposing changes to Project documentation and source claims across O:I products.

## Purpose

Preserve the actual layered relation between authored meaning, adopted design, architecture, implementation, evidence and Agent inference while working in heterogeneous existing Projects.

The canonical standing ladder is:

```text
authored position
    ↓
design commitment
    ↓
architecture contract
    ↓
implementation fact
    ↓
observed evidence
    ↓
Agent inference
```

Machine ids are defined by `docs/documentation-standing.v1.json`.

This ladder is a documentation/claim-standing relation. It is not the P0–P5 Project-act cycle and it is not a scalar runtime precedence score.

## Ground laws

1. Receive the Project before determining it. Recover relevant native source and current revision before making a strong claim.
2. Filename is a discovery hint, not standing. `VISION.md`, `ARCHITECTURE.md`, an ADR, diagram, test, Wiki page or HTML file gains no standing from its name or extension.
3. Standing, provenance, scope, temporal state, authority and runtime activation remain different dimensions.
4. `current-development-state` is temporal/lifecycle context, not a documentation standing.
5. P0 Ground → P5 Return describes how context participates in an act. It must not be used to flatten documentation into P1 `World`.
6. Agent synthesis remains `agent-inference` until a different determining relation is actually established.
7. Retain native source in place whenever stable refs and relations are sufficient. Do not reorganise a Project merely to make this Skill work.

## Standing meanings

### Authored position — `authored-human-position`

Use only for a recognised human-authored or human-adopted determination of Project meaning at a declared scope: founding position, purpose, value, intended experience, stabilised vision or another determination for which the author remains responsible.

Do not infer this standing from prose tone or a human-looking filename.

### Design commitment — `design-commitment`

Use for an adopted design determination that says how authored meaning is intended to be realised. A draft design, generated mockup or unaccepted proposal remains at its actual provenance/standing.

### Architecture contract — `architecture-contract`

Use for a structural relation implementation is expected to satisfy: interface, protocol, schema, invariant, ownership boundary or accepted architecture relation.

A diagram that merely describes current code is not automatically a contract.

### Implementation fact — `implementation-fact`

Use for revision-specific current source/executable reality: code path, schema, configuration, merged behaviour encoded in source or another directly inspectable implementation fact.

Implementation does not retroactively define product meaning or architecture authority.

### Observed evidence — `observed-evidence`

Use for an identifiable returned result: test/CI result, Run receipt, benchmark, measurement, physical/local acceptance result, deployment observation or human-observed behaviour.

Test source is implementation; a concrete test result is evidence.

### Agent inference — `agent-inference`

Use for generated interpretation, synthesis, diagnosis, hypothesis, Wiki synthesis, research synthesis or proposed relation.

Inference may cite and accurately restate higher-standing sources. Its own novel connective claims remain inference unless separately established.

## Operating procedure

### 0. Recover

Determine the subject and retrieve the smallest sufficient actual source set.

When product meaning is material, begin from the current authored positions or their canonical successor and follow provenance toward design, architecture, implementation and evidence as required.

For a narrow implementation question, implementation/evidence may be sufficient. Do not load all six standings ritualistically.

### 1. Identify claims, not just files

For every materially important claim, retain at least:

```text
source_ref / path
revision or current identity
provenance
standing
scope
current temporal applicability
relation to neighbouring claims
```

A single file may contain multiple claims with different standings.

### 2. Follow the relation

Read vertically where the task requires it:

```text
authored position
    → what was designed from it?

design commitment
    → what architecture carries it?

architecture contract
    → what implementation realises it?

implementation fact
    → what evidence shows what actually happened?

observed evidence
    → what inference is warranted, and what upstream source is pressured?
```

Do not manufacture missing intermediate layers merely to complete the ladder.

### 3. Detect drift

Report a drift relation when actual sources establish it, for example:

```text
design commitment != current architecture
architecture contract != current implementation
implementation claim != observed evidence
Wiki inference stale against current source
current development account stale against branch/PR/main reality
```

State both sides with their standing and revision. Do not collapse the conflict into “latest wins” or “highest wins.”

### 4. Keep temporal state orthogonal

Track branch, PR, issue, Focus, NOW, merged/open/blocked/superseded and exact-main state separately.

Example:

```text
claim: native suite passed
standing: observed-evidence
source: CI run / receipt
temporal state: PR #165 at commit abc123 is current and open
```

Never encode the temporal line as `current-development-state` standing in new reasoning.

### 5. Compose the situated act separately

Only after standing is understood, attach P0–P5 participation where useful:

```text
standing: architecture-contract
act position: P0 relevant inherited Ground

standing: implementation-fact
act position: P1 encountered World

standing: observed-evidence
act position: P5 Return
```

The same standing can participate at different P positions in different Context Frames.

### 6. Return with provenance intact

For substantive project work, distinguish explicitly between:

```text
authored position
design commitment
architecture contract
implementation fact
observed evidence
current development state
Agent inference
```

The first six-line ladder above contains the documentation standings; `current development state` is listed here only as an orthogonal report dimension.

When proposing a change, target the owner-native layer actually under pressure. Do not rewrite authored meaning because code changed; do not rewrite architecture because one test failed; do not rewrite evidence into an inference.

## Promotion and Recognition

No generated text promotes itself.

A new relation is established only by the event appropriate to that standing:

```text
inference -- concrete observation --> evidence exists
inference/proposal -- implementation --> implementation fact exists
proposal -- architecture adoption --> architecture contract exists
proposal -- design adoption --> design commitment exists
proposal -- human authorship/adoption --> authored position exists
```

The original inference remains attributable as the source/proposal where material.

## Agent Wiki treatment

The Agent Wiki is a developed knowledge surface, not one standing.

- Direct references to authored/design/architecture/implementation/evidence sources should retain those source standings.
- New Wiki synthesis is `agent-inference`.
- A Wiki record may be updated as implementation/evidence changes when Wiki authority permits.
- Wiki text does not silently become the owner-native architecture, design or authored source it describes.

## Convention files and harness instructions

`AGENTS.md`, `CLAUDE.md`, `GEMINI.md` and equivalent files have two independent questions:

```text
What is their semantic/provenance/standing relation?
How did the harness make them operative?
```

Harness-native auto-loading does not establish authorship, standing or Central recognition. AIKit ContextResolution/Explain owns operative accounting.

## Management outputs

A useful standing-aware audit or bot report should prefer a compact relation such as:

```text
SUBJECT
  Authored position:      <source/ref or unresolved>
  Design commitment:      <source/ref or unresolved>
  Architecture contract:  <source/ref or unresolved>
  Implementation fact:    <source/ref or unresolved>
  Observed evidence:      <source/ref or unresolved>
  Agent inference:        <explicit synthesis, if any>

CURRENT DEVELOPMENT STATE
  <branch/PR/issue/main/Focus state>

DRIFT / PRESSURE
  <only relations actually established by the recovered field>

RETURN
  <owner-native consequence or proposal>
```

Do not fill absent rows with invented artifacts. `unresolved` means retrieval or Recognition is still required.

## Ownership

Central owns durable source identity, provenance, standing relations, scope and source lifecycle. AIKit owns runtime source selection, progressive disclosure and precedence. Factory consumes the resolved developmental condition and returns implementation/evidence. Actuation owns situated Agency/Return semantics. Workcell owns material execution lifecycle. O:I surfaces own explicit presentation/projection relations.

This Skill makes those products mutually legible; it does not move their ownership into Central.