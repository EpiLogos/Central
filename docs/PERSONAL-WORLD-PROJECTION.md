# Central — Personal World Projection

**Status:** integration contract with O:I Projection

## 1. Purpose

Central is the person's durable local authored world.

A person can choose a face of that world to make present in O:I without creating a second profile database or transferring ownership of Central source.

The relation is:

```text
Central
├── Control/user
├── Control/agents
├── Control/machines
└── Work
     └── ordinary Projects
        │
        │ local selection + authored / reviewed presentation
        ▼
O:I WorldPresentation
        │
        │ explicit ratification
        ▼
O:I Projection revision
        │
        └── local/public personal world — the meaningful profile
```

The Projection is a selected reading of Central. Central remains the canonical owner of the authored source.

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
    authored personal ground

Control/agents
    authored ground for recurring human-Agent relation

Control/machines
    authored machine and environment ground

Work
    ordinary Projects and working material
```

The content protocol still defines no fixed schema beneath these roots.

Rich account documents are optional authored or generated surfaces. They can help a person understand or present their world. They do not become required root schemas.

For example, `Control/user` can contain a sustained personal account, or only a few natural prose files. `Control/machines` can contain a visual environment account, or ordinary declarations. Both are valid.

## 4. Selection is narrower than readability

The existence or local readability of Central material does not imply that it is selected for Projection.

Keep these relations distinct:

```text
source exists
source can be indexed
source can be retrieved
source is relevant
source is allowed in this context
source is selected for this Projection
source is public
```

A Projection must name the material actually selected. Selecting one file beneath `Control/user` does not project the root. Selecting one Project beneath `Work` does not project every Project or every source file inside it.

The safest public representation is therefore omission by default: unselected material does not enter the Projection representation.

## 5. Account as reading

Central does not introduce an `Account` ontology.

An account is a coherent authored or generated reading over native source material. Several accounts can exist over the same Central world.

Examples include:

- a public personal/world account;
- a private personal account;
- an Agent-collaboration account;
- a machine/environment account;
- a current-work account.

Each account keeps source provenance. None silently replaces its sources.

This follows the existing Control content rule that generated projections, summaries, indexes, and target-specific representations remain subordinate to authored source.

## 6. Work remains ordinary Projects

`Work/` remains ordinary filesystem material.

A Project can be a lightly documented directory, an imported repository, a mature Project canon, or a multi-repository authored whole.

Central must not require a special Project document format as the price of Projection.

Rich Project account work develops with the Project when it is useful:

```text
Work/<project>
    ├── raw intent / positions
    ├── vision
    ├── design
    ├── diagrams
    ├── architecture
    ├── source / code
    ├── Wiki / knowledge
    ├── Runs / decisions / evidence
    └── current reality
         │
         ▼
    authored reading
         │
         ▼
    O:I Projection
```

The Project's native source remains its authority.

## 7. Public refinement does not mutate Control

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

Central must not treat a browser draft, local presentation state, or public refinement as an accepted source mutation.

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
        ├── supporting context
        └── final diff
        ↓
human review
        ↓ accepted mutation
new Central source revision
```

This is the existing durable-change proposal law in `CONTROL-CONTENT-PROTOCOL.md`.

The return is explicit because Projection authority and Central authorship are different things.

## 9. Skill ownership

Central owns the source being worked upon and the accepted return mutation.

Reusable authoring procedure belongs in AIKit Skills rather than being copied into Control:

```text
Central
    authored personal source

AIKit
    product-understanding
    structured-account-authoring
    projection-authoring
    html-account

O:I
    Projection / WorldPresentation / Explore
```

A Skill being available does not grant read authority to all Control material. Source treatment, relevance, context eligibility, and explicit Projection selection still apply.

## 10. Local and public are readings of one world

The local O:I application can show more of Central than public Explore because it can operate under different source treatment and audience boundaries.

Local Central can include private authored material, machine state, current Projects, local Agents, proposals, and current operations.

A public Central Projection contains only explicitly projected material.

These are different readings of the same authored world, not separate semantic person identities.

## 11. Human and Agent reading

The same Projection ref and revision should identify the personal-world representation for human and Agent surfaces.

Humans can encounter it through Explore, O:I desktop, or a standalone HTML account.

Agents should receive the structured WorldPresentation/Projection reading with source refs and provenance. They should not need to scrape HTML.

## 12. Boundary summary

The integration preserves five ownership laws:

1. Central remains the person's durable local authored world.
2. A projected personal world is the meaningful profile; no shadow profile database is required.
3. Public selection never implies that an entire Central root is public.
4. Public presentation refinement changes Projection, not Control.
5. A refinement returns to durable Central source only through an explicit accepted proposal.
