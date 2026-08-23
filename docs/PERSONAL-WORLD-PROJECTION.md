# Central — Personal World Projection

**Status:** integration contract with O:I Projection

## 1. Purpose

Central is the person's durable local authored world.

A person can choose a face of that world to make present in O:I without creating a second profile database or transferring ownership of Central source.

The current relation is:

```text
Central
├── Control/user/**
├── Control/agents/
│   ├── governance/**
│   └── wiki/wiki.json
├── Control/machines/**
└── Work/
    └── <project>/ProjectCentral/
        ├── user/**
        ├── agents/governance/**
        ├── agents/wiki/wiki.json
        ├── relations/source-relations.json   # optional
        └── project.json
             │
             │ selected provenance-aware reading
             ▼
        O:I WorldPresentation
             │
             │ explicit ratification
             ▼
        O:I Projection revision
             │
             └── local/public personal world — the meaningful profile
```

The Projection is a selected reading of Central. Central remains the canonical owner of Central source and accepted ProjectCentral source relations.

## 2. No profile shadow

Central must not maintain a second public-profile object which merely describes the same person.

The useful human relation is:

> My profile is the face of my world I have chosen to make present here.

A personal Projection can include selected identity or presentation material, interests, positions, Projects, Agents, writing, research, Wiki material, current outputs, presentation preferences, or SharedField relations.

Those are projected selections. They do not form a new canonical person schema.

## 3. Existing roots keep their meaning

This integration does not change the Central root law.

```text
Control/user
    human-owned personal authorship ground

Control/agents/governance
    human-authored recurring human-Agent relation

Control/agents/wiki
    Agent-maintained personal/root semantic knowledge

Control/machines
    authored machine and environment ground

Work
    ordinary Projects, each optionally carrying ProjectCentral
```

The content protocol still defines no fixed schema beneath the authored roots.

ProjectCentral recursively preserves the same human-source ↔ Agent-Wiki distinction inside an ordinary Work Project. It does not make `Work/` a proprietary project format.

Rich account documents are optional authored or generated surfaces. They can help a person understand or present their world. They do not become required root schemas.

## 4. Selection is narrower than readability

The existence or local readability of Central material does not imply that it is selected for Projection.

Keep these relations distinct:

```text
source exists
source is readable
source can be indexed
source can be retrieved
source is selected for a reading
source is selected for local presentation
source is selected for this Projection
source is public
```

A Projection must name the material actually selected. Selecting one file beneath `Control/user` does not project the root. Selecting one Project beneath `Work` does not project every Project, every ProjectCentral source relation, or every WikiNode inside it.

The safest public representation is therefore omission by default: unselected material does not enter the Projection representation.

## 5. Account as reading

Central does not introduce an `Account` ontology.

An account is a coherent authored or generated reading over native source material. Several accounts can exist over the same Central world or Project.

Examples include:

- a public personal/world account;
- a private personal account;
- an Agent-collaboration account;
- a machine/environment account;
- a current-work account;
- a deep Project account relating authored purpose to implementation/evidence.

Each account keeps source provenance. None silently replaces its sources.

For ProjectCentral, `projectcentral.ground.inspect` supplies exact recognised human-source refs/standing, other source relations, and Agent Wiki source/WikiSpace identity as an account handoff. AIKit owns the reusable account-authoring procedure. HTML remains a renderer.

## 6. Work remains ordinary Projects

`Work/` remains ordinary filesystem material.

A Project can be a lightly documented directory, an imported repository, a mature Project canon, or a multi-repository authored whole.

Central must not require a special Project document format as the price of Projection.

Where ProjectCentral exists, the current reading can be formed from heterogeneous native source without moving it into one folder:

```text
Work/<project>/
    ├── ProjectCentral/user/**
    │      recognised human-authored/adopted ground
    ├── ProjectCentral/relations/source-relations.json
    │      accepted relations to native Project sources
    ├── ProjectCentral/agents/wiki/wiki.json
    │      Agent-maintained semantic knowledge
    ├── native vision / design / architecture
    ├── source / code
    ├── Runs / decisions / evidence (native owners referenced)
    └── current reality
         │
         ▼
    provenance-aware reading
         │
         ▼
    WorldPresentation / O:I Projection
```

A native source may remain in place and still be explicitly related as human-authored Project ground. Discovery alone does not infer that authorship.

The Project's native source and authority relations remain canonical at their own scopes.

## 7. Public refinement does not mutate Central

A human can edit the projected presentation of their world. That operation changes the Projection representation, not Central source.

```text
Central source revision C7
        ↓ selection / authored presentation
Projection P1
        ↓ public presentation refinement
Projection P2
        ├── still grounded in C7
        └── editor provenance retained
```

O:I owns this Projection revision relation.

Central must not treat a browser draft, local presentation state, standalone HTML account, or public refinement as an accepted source mutation.

## 8. Return from Projection to Central

Presentation can teach the person something real about their own authored ground. The return path therefore matters.

When a public or local refinement should become durable Central source:

```text
Projection difference
        ↓
proposed Central change
        ├── target source
        ├── proposed content
        ├── reason
        ├── supporting context / provenance
        └── final diff
        ↓
human review
        ↓ accepted mutation
new Central source revision
```

This is the existing durable-change proposal law in `CONTROL-CONTENT-PROTOCOL.md`.

The return is explicit because Projection authority and Central authorship are different things.

Where the target is ProjectCentral, return also preserves the distinction:

```text
return to Agent-maintained Wiki knowledge
        ≠
proposal to human-authored / human-adopted Project source
```

## 9. Skill ownership

Central owns the source being worked upon and the accepted return mutation/source relation.

Reusable authoring procedure belongs in AIKit Skills rather than being copied into Control:

```text
Central
    authored personal / Project source
    ProjectCentral source relations

AIKit
    product-understanding
    Knowledge Navigation / Agent Wiki maintenance
    structured-account-authoring
    projection-authoring
    html-account

O:I
    Projection / WorldPresentation / Explore
```

A Skill being available does not grant read authority to all Central material. Source treatment, relevance, context eligibility and explicit Projection selection still apply.

## 10. Local and public are readings of one world

The local O:I application can show more of Central than public Explore because it can operate under different source treatment and audience boundaries.

Local Central can include private authored material, machine state, current Projects, local Agents, proposals, Agent Wiki knowledge, source-return pressure and current operations.

A public Central Projection contains only explicitly projected material.

These are different readings of the same authored world, not separate semantic person identities.

## 11. Human and Agent reading

The same Projection ref and revision should identify the personal-world representation for human and Agent surfaces.

Humans can encounter it through Explore, O:I desktop, or a standalone HTML account.

Agents should receive the structured WorldPresentation/Projection reading with source refs and provenance. They should not need to scrape HTML.

For Project readings, recognised human source, Agent Wiki knowledge, design/architecture, implementation/evidence, current development state and inference remain distinguishable rather than being flattened into one generated narrative authority.

## 12. Boundary summary

The integration preserves seven ownership laws:

1. Central remains the person's durable local authored world.
2. ProjectCentral recursively preserves human-owned authored ground and Agent-maintained Wiki knowledge without making them the same authority.
3. A projected personal world is the meaningful profile; no shadow profile database is required.
4. Local/source readability never implies Projection or public selection.
5. Public presentation refinement changes Projection, not Central.
6. A refinement returns to durable Central human source only through direct human authorship or an explicit accepted proposal.
7. A return to Agent Wiki knowledge is a different operation from human-source revision.
