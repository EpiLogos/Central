# ProjectCentral NOW / DAY

**Status:** Central temporal source/retention contract for Central #74  
**Scope:** optional Project-local human↔Agent working field  
**Ordinary correctness:** no QL, AIKit, Factory, O:I, database, session or chat dependency

## Why this exists

ProjectCentral already distinguishes durable human-authored Project ground from Agent-maintained Wiki knowledge. A further temporal distinction is needed because not everything useful in collaboration deserves either standing.

A person may need to leave rough current state. An Agent may need to return a bounded result after the initiating chat is gone. An unanswered question may need to remain visible tomorrow. A completed transient check should not permanently accumulate in the current working view. A useful statement may later deserve durable human authorship or Wiki standing, but temporal presence alone must never grant that authority.

The resulting relation is:

```text
ProjectCentral/user/**
    durable human-authored Project ground
              ↕ explicit accepted return
ProjectCentral/now/**
    moving temporal working field
              ↕ bounded refs / promotion
ProjectCentral/agents/wiki/**
    Agent-maintained Project knowledge
```

This is a temporal source layer, not another collaboration product.

## Core semantics

```text
NOW = the moving current working horizon
DAY = a dated local-civil aggregation / rollover boundary
```

`NOW` is deliberately session-independent. Several Sessions can contribute to one NOW, and a later Agent can recover the useful current horizon without receiving the old chat transcript.

`DAY` is a closure reading of what the NOW horizon contained at one supplied local civil date and what the rollover did with it. The dated reading survives cleanup so that removal from the moving horizon does not erase what happened.

The following distinctions are constitutional:

```text
NOW ≠ DAY
NOW ≠ Session
NOW ≠ Run
NOW ≠ Focus
NOW ≠ Wiki
NOW ≠ authored Project canon

DAY ≠ Project history database
DAY ≠ automatic truth promotion
```

Run, Session, Focus, source, evidence and other identities therefore appear only as refs owned by their native systems.

## Filesystem shape

NOW is an opt-in child of an already-valid ProjectCentral:

```text
ProjectCentral/
├── user/                       durable human Project ground
├── agents/
│   ├── governance/
│   └── wiki/
│       ├── wiki.json           canonical Agent Wiki source
│       └── returns/            returned sources awaiting/feeding Wiki-owner maintenance
└── now/
    ├── user/                   free human scratch/current source
    ├── agents/                 attributed bounded Agent returns
    ├── day/                    derived dated closure readings
    ├── policy.json             inspectable rollover policy
    └── promotions.json         current promotion receipts
```

A ProjectCentral without `now/` remains fully valid. `projectcentral.now.inspect` is read-only and does not opt the Project in. `projectcentral.now.init` performs that explicit opt-in.

The shape intentionally separates authorship before it tries to aggregate experience. One shared Markdown file would make a human sentence, an Agent inference and a derived daily summary difficult to distinguish later.

### Human side

`ProjectCentral/now/user/**` is ordinary human-owned source. A person can create or edit Markdown, text, sketches or other files directly. Central does not require a frontmatter schema for scratch and rollover does not automatically delete it.

Temporal human material is not durable authored Project ground merely because the same human wrote it. Promotion to `ProjectCentral/user/**` is an explicit operation requiring `acceptance = human-accepted`; Central copies the source and records the return rather than silently changing its standing in place.

### Agent side

Each Agent return in `ProjectCentral/now/agents/*.json` is a bounded attributed record using `central.project-now.handoff/v1`.

It carries:

```text
id
actor
provenance = agent-authored-bounded-return
kind = handoff | question | note | learning
timestamp
subject
returned result
status = active | waiting | resolved | carried | promoted | expired
optional Run / Session / Focus refs
optional source / evidence refs
optional preserve refs
carry / promotion lineage
```

This small envelope exists because a result that must survive its Session needs enough authorship and lifecycle information to remain intelligible. It is not a substitute for Factory Artifact, Claim, Evidence, Decision, Run or HumanRequest identities. Where those objects exist, the NOW record points to them.

Agent learning returns toward the existing Wiki owner path through `ProjectCentral/agents/wiki/returns/**`. This deliberately does **not** edit `wiki.json` directly. The returned source is made durable in the Wiki-owned region with provenance intact; the system that owns semantic Wiki maintenance can decide how it should alter the Wiki.

## Read model

`projectcentral.now.inspect` returns one bounded current reading:

```text
human_scratch
active_items
open_questions
inactive_items
invalid_items
day_records
promotion receipts
policy
boundary statements
```

This is the source/read-model seam for AIKit and future Surfaces. Central does not rank these records or decide how much of them belongs in a model context.

## DAY rollover

The caller supplies `day` and `next_day` as local civil dates (`YYYY-MM-DD`). Central does not guess a timezone from UTC and does not require a scheduler.

The current default policy is explicitly stored in `ProjectCentral/now/policy.json`:

```text
carry: active, waiting, carried
remove from moving NOW: resolved, expired, promoted
protect inactive record when preserve_refs is non-empty
human scratch cleanup: human-owned-manual
```

These are defaults, not a universal retention interval. There is no Nara-derived “keep N days” law.

Rollover proceeds in this order:

1. inspect human scratch, bounded Agent returns and current promotion receipts;
2. classify Agent records under the current policy;
3. write `day/YYYY-MM-DD.md` as the dated closure reading **before** cleanup;
4. leave live records at their stable paths and mark them `carried`, adding the closed DAY to their carry lineage;
5. delete resolved/expired/promoted Agent records from moving NOW when no `preserve_refs` protect them;
6. retain inactive records with preserve refs;
7. keep human scratch under human control;
8. clear the transient promotion-receipt ledger after the receipts have entered the DAY reading.

The DAY reading includes source paths plus Agent actor/provenance and the bounded returned text. Therefore deleting a resolved transient return from NOW removes current clutter without making the day's historical fact disappear.

If DAY was written but a later cleanup step fails, the Action returns `partial_completion` and reports the failed cleanup operation rather than claiming an atomic success that did not happen.

### Reference protection

Central cannot discover every future durable object in every external owner. It therefore uses an explicit conservative contract: a NOW return with one or more `preserve_refs` is not deleted by rollover even if its lifecycle status would otherwise be removable.

A Factory Artifact, Run, accepted canonical relation or another durable owner can place its own ref there when the temporal source must remain materialised. This satisfies the deletion law without making Central parse or own those external ontologies.

## Actions

```text
projectcentral.now.inspect    read current temporal field; non-mutating
projectcentral.now.init       opt a valid ProjectCentral into NOW/DAY
projectcentral.now.return     write attributed bounded Agent return
projectcentral.now.update     update status / add preserve refs
projectcentral.now.promote    explicit return into human ground or Agent Wiki owner path
projectcentral.now.rollover   close DAY and clean/carry NOW
```

Human scratch requires no Action. Generic Agent/action callers can use the structured Actions; a future O:I Surface can project the same contract.

## Ownership boundaries

**Central owns:** directory/source identity, provenance envelope, inspectable retention policy, DAY closure and temporal cleanup, explicit source-return semantics.

**AIKit owns:** discovery as a ContextSource, relevance/ranking, bounded retrieval and ContextResolution. Central does not push the entire NOW horizon into every Agent context.

**Software Factory owns:** Run, RunMap, Decision, HumanRequest, Claim/Evidence, Artifact and durable developmental topology. NOW only stores refs and bounded returns around those objects.

**O:I owns:** any unified Now/Today presentation across its application surfaces. It should render this source rather than create a second temporal store.

**Agent Wiki owner path:** returned Agent learning can become source under `agents/wiki/returns/**`; semantic incorporation into `wiki.json` remains a Wiki-maintenance concern.

## Root / personal scope

This tranche does not add a new top-level `Central/Now` root. The established personal-world law remains `Control/**`. The implemented contract is Project-local because #74's acceptance concerns a Work Project and because adding another top-level root merely for symmetry would weaken the existing Central boundary.

A later personal NOW design should be derived through the current Control structure only when there is a concrete personal-world use case and owner contract for it.

## Nara precedent

The nara-personal daily-note work established a useful experiential result: a dated aggregation should contain the person's state in their own words, preserve open forward questions for later Sessions, and keep a compact progress/wayfinder reading. ProjectCentral generalises the relation rather than the Nara ontology.

The generalised rule is:

```text
human current source
+ Agent returns
+ active refs
+ open questions
        ↓
DAY reading
        ↓
carry what remains live
remove current clutter
return meaningful material to its durable owner
```

## Acceptance evidence

`ctrl/tests/projectcentral_now.rs` proves a filesystem-level Work Project traversal:

```text
human writes direct scratch
→ later Action/Agent reads NOW
→ Agent writes bounded question/handoff/learning
→ another reading sees the question without chat history
→ meaningful Agent learning returns into the Wiki owner path
→ meaningful human source returns into authored Project ground only with human acceptance
→ DAY closes
→ resolved and promoted transient Agent records leave moving NOW
→ waiting question carries at the same source ref
→ carried question retains Run/Session refs
→ no Session/Run/Focus/Wiki replacement is created
```

`ctrl/tests/projectcentral_now_portable_real.rs` additionally copies exact files from the checked-out Central Project into a portable `Central/Work/Central-current` specimen, opts that Project into NOW, performs DAY rollover, and verifies that native README, vision and implementation source bytes remain unchanged.

This portable evidence is deliberately distinct from a receipt against the owner's physical `~/Central/Work/*` installation. CI can prove the repository contract; it cannot truthfully claim access to a machine it does not have.
