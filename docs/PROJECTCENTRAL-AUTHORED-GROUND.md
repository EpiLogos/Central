# ProjectCentral authored Project ground

**Status:** Central #70 design/implementation contract  
**Owner:** Central filesystem/source relation  
**Consumes:** `PROJECTCENTRAL-CONTRACT.md`, `CONTROL-CONTENT-PROTOCOL.md`  
**Downstream:** AIKit Knowledge Navigation/account Skills, Software Factory Project reading, O:I WorldPresentation/Projection

## 1. Purpose

ProjectCentral already gives every participating Project the recursive relation:

```text
ProjectCentral/
├── user/
├── agents/
│   ├── governance/
│   └── wiki/wiki.json
└── project.json
```

The missing human experience is not another document schema. It is a clear way to answer:

```text
Does this Project have authored ground?
Which sources have actually been recognised as human-authored/adopted?
Which existing native sources might matter but still need human judgement?
What authority/standing does each recognised source have?
What would a proposed source treatment change?
What can an account reader safely consume without confusing reading with source?
```

The intended experience after ProjectCentral lifecycle setup is therefore:

```text
ProjectCentral       ready
Agent Wiki           ready
Human ground         empty | partial | established

Optional:
    inspect / establish authored Project ground
```

Optional does not mean peripheral. For a substantial Project, writing or recognising purpose, desired experience, vision, design judgement or another high-altitude human responsibility should be an obvious early activity. A tiny Project remains valid with one note or temporarily no human ground at all.

`projectcentral.init` still creates no human document.

## 2. `ProjectCentral/user` is a human-owned aperture, not an authorship detector

`ProjectCentral/user/**` is the preferred Project-local human authorship space. The human may organise it naturally and edit it with ordinary tools.

Filesystem location alone, however, cannot prove who produced a particular byte sequence. This matters for AI-assisted authoring.

```text
file is inside ProjectCentral/user
        ≠
file is therefore human-authored
```

The machine-readable ground model consequently treats an unclassified file in `ProjectCentral/user` as **authorship unresolved** until direct human authorship or adoption has been recognised once.

This does not introduce approval ceremony for ordinary editing. Once a source relation says that `ProjectCentral/user/vision.md` is human-authored or human-adopted, subsequent direct human edits remain ordinary source editing. The relation does not require approval for every keystroke.

An AI-generated suggestion may therefore exist in the aperture while remaining explicitly `generated-suggestion`. It acquires human-source authority only when the human actually authors/adopts that source relation.

The operational preference remains stronger: Agent systems should normally keep generated drafts in proposal/derived state until the human chooses to bring them into the authored aperture.

## 3. Source relations, not a second content store

Central records accepted machine-readable relations in:

```text
ProjectCentral/relations/source-relations.json
```

Schema:

```text
central.project.ground-relations/v1
```

This file is **Central relation metadata**. It is not the Project account, not generated Project canon and not a replacement for the source it names.

A relation carries:

```text
stable source ref
project-relative source path
provenance/authorship class
truth/authority standing
free semantic roles
source treatment
explicit recognition provenance
```

The relation allows a native file such as `docs/vision/product.md` to remain exactly where the Project already keeps it while becoming a recognised human source for ProjectCentral/AIKit readers.

## 4. Roles are free semantic roles, not a folder taxonomy

The relation may carry roles such as:

```text
purpose
intent
positions
desired-experience
visual-direction
interaction-direction
design
plans
mockup-prototype
research-framing
important-decision
recognised-change-of-direction
```

The list is illustrative, not closed.

Central does not require `VISION.md`, `DESIGN.md`, `/mockups`, `/plans`, or any other canonical filename. The Project may retain its native vocabulary and arrangement.

Path/filename discovery can produce **role hints** to help inspection. A file called `docs/VISION.md` may reasonably produce hints such as `vision` and `purpose`; that signal does not establish authorship or authority.

## 5. Provenance and standing remain separate

The executable read model distinguishes source provenance including:

```text
human-authored
human-edited-draft
human-adopted
generated-suggestion
generated-derived
agent-maintained
observed
inference
unresolved
```

It separately exposes truth/authority standing including:

```text
authored-human-position
design-commitment
architecture-contract
implementation-fact
observed-evidence
current-development-state
agent-inference
unspecified
```

The separation matters because an ordinary human-authored file can contain an architecture commitment, while observed implementation evidence can be written into a human-readable report without becoming authored intent.

The governing law remains:

```text
vision / human position
    says what is meant
    does not prove current behaviour

architecture / design
    constrains intended form
    does not by itself prove implementation

implementation fact
    says what exists now
    does not retroactively author why the Project exists

run / test / observed evidence
    says what happened under a condition
    does not automatically determine intended direction

Agent inference
    can relate all of the above
    does not inherit their authority
```

## 6. Native Actions and read models

Central #70 adds three Actions under the existing ProjectCentral Action domain.

### `projectcentral.ground.inspect`

Read-only.

Returns:

- ProjectCentral readiness and project identity;
- preferred human aperture;
- `empty | partial | established` ground status;
- recognised source relations and exact refs;
- unresolved native candidates with non-authoritative role hints;
- retrieval-denied subtrees;
- canonical Agent Wiki source/WikiSpace ref when available;
- account-handoff sources;
- source-return policy;
- optional next actions.

Ground status means:

```text
empty
    no recognised/aperture source requires machine provenance judgement yet

partial
    source material/relations exist, but no source is yet recognised as
    human-authored or human-adopted

established
    at least one existing source has an accepted human-authored/adopted standing
```

Unresolved native candidates do not themselves change `empty` to `established`.

### `projectcentral.ground.plan`

Read-only.

Produces a reviewable treatment plan. For existing native Project source its default recommendation is conservative:

```text
retain source in place
+ record relation if the human recognises it
+ leave source bytes/path unchanged
```

Other valid outcomes remain visible:

- leave as ordinary Project source;
- classify as generated/observed/inferred rather than human-authored;
- explicitly reorganise later after review;
- remain unresolved.

The plan does not move or copy source.

### `projectcentral.ground.apply`

Locally mutating, but only at the relation-metadata layer.

It requires an explicit `acceptance = human-accepted` input and records one accepted source/provenance/standing/treatment relation.

It reports explicitly:

```text
source bytes mutated     false
source path mutated      false
relation metadata        changed
```

The current Central Action layer does not cryptographically identify a caller. `human-accepted` is therefore the semantic boundary Surfaces/Agents must honour; it is not a claim that Central can infer a person's identity from a filesystem call. A future authority layer can strengthen caller attestation without changing the source relation itself.

The direct convenience surface is:

```text
ctrl projectcentral ground inspect <work-project>
ctrl projectcentral ground plan <work-project>
```

Structured callers can discover/invoke all three through the normal Action descriptor/API surface.

## 7. Existing Projects: discovery is not adoption

Inspection looks for potentially meaningful native sources such as project overviews, intent/vision/position material, product/design/UX/visual material, HTML prototypes, plans, research framing and architecture documents.

It deliberately does **not** classify them as human-authored from filename, extension or location.

The safe progression is:

```text
native source discovered
        ↓
unresolved candidate + role hints
        ↓
human/source provenance judgement
        ↓
accepted relation
        ├── retain native in place
        ├── ProjectCentral/user source
        └── another explicit treatment
```

Adoption therefore never means “move all docs into ProjectCentral”.

## 8. AI assistance and human authorship

AI assistance can help form authored ground without collapsing source authority.

A useful progression is:

```text
generated suggestion
        ↓ optional human editing
human-edited draft
        ↓ actual adoption/authorship
human-adopted / human-authored source
```

Those are different provenance states.

The source-relation Action can record the transition without owning the prose-generation procedure. Reusable account/document authoring procedure remains in AIKit Skills.

## 9. Agent Wiki and recursive return

The Agent Wiki remains:

```text
ProjectCentral/agents/wiki/wiki.json
```

It can be maintained by the authorised AIKit Wiki procedure and may relate:

```text
human ground
native design / architecture
source / implementation
Runs / tests / evidence
current development state
inference
```

Returned reality may reveal a difference between these standings.

Difference is informative; it is not automatically an error and not permission to mutate human source.

```text
human-authored ground
        ↓
Agent Wiki understanding
        ↓
Project development / evidence
        ↓
updated Agent knowledge
        ↓
difference / tension / pressure
        ↓
account / explanation / proposal
        ↓
human judgement
        ├── implementation should change
        ├── design should change
        ├── intent should develop
        └── no authored change required
```

Agent Wiki maintenance and human-source mutation therefore remain different operations.

## 10. Account interoperability

Central does not build an Account renderer.

`projectcentral.ground.inspect` exposes an `account_handoff` containing:

- the preferred ProjectCentral authored aperture;
- exact recognised human source refs, paths, provenance, standing and roles;
- other accepted source relations;
- canonical Agent Wiki source and WikiSpace ref;
- the relation-source path.

It also states explicitly:

```text
account_is_source     false
html_is_source        false
projection_is_source  false
```

That is sufficient for AIKit's accepted `product-understanding` and `structured-account-authoring` procedures to form a provenance-aware reading without prompt-specific path folklore. `html-account` remains a renderer. O:I `WorldPresentation`/`Projection` remains presentation/selection authority.

## 11. Personal-world Projection recovered onto current semantics

The valid architecture previously explored in closed PR #60 is retained through the current ProjectCentral world:

```text
Central source
    ↓ selected reading
WorldPresentation
    ↓ explicit ratification
Projection
```

and on return:

```text
Projection refinement
    ≠ Central source mutation

Projection difference
    ↓
Central proposal
    ↓
human acceptance
    ↓
new Central source revision
```

The current source world is:

```text
Central/
├── Control/user/**
├── Control/agents/governance/**
├── Control/agents/wiki/wiki.json
├── Control/machines/**
└── Work/*/ProjectCentral/**
```

The obsolete pre-ProjectCentral filesystem examples from PR #60 are not restored.

The disclosure ladder remains:

```text
source exists
≠ source is readable
≠ source is indexed
≠ source is retrieved
≠ source is selected for a reading
≠ source is selected for Projection
≠ source is public
```

A local O:I world may therefore expose a richer private reading than public Explore without creating a second canonical profile/world database.

## 12. Work UX and ownership boundary

At Work scope, ProjectCentral authored ground is an ordinary Project relation. Central can report:

```text
ProjectCentral readiness
human-ground status
recognised source refs/standing
native candidate pressure
Agent Wiki source/WikiSpace
unresolved provenance state
```

Central does not calculate Factory Run state or AIKit relevance. Those products reference this source model and retain ownership of their own state.

No `Focus` primitive is introduced here. The stable Project/source/Wiki refs are sufficient for a later Focus relation to select over them without changing the ground contract.

## 13. Acceptance implications

The implementation is conformant when these remain true:

1. ProjectCentral initialization creates no human document.
2. A new Project reports empty authored ground and can be intentionally established with one natural source.
3. A file in `ProjectCentral/user` does not gain machine-readable human authorship solely from location.
4. Existing native role-like files remain unresolved until explicitly recognised.
5. Retain-in-place relation establishment changes relation metadata, not source bytes/path.
6. Generated suggestion, human-edited draft and human-adopted source remain distinguishable.
7. Agent Wiki identity remains separate from human source identity.
8. Account handoff preserves provenance/standing and does not create an Account ontology.
9. Returned reality can pressure authored ground without mutating it automatically.
10. Projection refinement remains presentation change until an explicit return is accepted into Central source.
