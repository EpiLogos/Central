# Product ground in Control

**Status:** optional Control convention over the existing human-authored source contract  
**Owner:** the human author through Central / Control  
**Does not create:** a new Control root, schema, database, Profile system, or automatic import path  
**Whole-context relation:** `PROJECT-CONTEXT-PROTOCOL.md` — authored Ground participates at P0; bounded Intent participates at P3 without becoming a separate source authority

Central already fixes the important boundary: `Control/user`, `Control/agents`, and `Control/machines` are human-authored source roots, while the tree beneath them remains open. Product ground uses that existing contract rather than adding another ontology.

The smallest conventional product boundary is:

```text
Control/user/products/<product>/
```

Nothing beneath that path is mandatory. When the distinction is useful, a product may naturally grow something like:

```text
Control/user/products/<product>/
├── expressions/
│   ├── intent and purpose
│   ├── desired experience / encounter
│   ├── philosophical or conceptual notes
│   ├── visual / interaction stipulations
│   └── rejected framings worth preserving
├── positions/
│   └── relatively stabilised authored positions
└── VISION.md
    └── relatively stabilised authored vision
```

These names are a **convention, not a required schema**. A product can keep a different native arrangement when that preserves its meaning better. `Control/user/products/<product>/` is simply a useful common boundary for product-facing material that is genuinely part of the human's durable authored ground.

## Why this belongs under `Control/user`

Product vision here is not generic project documentation and it is not generated agent memory. It is durable authored material about what the human is trying to make, what kind of encounter matters, and which distinctions should survive across individual work sessions and repository reorganisations.

The native product repository still owns its own current vision, design, architecture, code, tests and development history. Control does not replace those sources. It can hold the human-authored ground from which several product repositories or projections are understood.

The relation is therefore:

```text
Control/user/products/<product>
    durable human-authored ground

native product repository
    product-local vision / design / architecture / implementation truth

AIKit / other agents
    authorised retrieval and procedure over those sources
```

## Source distinctions

Preserve these differences explicitly:

```text
relatively raw human expression != generated summary
human-authored position          != implementation fact
vision                            != current capability
stabilised position               != immutable dogma
evidence about a position         != permission to rewrite it
```

### Relatively raw expressions

Raw does not mean unimportant or temporary. A handwritten note, rejected framing, desired encounter, sketch description, or philosophical reflection can be the highest-authority evidence for *why* a product distinction exists even when it is not yet a polished position.

Agents may organise or summarise this material for a task, but the generated organisation remains derived until the human explicitly adopts it as authored source.

### Stabilised positions

A stabilised position is a human-authored determination that has become useful as a durable starting point. Stabilisation means agents should not casually reinterpret or overwrite it; it does **not** mean the position is immune to revision.

### Stabilised vision

Vision states what the product is meant to become or preserve. It is authoritative for intended meaning at its scope. It is not evidence that the implementation currently realises that intent.

### Bounded intent

The whole-context protocol distinguishes stabilised Vision from a bounded present Intent. Intent states what is presently being brought about within the larger authored horizon: the desired change/outcome, why it matters now, its scope/constraints and recognisable success conditions where those are known.

This is a semantic role, not a required filename or new source store. `intent.md`, `INTENT.md`, an issue, brief or another native Project source may play the role when provenance and standing support that reading. A completed or superseded Intent does not by itself revise the Project's Vision or founding positions.

## Retrieval and provenance

No new product-ground index or document database is required.

Existing Central semantics remain sufficient:

```text
control.open
control.search
ordinary filesystem reads
```

operate over the normal Control tree. Derived indexes may make discovery faster, but the authored file remains the source. Availability also remains distinct from disclosure: an agent should retrieve the narrowest source needed for the current act rather than loading every product note by default.

When reporting product-ground material, retain at least:

- the source path;
- that the source class is `authored` when it is live Control source;
- the scope/product to which it applies;
- any material conflict or supersession relation discovered while reading it.

Git history can preserve older authored forms. The live tree should make the current authored statement intelligible after an accepted revision.

## Returned reality and proposal pressure

Development, application, tests, experiments or product encounter may show that an authored position needs reconsideration. That is a normal return path, not a reason to let generated understanding mutate Control automatically.

Use the existing Control-maintenance proposal discipline:

```text
implementation / experimental / encounter evidence
        ↓
explicit statement of the pressure it places on current authored ground
        ↓
reviewable proposal
        target
        reason
        supporting context + provenance
        final diff
        ↓
explicit human acceptance / revision / rejection
        ↓
optional authored Control mutation
```

The proposal should say what class of returned material produced the pressure—for example implementation fact, experimental finding, current development state, or interpretation—so that an accurate observation is not silently promoted into authored intent.

A proposal can recommend:

- revising a position;
- adding a new position;
- moving a once-stable statement back into active exploration;
- recording a rejected framing;
- leaving the authored source unchanged because current implementation is the thing that should move.

The human resolves the authored relation.

## Relation to product-understanding procedure

The procedure for traversing authored ground through product vision/design, architecture, implementation and evidence belongs in an operational Skill (for the O:I suite, AIKit's native product-understanding Skill), not in persistent Control context.

Control supplies durable authored source. The Skill supplies a reusable method for deciding when and how deeply to retrieve it. Current implementation and experimental evidence remain in the repositories/systems that own them.

## Conformance questions

A product-ground implementation is healthy when all of these remain true:

- `Control/user/products/<product>/` is optional and does not become a fourth fixed Control root;
- no mandatory ontology is imposed below the product boundary;
- raw human material is not replaced by an agent summary merely for neatness;
- open/search continue to use ordinary Control source rather than a second database;
- vision is never presented as proof of current capability;
- bounded Intent can change without silently changing stabilised Vision/founding Ground;
- code is never treated as retroactive authorship of product purpose;
- returned evidence can generate a reviewable proposal without mutating authored source before explicit acceptance.
