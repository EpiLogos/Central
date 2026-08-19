# Central

Central is a **human-owned operating root for a technological life**.

It exists so that a person's working world can remain recognisably theirs while models, agent runtimes, applications, interfaces, machines and infrastructure change around it.

That continuity cannot safely be delegated to whichever tool happens to be current. Agent products can observe a person, infer patterns and maintain their own state, but those are not the same thing as the person deliberately saying: **this is part of the ground I want my technological world to carry forward**.

Central therefore gives ordinary authored source a durable home, keeps ordinary work ordinary, and defines stable Actions through which humans and software can operate on that world without taking ownership of it.

Central is not a configuration product or a documentation system. Machine configuration and Project documentation are things a durable authored ground may contain. The larger product is the relation between **human authorship, continuity, ordinary work, Agent-maintained knowledge and changing technological reality**.

## Why authored ground exists

Fast agentic development creates an attention problem as much as a context problem. A person can remain continuously "in the loop" while spending most of their time restating purpose, repairing context, approving reversible implementation choices, reconstructing state and checking facts that the surrounding system could have carried.

Central exists partly to change that relation.

The human should have a durable place for the determinations where their authorship is consequential: what something is for, why it matters, what experience it should create, which distinctions should survive implementation, what direction is worth pursuing, and when returned reality warrants a genuine change of mind. Agents and deterministic systems can then carry more of the recoverable machinery around those determinations: source discovery, codebase navigation, implementation facts, indexes, Wiki maintenance, evidence gathering and routine reversible engineering.

This is not a rigid human-versus-Agent task taxonomy. Agents can help with vision, design, analysis, prose, alternatives and prototypes; humans can inspect technical depth whenever they want. The distinction concerns **attention and authority**. Generated assistance can participate deeply without silently becoming human-authored ground, and routine system complexity should not be turned back into human approval work merely because a person is available.

A useful developmental relation is:

```text
COMMISSION
    an authored direction, existing source or sufficiently determinate request
              ↓
DEVELOPMENTAL BODY
    Agent judgement + deterministic machinery + source + evidence
              ↓
RETURNED REALITY
    implementation, tests, prototypes, encountered consequences
              ↓
RECOGNITION
    accept / redirect / deepen / revise intent / leave ground unchanged
```

Commission does not require ceremony when the relevant intention already exists. Recognition does not mean approving every technical completion. Both matter where human meaning is actually at stake.

## Why ordinary authored source matters

A durable personal world needs a source whose meaning does not depend on one agent's memory format, one application's database, or one provider's current account model.

Central uses ordinary files because they remain directly inspectable, editable, versionable and portable. Optional software can index, retrieve, render or act on them, but that software does not become the source owner merely by making the source easier to use.

This creates an important distinction:

```text
Authored
    the human deliberately states, adopts or retains it

Observed
    software measures or discovers it

Inferred
    software derives it from evidence
```

Observation and inference can support a proposal. They do not silently become authored ground. An observed pattern may be accurate without being something the person wants to define future interaction; an implementation fact may be current without explaining why the Project exists.

The same law applies recursively inside Projects. Vision can say what is meant without proving current behaviour. Code can say what exists now without retroactively authoring purpose. Evidence can say what happened under a condition without deciding the intended direction. Agent synthesis can relate those sources without inheriting their authority.

## Start where you already are

Central is designed to meet an existing technological world rather than requiring a replacement world first.

A person's editor, launcher, package manager, automation system, agent harness, filesystem conventions and Projects can remain native. Existing source is inspected before the person is asked to restate what is already known.

For a Project, the useful bootstrap relation is:

```text
existing repo / local Project / fresh Project
        ↓
recover what is already known
        ↓
establish only genuinely missing authored ground
        ↓
Project becomes human-legible + Agent-operable
```

A mature Project may already contain purpose, positions, design and architecture in native files. Those sources can remain where they are and be explicitly related to ProjectCentral. A fresh Project may naturally require more visioning. A tiny Project may need only one note, or no authored ground yet.

The target is not "metadata installed". The target is a Project that a human and an Agent can enter meaningfully without either side having to reconstruct the whole world from scratch.

## The product in one view

```text
Central
├── Control/      durable personal source world
│   ├── user/     human-owned personal authorship ground
│   ├── agents/
│   │   ├── governance/  human-authored recurring Agent governance
│   │   └── wiki/        Agent-maintained root/personal Wiki
│   └── machines/        intended machine roles and operating intent
├── ctrl/         stable executable Actions and public SDK
├── connectors/   replaceable bindings from Ports to real technologies
├── skills/       reusable Agent procedures for working with Central
├── Work/         ordinary work, with optional ProjectCentral per Project
│   └── <project>/ProjectCentral/
│       ├── user/          human-owned Project authorship aperture
│       ├── agents/wiki/   Agent-maintained Project Wiki
│       └── relations/     accepted source/provenance relations when needed
└── .central/     derived local state subordinate to authored source
```

The compact dependency rule is:

> Authored source says what should remain answerable to the human. Agent Wikis maintain changing knowledge around that source. `ctrl` says what can be done. Connectors say how it can be done here.

Those responsibilities remain separate because continuity is not only persistence of bytes; it is persistence of the relation between meaning, knowledge and current reality.

## ProjectCentral: authored ground surrounded by living knowledge

Inside `Work/<project>`, ProjectCentral gives a substantial Project an obvious but optional place to author the few high-altitude things the person most needs to remain responsible for: purpose, intended experience, vision, design judgement, plans, important distinctions or recognised changes of direction. The human can structure that material naturally; Central does not generate a documentation taxonomy.

The complementary Agent side is the Project Wiki. It can keep track of native design and architecture, source, implementation, tests, evidence and current development while retaining provenance back to the sources that support those claims.

The recursive relation is:

```text
human-authored Project ground
        ↓
Agent Wiki understanding
        ↓
implementation / evidence / current reality
        ↓
Agent Wiki learns what changed
        ↓
difference becomes legible
        ↓
account / explanation / proposal
        ↓
human Recognition where warranted
        ↺
```

Difference is not automatically bad. The implementation may be wrong; the design may need to develop; the original intention may become more precise; or no authored change may be needed. The important rule is that **difference is returned before authority is collapsed**.

Existing native source does not need to move into ProjectCentral. A source can be explicitly recognised and retained in place. `ProjectCentral/user/**` is a human-owned aperture, but machine-readable authorship is not inferred from location alone: an AI-generated suggestion does not become human-authored merely because it landed there.

## What changes for a human

A person can recover a new machine or agent environment without reconstructing themselves from application settings and scattered memories. They can inspect the source directly, edit it without a special UI, keep project-specific material with the Project, and decide explicitly when returned evidence deserves to become part of the future ground.

The intended result is not more personal configuration or documentation work. It is **less repeated re-authoring and less routine supervision**, so human attention can remain closer to purpose, experience, judgement, taste, consequential choice and Recognition.

## What changes for an Agent

An Agent can enter a world with a stable, permission-bounded authored ground rather than treating every session as a blank prompt or silently learning a replacement profile.

Availability still does not imply disclosure. A file can exist, be indexable and be retrievable without being loaded into every context. Central supplies source identity and treatment; systems such as AIKit determine what is relevant and permitted for the current act.

ProjectCentral gives an Agent stable distinctions among:

```text
recognised human Project source
Agent-maintained Wiki knowledge
native design / architecture / code
observed evidence / current development state
inference
```

Those distinctions are useful because they reduce unnecessary human questioning. An Agent can resolve facts from source, inspect current implementation, use evidence, make reversible engineering judgements under existing authority, and only return a human question when consequential meaning is genuinely unresolved.

## Objective Internality

Central makes one part of Objective Internality concrete without requiring philosophical vocabulary for ordinary use.

A person or Agent does not need to carry an entire working world in biological memory or prompt tokens. Authored files, semantic knowledge, machine observations, Project source and histories can remain objective, inspectable structures while becoming part of the operative interior of an act when selectively disclosed.

Their value comes from **mutual legibility without authority collapse**:

```text
human-authored ground
Agent-maintained knowledge
observed reality
derived/indexed state
Projection / public presentation
```

These can participate in one coherent reading while remaining different kinds of source.

## Relation to the wider {O:I} field

**O:I** is the whole field of technological agency. Central is the durable authored ground within that field, not the owner of every surrounding capability.

**Actuation** defines how situated agency, delegation, authority and Return are constituted. Central can be the world in which an Agency is grounded without becoming the agency runtime.

**AIKit** resolves what is available to an actor now — capabilities, sources, models, sessions, Surfaces and other resources. Central supplies authored ground and ProjectCentral Wiki/source identities that AIKit can make addressable without collapsing availability into automatic prompt injection.

**Software Factory** develops Projects from authored intention through implementation, evidence, Candidate reality and Recognition. ProjectCentral provides a durable source relation for that development while Factory retains ownership of Runs, evidence, Decisions and developmental Return.

**Workcell** materialises computational worlds. Central can state durable machine intent while Workcell owns runtime placement, services, bindings and lifecycle.

**Quaternal Logic** can treat Central material as a subject of formal or semantic inquiry where requested; Central does not require QL in order to remain a valid authored root.

O:I can project selected readings of the same Central world. Projection remains selection/presentation rather than a shadow source database: local readability does not imply public disclosure, and presentation refinement does not silently rewrite Central.

## Product principles

1. **Human authorship is explicit.** Observation, inference and generated suggestions do not silently become authored source.
2. **Preserve human attention for consequential authorship and Recognition.** Recoverable facts and routine reversible machinery should not become unnecessary human approval work.
3. **Recover before asking.** Existing source should reduce redundant interrogation, not be ignored in favour of a fresh questionnaire.
4. **Authored continuity outranks implementation convenience.** A tool may improve access without becoming source authority.
5. **Control stays high-signal.** Persistent material belongs at the narrowest scope where it remains correct.
6. **Availability does not imply disclosure.** Existing or indexed information need not be loaded into every Agent context.
7. **Process and context stay distinct.** Skills contain reusable procedure; authored source can state durable preference or intent about procedure.
8. **Actions have stable identity.** Human and software Surfaces can invoke the same operation.
9. **Derived state stays subordinate.** Caches, indexes, accounts, projections and observations do not outrank authored source.
10. **Existing Projects remain native.** ProjectCentral can relate useful source in place rather than requiring wholesale migration.
11. **Returned reality can revise direction without silently rewriting it.** Difference becomes legible before human source changes.
12. **The real installation tests the architecture.** First-party extensions must use the same public seams available to others.

## Current implementation

Central is implemented in **Rust**. Executable product code, the public SDK, Connectors and executable product/conformance harnesses use Rust; ordinary data and authored source remain in representations appropriate to their meaning.

The base `ctrl` source-install contract is:

```sh
cargo install --path ctrl
```

A clean root can then be initialized and inspected with:

```sh
ctrl --root /path/to/Central init
ctrl --root /path/to/Central doctor --json
ctrl --root /path/to/Central action list --json
```

Initialization creates the recursive Control roots, root Agent Wiki, `.central`, and `Work`. Human-owned authorship roots begin empty rather than being populated by guessed personal facts.

ProjectCentral lifecycle Actions create the Project-local `user/` + `agents/{governance,wiki}` relation without generating authored prose. On the #70 implementation line, authored-ground inspection is available as:

```sh
ctrl --root /path/to/Central projectcentral ground inspect <work-project>
ctrl --root /path/to/Central projectcentral ground plan <work-project>
```

The structured `projectcentral.ground.apply` Action records an explicitly accepted source/provenance/standing relation without changing source bytes or path.

Current accepted `main` remains the authority for merged behaviour. The #70/#71 stack and physical owner-world acceptance are development state until merged/observed; downstream Factory/O:I behaviour is not presented here as already completed merely because the source contracts now exist.

## Documentation

Read the package in this order:

1. [`docs/CENTRAL-VISION.md`](docs/CENTRAL-VISION.md) — why the authored root exists and the experience it should preserve.
2. [`docs/CENTRAL-SYSTEM-SPEC.md`](docs/CENTRAL-SYSTEM-SPEC.md) — normative product and architecture specification.
3. [`docs/CONTROL-CONTENT-PROTOCOL.md`](docs/CONTROL-CONTENT-PROTOCOL.md) — authorship, durable information and disclosure boundaries.
4. [`docs/PROJECTCENTRAL-CONTRACT.md`](docs/PROJECTCENTRAL-CONTRACT.md) — recursive Project-local human-source / Agent-Wiki filesystem and identity contract.
5. [`docs/PROJECTCENTRAL-AUTHORED-GROUND.md`](docs/PROJECTCENTRAL-AUTHORED-GROUND.md) — optional authored-ground UX, accepted source relations, account handoff and return law.
6. [`docs/CENTRAL-PUBLIC-HANDOFF.md`](docs/CENTRAL-PUBLIC-HANDOFF.md) — concise outward-facing source for O:I/site descriptions of Central.
7. [`docs/PRODUCT-GROUND-CONVENTION.md`](docs/PRODUCT-GROUND-CONVENTION.md) — optional human-authored `Control/user/products/<product>/` convention for cross-context product ground.
8. [`docs/PERSONAL-WORLD-PROJECTION.md`](docs/PERSONAL-WORLD-PROJECTION.md) — selected personal/world Projection, public disclosure and explicit return to Central source.
9. [`docs/CONNECTOR-SDK-SPEC.md`](docs/CONNECTOR-SDK-SPEC.md) — Action, Port, Connector, Surface, SDK and conformance architecture.
10. [`docs/PERSONAL-EXTENSION-SPEC.md`](docs/PERSONAL-EXTENSION-SPEC.md) — first real extension set used to harden the public architecture.
11. [`docs/INSTALL.md`](docs/INSTALL.md) — native `ctrl` installation and clean-root verification.
