# Central

Central is a **human-owned operating root for a technological life**.

It exists so that a person's working world can remain recognisably theirs while models, agent runtimes, applications, interfaces, machines and infrastructure change around it.

That continuity cannot safely be delegated to whichever tool happens to be current. Agent products can observe a person, infer patterns and maintain their own state, but those are not the same thing as the person deliberately saying: **this is part of the ground I want my technological world to carry forward**.

Central therefore gives ordinary authored source a durable home, keeps ordinary work ordinary, and defines stable Actions through which humans and software can operate on that world without taking ownership of it.

Central is not a configuration product. Machine configuration is one thing a durable authored ground may express. The larger product is the relation between **human authorship, continuity, ordinary work and changing technological implementations**.

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

Observation and inference can propose a change to durable ground. They do not silently become that ground. The difference matters because a pattern detected by an agent may be useful without being something the person wants to define them, their agents, their machines or their Projects in the future.

## Non-displacement and continuity

Central is designed to meet an existing technological world rather than requiring a replacement world first.

A person's editor, launcher, package manager, automation system, agent harness, filesystem conventions and projects can remain native. Central supplies durable source and stable operation contracts around them. Connectors bind those contracts to technologies that exist on a particular machine.

This lets implementations change without forcing authored meaning to migrate every time:

```text
human-authored ground
        ↓
stable Central Action / Port relation
        ↓
current Connector
        ↓
current platform, tool or service
```

The current Connector can disappear and another can take its place without retroactively changing what the person meant.

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
├── skills/       reusable agent procedures for working with Central
├── Work/         ordinary work, with optional ProjectCentral per Project
│   └── <project>/ProjectCentral/
│       ├── user/          human-owned Project authorship aperture
│       ├── agents/wiki/   Agent-maintained Project Wiki
│       └── relations/     accepted source/provenance relations when needed
└── .central/     derived local state subordinate to authored source
```

The compact dependency rule is:

> Authored source says what should persist. `ctrl` says what can be done. Connectors say how it can be done here. Agent Wikis can maintain knowledge around that source without becoming its author.

The sentence is useful because the responsibilities remain separate. Human source carries authored meaning; an Action gives the world a stable operation surface; a Connector answers the local implementation question; Agent knowledge remains revisable knowledge about/across source. None of those layers is allowed to impersonate the others.

## What changes for a human

A person can recover a new machine or agent environment without having to reconstruct themselves from application settings and scattered memories. They can inspect the source directly, edit it without a special UI, keep project-specific material with the Project, and decide explicitly when an observed pattern is worth carrying forward.

Inside `Work/<project>`, ProjectCentral gives a substantial Project an obvious but optional place to author the few high-altitude things the person most needs to remain responsible for: purpose, intended experience, vision, design judgement, plans, important distinctions or recognised changes of direction. The human can structure that material naturally; Central does not generate a documentation template.

Existing native source does not need to move into ProjectCentral. A source can be explicitly recognised and retained in place.

The intended result is not more personal configuration or documentation work. It is **less repeated re-authoring of the same technological life**, while Agents carry more of the burden of maintaining navigable knowledge around the source.

## What changes for an agent

An agent can enter a world with a stable, permission-bounded authored ground rather than treating every session as a blank prompt or silently learning a replacement profile.

Availability still does not imply disclosure. A file can exist, be indexable and be retrievable without being loaded into every context. Central supplies the source; systems such as AIKit can decide what is relevant and permitted for the current act.

ProjectCentral also gives the agent a stable distinction between:

```text
recognised human Project source
Agent-maintained Wiki knowledge
native design / architecture / code
observed evidence / current development state
inference
```

A role-like filename can help discovery without proving authorship. `ProjectCentral/user` is a human-owned aperture, but machine-readable authorship is not inferred from location alone; an AI-generated suggestion does not become human-authored merely because it landed there.

Agents can invoke the same canonical Actions humans use. A launcher, shell command, agent tool and future UI do not need separate meanings for the same operation.

## Relation to the wider {O:I} field

**O:I** is the whole field of technological agency. Central is the durable authored ground within that field, not the owner of every surrounding capability.

**Actuation** defines how situated agency, delegation, authority and Return are constituted. Central can be the world in which an Agency is grounded without becoming the agency runtime.

**AIKit** resolves what is available to an actor now — capabilities, sources, models, sessions, Surfaces and other resources. Central supplies authored ground and ProjectCentral Wiki/source identities that AIKit can make addressable without collapsing availability into automatic prompt injection.

**Software Factory** develops projects from authored intention through design, implementation, evidence and Recognition. Project-specific authored ground and Agent Wiki knowledge can be entered through ProjectCentral while Factory retains ownership of Run/Return state.

**Workcell** materialises computational worlds. Central can state durable machine intent while Workcell owns runtime placement, services, bindings and lifecycle.

**Quaternal Logic** can treat Central material as a subject of formal or semantic inquiry where requested; Central does not require QL in order to remain a valid authored root.

## Product principles

1. **Human authorship is explicit.** Observation, inference and generated suggestions do not silently become authored source.
2. **Authored continuity outranks implementation convenience.** A tool may improve access without becoming source authority.
3. **Control stays high-signal.** Persistent material belongs at the narrowest scope where it remains correct.
4. **Availability does not imply disclosure.** Existing or indexed information need not be loaded into every agent context.
5. **Process and context stay distinct.** Skills contain reusable procedure; Control can state durable preference or intent about procedure.
6. **Actions have stable identity.** Human and software Surfaces can invoke the same operation.
7. **Core code depends on abstractions.** Ports state required ability; Connectors bind it to a real environment.
8. **Derived state stays subordinate.** Caches, indexes, accounts, projections and observations do not outrank authored source.
9. **Existing Projects remain native.** ProjectCentral can relate useful source in place rather than requiring wholesale migration.
10. **Extensions are open-ended.** New environments should be supportable through the public SDK and conformance contracts.
11. **The real installation tests the architecture.** First-party extensions must use the same public seams available to others.

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

ProjectCentral lifecycle Actions create the Project-local `user/` + `agents/{governance,wiki}` relation without generating authored prose. Authored-ground inspection is available as:

```sh
ctrl --root /path/to/Central projectcentral ground inspect <work-project>
ctrl --root /path/to/Central projectcentral ground plan <work-project>
```

The structured `projectcentral.ground.apply` Action records an explicitly accepted source/provenance/standing relation without changing source bytes or path.

Current main is the authority for implemented behaviour. Open extension PRs and physical-machine acceptance work remain development state until accepted; they should not be read back into the product vision as completed capability.

## Documentation

Read the package in this order:

1. [`docs/CENTRAL-VISION.md`](docs/CENTRAL-VISION.md) — why the authored root exists and the experience it should preserve.
2. [`docs/CENTRAL-SYSTEM-SPEC.md`](docs/CENTRAL-SYSTEM-SPEC.md) — normative product and architecture specification.
3. [`docs/CONTROL-CONTENT-PROTOCOL.md`](docs/CONTROL-CONTENT-PROTOCOL.md) — authorship, durable information and disclosure boundaries.
4. [`docs/PROJECTCENTRAL-CONTRACT.md`](docs/PROJECTCENTRAL-CONTRACT.md) — recursive Project-local human-source / Agent-Wiki filesystem and identity contract.
5. [`docs/PROJECTCENTRAL-AUTHORED-GROUND.md`](docs/PROJECTCENTRAL-AUTHORED-GROUND.md) — optional authored-ground UX, accepted source relations, account handoff and return law.
6. [`docs/PRODUCT-GROUND-CONVENTION.md`](docs/PRODUCT-GROUND-CONVENTION.md) — optional human-authored `Control/user/products/<product>/` convention for cross-context product ground.
7. [`docs/PERSONAL-WORLD-PROJECTION.md`](docs/PERSONAL-WORLD-PROJECTION.md) — selected personal/world Projection, public disclosure and explicit return to Central source.
8. [`docs/CONNECTOR-SDK-SPEC.md`](docs/CONNECTOR-SDK-SPEC.md) — Action, Port, Connector, Surface, SDK and conformance architecture.
9. [`docs/PERSONAL-EXTENSION-SPEC.md`](docs/PERSONAL-EXTENSION-SPEC.md) — first real extension set used to harden the public architecture.
10. [`docs/INSTALL.md`](docs/INSTALL.md) — native `ctrl` installation and clean-root verification.
