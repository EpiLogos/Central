# Central — Project Context Protocol

**Status:** correction candidate over the merged Central #104 protocol  
**Suite Wayfinder:** EpiLogos/O-I#84  
**Builds on:** ProjectCentral authored Ground (#70), Agent governance (#72), temporal working field (#74), Project praxis/source work (#82)  
**Operational consumer:** AIKit ContextResolution / Knowledge / praxis  
**Development consumer:** Software Factory Project / Focus / Commission / Run / Return

## 1. Why this protocol exists

A Project contains several kinds of context which matter to humans and Agents for different reasons:

- human-authored purpose, founding positions, values and vision;
- design decisions and commitments;
- architecture contracts and structural constraints;
- current implementation, code, schemas and tests;
- observed Runs, test results and other evidence;
- Agent-maintained Wiki knowledge and derived inference;
- bounded current Intent, briefs and desired outcomes;
- human-authored Agent governance;
- Skills, Methods, capabilities, tools and learned praxis;
- temporal material such as NOW, DAY, Focus and current development state;
- harness-native instruction files such as `AGENTS.md` or `CLAUDE.md`;
- presentations such as HTML accounts and O:I `WorldPresentation` projections.

Central already preserves many of these distinctions separately. This protocol states how they compose without turning them into one source class, one prompt, one database or one mandatory document tree.

The primary law remains:

> **source role != source authority != disclosure / activation != runtime precedence**

This correction adds another equally important law:

> **documentation standing != Project-act position**

The merged #104 protocol accidentally made the P0–P5 Project-act movement carry too much of the documentation burden. P1 `World` then had to contain architecture, design, code, tests, evidence, constraints and Agent knowledge at once. That sixfold remains useful as an agency/context grammar, but it is not the documentation ladder.

For documentation, source recovery, claim management and bot reasoning, the canonical standing ladder is:

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

The standing of a claim is established by provenance, scope, owner recognition and the relation the source actually bears to the Project. A filename is only a discovery hint.

`VISION.md` does not become authored position because it is named `VISION.md`. An architecture diagram does not become an architecture contract because it is a diagram. A test file is not observed evidence until something was actually run or observed. An Agent Wiki page does not become implementation fact because it accurately describes code.

## 2. The documentation standing ladder

The standing ladder is the default grammar for understanding Project documentation and claims across O:I products.

It answers a different question from P0–P5:

```text
documentation standing
    What kind of determination or truth claim is this?

Project-act position
    What is this source doing in the situated act now?
```

A single source may contain claims at more than one standing. Classification should therefore be claim-sensitive where material rather than assigning one standing to an entire file merely for convenience.

### Authored position

An authored position states meaning for which a human or recognised author remains responsible at the relevant scope.

Typical material includes:

```text
founding positions
purpose / why
values
philosophical or conceptual determinations
intended human encounter
stabilised product vision
important refusals or non-negotiable judgements
```

It is authoritative for authored meaning at its scope. It is not evidence that a design exists or that an implementation currently realises that meaning.

Machine standing id: `authored-human-position`.

### Design commitment

A design commitment is a deliberately adopted determination of how an authored position is to be realised.

Typical material includes:

```text
accepted product/design decisions
interaction or experience commitments
data-model or workflow decisions before structural contract
accepted ADR decisions where the decision itself is the important claim
explicitly adopted design constraints
```

A design document, mockup, prototype or ADR is only a design commitment where its relevant claim has actually been adopted at that scope.

Machine standing id: `design-commitment`.

### Architecture contract

An architecture contract is a structural relation that implementations and integrations are expected to satisfy.

Typical material includes:

```text
public interfaces and protocols
schemas and invariants
ownership boundaries
component relations
accepted architecture diagrams
compatibility contracts
cross-product handoff contracts
```

Architecture is not merely an explanatory drawing of current code. A contract has a determining relation to implementations at its scope.

Machine standing id: `architecture-contract`.

### Implementation fact

An implementation fact states what the current executable/source world actually contains or does at a specific revision.

Typical material includes:

```text
current code paths
current schema or configuration values
current CLI/API behaviour encoded in source
merged implementation state
exact revision-specific capability
```

Code can establish implementation fact without becoming the source of product purpose or retroactively redefining an architecture contract.

Machine standing id: `implementation-fact`.

### Observed evidence

Observed evidence records what actually happened when a system, user, Run, test or environment returned a result.

Typical material includes:

```text
test / CI result
Run receipt
benchmark or measurement
physical/local acceptance result
user-observed behaviour
deployment or environment observation
failure log tied to a concrete execution
```

Evidence can support or contradict an implementation claim or architecture expectation. It is not lower-value merely because it appears later in the ladder; the ladder is a provenance/standing relation, not a simplistic precedence ranking.

Machine standing id: `observed-evidence`.

### Agent inference

Agent inference is a derived interpretation, synthesis, hypothesis, diagnosis or proposed relation produced from other sources.

Typical material includes:

```text
Agent Wiki synthesis
cross-source summary
inferred architecture not yet adopted as contract
diagnosis of likely cause
research synthesis
proposed design or source change
```

Inference can be excellent and highly useful. Its standing remains inference until the relevant determining relation changes through observation, implementation, contract adoption, design adoption or authorship.

Machine standing id: `agent-inference`.

## 3. Standing is not filename, freshness, role or precedence

The ladder is useful only if adjacent dimensions remain distinct.

### Filename / semantic role

Filename and content can provide role hints such as:

```text
vision
intent
architecture
design
research
prototype
governance
wiki
run-report
```

Those hints drive discovery. They do not establish standing.

Examples:

```text
VISION.md
    may contain authored position, design commitment, an unadopted draft,
    or a generated proposal

ARCHITECTURE.md
    may contain an architecture contract, a descriptive implementation account,
    or an Agent inference

prototype.html
    may be an authored desired-experience source, a design commitment,
    a generated presentation, or ordinary implementation

Agent Wiki page
    may faithfully cite implementation facts and evidence while its own synthesis
    remains Agent inference
```

### Current development state

`current-development-state` is **not a seventh documentation standing**.

Current development state is temporal/lifecycle context:

```text
active branch / PR / issue
current Focus / Commission
merged / open / draft / blocked / superseded
current-main revision
local-vs-remote convergence state
work presently in flight
```

It belongs with scope/temporal applicability and lifecycle/Return. A statement such as “PR #165 is green” still needs a substantive standing for the underlying claim — normally observed evidence — plus temporal state saying which PR/revision is current.

Existing serialized `current-development-state` values are a compatibility concern for the current Central source-relation implementation; new documentation reasoning must not treat that legacy value as a rung in the ladder.

### Runtime precedence

A higher standing is not automatically injected earlier or allowed to override every other source. AIKit owns operative ContextResolution and precedence. Standing tells the resolver and Agent what kind of claim it is handling; it does not replace situated resolution.

### Conflict is relational, not “highest wins”

The ladder is not a scalar trust score.

Useful conflicts include:

```text
authored position ↔ design commitment
    Is the adopted design still true to what the Project means?

design commitment ↔ architecture contract
    Does the structural contract actually express the accepted design?

architecture contract ↔ implementation fact
    Does current code satisfy the contract?

implementation fact ↔ observed evidence
    Did the implementation behave as its source/state suggested?

observed evidence ↔ Agent inference
    Does the interpretation actually follow from what was observed?
```

A bot should preserve both sides of a real conflict and name the drift. It should not erase returned reality because an upstream source is more authoritative for a different question.

## 4. The P0–P5 Project-act movement

P0–P5 remains the canonical grammar for how a Project field participates in a situated act:

```text
P0 — GROUND
    durable originating horizon relevant to the act
        ↓
P1 — WORLD
    the Project/world encountered for the act
        ↓
P2 — PRAXIS
    developed ways and powers of acting
        ↓
P3 — INTENT / DETERMINATION
    bounded present determination
        ↓
P4 — CONTEXT FRAME
    situated composition for this act
        ↓
P5 — RETURN / RECOGNITION
    what the act and world return
        ↺
    renewed Ground / World / Praxis / Intent
```

These positions are **not documentation levels, required directories or exclusive file classes**. They answer what a distinction is doing in the contextual whole.

A design commitment can participate at P0 when it is part of the relevant inherited ground for a narrow implementation act. The same design commitment can participate at P3 when the present task is explicitly to realise it. An implementation fact can participate at P1 as encountered World and at P5 when a just-completed implementation is the returned reality of the Run. Observed evidence often participates at P5 but may be part of P1 for the next act. Agent inference can appear in P1 as available knowledge without becoming implementation truth.

This is why the two sixes must not be collapsed.

### QL compatibility

The P-cycle remains deliberately compatible with QL's dynamic grammar without making QL-MEF a dependency of ordinary Project correctness.

A useful conjugate reading remains:

```text
P0 Ground        ↔ P5 Return / Recognition
P1 World         ↔ P4 Context Frame
P2 Praxis        ↔ P3 Intent
```

P4 remains the recursive aperture. A resolved frame may expose:

```text
4.0 — relevant Ground
4.1 — relevant World / current reality
4.2 — relevant Praxis / capabilities / governance
4.3 — active Intent / Commission
4.4 — situated actor · authority · temporal · harness composition
4.5 — expected and actual Return / evidence / Recognition path
```

That recursive agency grammar is complementary to documentation standing. It does not classify architecture, code, evidence or inference into P-level documentation buckets.

## 5. The Context Source Form

Every materially relevant context source or claim should be describable through the following dimensions. This is a protocol/form, not a requirement to serialise every field for every trivial source.

### C0 — Identity / provenance

```text
source_ref
native owner
native path / URI
revision / digest where material
authorship/provenance
    human-authored
    human-adopted
    Agent-maintained
    generated
    observed
    inferred
    unresolved
```

Question: **what is this source and where did it come from?**

### C1 — Role / documentation standing

```text
semantic roles
standing
    authored-human-position
    design-commitment
    architecture-contract
    implementation-fact
    observed-evidence
    agent-inference
    unresolved
```

Question: **what kind of determination or truth claim is this here?**

`current-development-state` is not part of this list. P0–P5 may be attached separately as situated participation.

### C2 — Scope / relation / temporal state

```text
suite / product / Project
subtree / module / subject
Focus / task / Commission
temporal applicability / current-development-state
actor / Agency / harness applicability
relations to neighbouring sources / claims / code / Runs
```

Question: **where, when and in relation to what does it apply?**

### C3 — Authority / mutation

```text
source owner
source treatment
retain-native-in-place / ProjectCentral / derived
who may revise
who may adopt
whether Recognition is required
proposal / acceptance boundary
```

Question: **who determines this source and how may it change?**

### C4 — Availability / disclosure / activation

```text
source exists
    ↓
Agent-readable / eligible
    ↓
available to resolver / harness
    ↓
selected
    ↓
retrieved / projected / harness-auto-loaded
    ↓
materially active in this act
```

Where relevant, an activation receipt should retain loader/resolver, harness, scope, selection reason, activation mode, runtime precedence explanation and whether O:I selected the source.

Question: **how, if at all, did this source become operative now?**

Central owns source existence, durable identity and its own retrieval/privacy eligibility. AIKit owns runtime composition, progressive disclosure and ContextResolution/Explain. Native harnesses may independently auto-load convention files; AIKit should account for that fact rather than attribute the activation to Central.

### C5 — Return / lifecycle

```text
supersedes / superseded-by
current / stale / conflicted / blocked / closed
Run / Claim / Evidence refs
returned pressure
proposal
Recognition result
accepted / rejected / narrowed / deleted revision
```

Question: **what has reality returned about this source or its use, and what happened next?**

## 6. Convention files

Well-known filenames are useful because humans and harnesses can discover them cheaply. They remain signals, not constitutional authority.

| Convention / source | Useful role hint | Standing must be established from |
| --- | --- | --- |
| `FOUNDING-POSITIONS.md` | founding position | authorship/adoption + scope |
| `VISION.md` | vision / intended horizon | provenance + actual determining relation |
| `intent.md`, `INTENT.md`, briefs | bounded Intent | provenance + present scope; often authored position or design commitment, but not by name |
| README / design docs / ADRs | explanation / design | adoption and relation to implementation |
| architecture docs / diagrams | architecture | accepted contract relation, not drawing format |
| source code / schemas | implementation | exact revision and executable/source ownership |
| test source | verification mechanism | code alone is implementation; a concrete result is evidence |
| CI / Run / acceptance receipt | evidence | identifiable execution / observation |
| Agent Wiki / ProjectMap | Agent knowledge | source-linked claims plus Agent-inference standing for synthesis |
| `AGENTS.md`, `CLAUDE.md`, equivalent | possible governance / local contract | provenance/recognition independently of harness activation |
| NOW / DAY / Focus / Commission | current development state / intent | temporal scope, not a standing rung by filename |
| HTML / `WorldPresentation` | presentation | provenance + source relation; extension is orthogonal to standing |

A source can legitimately participate in more than one semantic role and Project-act position. Where one file contains materially different claims, the bot should preserve claim-level distinctions rather than force the whole file into one standing.

## 7. Bot retrieval and documentation management

Central should make the standing relation easy for a bot to use across heterogeneous existing Projects without forcing migration.

### Receive before determining

For a substantive Project question or mutation, recover enough of the actual field before synthesising:

```text
question / requested change
    ↓
recover relevant native sources and current revisions
    ↓
identify provenance + standing + scope for material claims
    ↓
follow relations between standings
    ↓
inspect current development state separately
    ↓
resolve the smallest sufficient situated Context Frame
    ↓
act / return with source refs and standing-preserving evidence
```

The bot should not begin from a generic documentation taxonomy and then search for files to fill it.

### Retrieval by question

The ladder informs retrieval depth, not mandatory prompt size.

```text
product meaning / why
    authored position → design commitment as needed

design decision
    authored position ↔ design commitment → relevant architecture

architecture question
    design commitment ↔ architecture contract ↔ current implementation

implementation / bug question
    implementation fact ↔ observed evidence
    + upstream contract/design only where materially relevant

cross-source synthesis / recommendation
    recover the standings actually needed
    → return new synthesis explicitly as Agent inference
```

### Drift detection

The bot should be able to report standing-sensitive drift such as:

```text
design not represented by architecture
architecture contract not realised by current code
implementation claim contradicted by test/Run evidence
observed result not yet reflected in current-state account
Agent Wiki inference stale against current implementation
a generated proposal presented as though it were adopted design
```

Drift is a relation to surface, not permission to silently rewrite the upstream source.

### Promotion / adoption law

No source silently promotes itself up the ladder.

```text
Agent inference
    -- observation --> may be supported by observed evidence
    -- implementation --> may become true in code as implementation fact
    -- architecture adoption --> may become architecture contract
    -- design adoption --> may become design commitment
    -- human authorship/adoption --> may become authored position
```

These arrows are changes in relation and authority, not automatic transformations of the original generated text.

Observed evidence likewise does not automatically become implementation fact; it may reveal what the implementation did in one concrete condition. Implementation fact does not automatically become architecture contract. Architecture does not retroactively author product meaning.

### Management actions

A documentation-aware bot may:

- discover and classify candidate sources conservatively;
- attach or propose standing/provenance/scope relations;
- retain native source in place;
- identify unresolved standing rather than guessing;
- show conflicts and drift across adjacent standings;
- update Agent-maintained Wiki material when authority permits and current sources warrant it;
- propose owner-native changes at the standing where the correction belongs;
- retain exact source/revision/evidence refs for consequential returns.

It must not:

- infer authorship or adoption from filename, folder or prose style;
- rewrite authored position from implementation or evidence without Recognition;
- present an Agent inference as architecture/implementation/evidence merely because it is plausible;
- use P1 `World` as a documentation bucket that erases standing;
- use `current-development-state` as a substitute for documentation standing;
- force Projects into a new folder/file taxonomy to gain layered awareness.

## 8. Ambient harness context

Agent harnesses increasingly treat well-known files as extensions of their operative instruction field. That behaviour remains distinguishable from O:I/Central source recognition and from documentation standing.

A Project may truthfully have:

```text
source_ref                native / stable ref
path                      AGENTS.md
semantic_role             possible-project-agent-governance
standing                  unresolved
provenance                unresolved
central_recognised        false
agent_readable            true
harness                   codex
activation                harness-native-auto-loaded
runtime_precedence        harness-owned / explainable by adapter
ai-kit-selected           false
materially_active         true
```

`materially_active` does not mean `authoritative`, and neither property determines documentation standing.

## 9. Intent and returned development

A recognised bounded Intent should enter execution through existing identities rather than a new universal `IntentRef` database:

```text
recognised Intent source
        ↓ standing + SourceRef / ContextSourceRef
P3 developmental determination
        ↓
Factory Commission / Focus
        ↓
P4 situated Context Frame
        ↓
Run / development
        ↓
implementation facts + observed evidence
        ↓
P5 Return
        ├─ Agent inference / Wiki update where warranted
        ├─ praxis fitness evidence
        └─ owner-native proposal at the standing actually pressured
```

A workflow such as `intent → requirements/design → plan → implementation → verification` can therefore be consumed naturally without constitutionalising one vendor's artifact sequence. Its useful distinctions are represented by provenance, standing, scope and Return rather than filename mandate.

## 10. HTML and WorldPresentation

Presentation remains orthogonal to documentation standing and Project-act position.

```text
native / recognised source + standing + contextual relations
        ↓ explicit reading / projection
structured account / WorldPresentation
        ↓
HTML / desktop / Explore / another Surface
```

A `.html` or `.htm` path is a role hint. Explicit provenance and recognition determine whether its claims are authored position, design commitment, implementation, generated presentation or something else.

## 11. Ownership across O:I

### Central

Central owns the durable source side:

```text
source identity
provenance / human authorship / adoption
semantic role and documentation standing relation
scope / temporal applicability
ProjectCentral relations
Agent governance source
Agent Wiki filesystem identity
native-source recognition / retain-native-in-place
retrieval/privacy eligibility in Central
durable lifecycle / source-return proposal relation
```

Central describes the field; it does not resolve a runtime prompt.

### AIKit

AIKit owns:

```text
ContextSource integration
Knowledge traversal
standing-aware source selection where useful
Skill / Method / Profile resolution
progressive disclosure
Context Frame operational composition
harness adaptation
ambient native-instruction accounting
ContextResolution / Explain
```

AIKit may use standing to explain why a source was selected and what kind of claim it can support. It does not change the source's standing merely by activating it.

### Factory

Factory consumes stable refs and the resolved developmental condition:

```text
Project / Focus / Commission
materially consulted source refs + standings
current implementation state
Methods / Skills / capability condition
Run / Artifacts / Claims / Evidence
returned pressure against the standing actually implicated
```

Factory does not become source, Wiki, Skill or prompt authority.

### Actuation / Workcell / O:I surfaces

Actuation owns situated Agency and Return semantics. Workcell owns material execution lifecycle. O:I Projection/WorldPresentation owns explicit presentation/publication envelopes. Each can return evidence about context use without rewriting Central source authority.

## 12. Context economy

The protocol describes a whole larger than any one prompt.

```text
Project Context Field
    !=
load all Project context
```

The operating law is:

> **resolve the smallest sufficient Context Frame for the present act while retaining addresses and standings back into the larger Project field.**

Standing-aware retrieval should increase precision, not create a requirement to load all six standings for every task.

## 13. Conformance

A healthy implementation should prove at least these relations:

1. The exact six documentation standings are represented in order: authored position → design commitment → architecture contract → implementation fact → observed evidence → Agent inference.
2. `current-development-state` is represented as temporal/lifecycle context and not as a seventh standing in new reasoning/contracts.
3. P0–P5 remains available as a situated Project-act grammar and is explicitly orthogonal to documentation standing.
4. Filename/extension does not establish standing.
5. A mixed Project can distinguish a Vision/position, adopted design, architecture contract, current code, concrete test/Run evidence and Agent Wiki inference without moving them into required directories.
6. A conflict between architecture and code or between code and evidence is surfaced as drift rather than collapsed by scalar precedence.
7. Agent inference cannot silently promote to authored/design/architecture/implementation/evidence standing.
8. AIKit can progressively disclose the smallest sufficient sources while retaining standing/provenance in Explain.
9. Factory/Actuation/Workcell can return implementation/evidence pressure to the correct owner-native source without becoming source authority.
10. Existing native Projects remain valid; the pattern works by relation and provenance rather than migration.

## 14. Non-goals

This protocol does not create:

- six required documentation directories;
- a requirement that every file have exactly one standing;
- a scalar “authority score” where upstream always wins;
- a mandatory `INTENT.md`, `VISION.md`, `ARCHITECTURE.md` or documentation suite;
- a universal `ProjectContext` database;
- a Central-owned runtime precedence engine;
- a requirement for QL-MEF in ordinary operation;
- permission for Agent-generated interpretation to become human authorship automatically.

## 15. Compact form

For human and Agent inspection, the canonical compact readout is now two-dimensional:

```text
DOCUMENTATION STANDING

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

PROJECT ACT

P0 Ground
P1 World
P2 Praxis
P3 Intent
P4 Context Frame
P5 Return / Recognition
```

And every material source remains inspectable through:

```text
identity / provenance
role / standing
scope / temporal state / relations
authority / mutation
disclosure / activation
return / lifecycle
```

The result is a Project field in which a bot can recover meaning, design, structure, implementation, evidence and inference without flattening them into `World`, while still using the P-cycle to understand how those sources compose in an actual act.