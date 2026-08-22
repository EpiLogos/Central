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

Observation and inference can propose a change to durable ground. They do not silently become that ground. The difference matters because a pattern detected by an agent may be useful without being something the person wants to define them, their agents, or their machines in the future.

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

## The two worlds

Central deliberately separates two things that are easy to fuse:

```text
Personal root                              Product source checkout
~/Central                                  (this repository)
├── Control/   durable personal ground     ├── ctrl/        executable Actions
│   ├── user/                              ├── crates/      public SDK
│   ├── agents/                            ├── connectors/  Port bindings
│   │   ├── governance/                    ├── skills/      agent procedures
│   │   └── wiki/                          ├── docs/        documentation corpus
│   │       └── wiki.json                  └── .github/     product workflows
│   └── machines/
├── Work/     ordinary Projects
│   └── <Project>/
│       └── ProjectCentral/
│           ├── user/
│           ├── agents/
│           │   ├── governance/
│           │   └── wiki/wiki.json
│           └── project.json
├── .central/ derived local state
└── .obsidian/ local editor state
```

The personal root is the lived authored world: durable `Control/`, ordinary `Work/`, subordinate `.central/` derived state, and — when the person opens the root in Obsidian — local `.obsidian/` editor state. None of those are product repository state. The product checkout is a developer artifact; on a machine whose personal root is `~/Central` it can live at `~/Central/Work/Central`, following the same convention as the other {O:I} suite products under `Work/`.

`Control/agents/wiki/wiki.json` is the root Agent-Wiki federation source. A Project can remain an ordinary heterogeneous directory while `ProjectCentral/` supplies its recursive human-source / Agent-governance / Agent-Wiki relation. ProjectCentral initialization or adoption preserves existing Project material by default and records the relation rather than requiring wholesale migration into a Central-owned content layout.

`ctrl doctor` detects the strong collision of a personal root that is also the Central source checkout and reports it (`mixed_root` in the structured output).

The compact dependency rule is:

> Control says what should persist. `ctrl` says what can be done. Connectors say how it can be done here.

The sentence is useful because the responsibilities remain separate. Control carries authored meaning; an Action gives that meaning a stable operation surface; a Connector answers the local implementation question. None of those layers is allowed to impersonate the others.

## What changes for a human

A person can recover a new machine or agent environment without having to reconstruct themselves from application settings and scattered memories. They can inspect the source directly, edit it without a special UI, keep project-specific material with the project, and decide explicitly when an observed pattern is worth carrying forward.

The intended result is not more personal configuration work. It is **less repeated re-authoring of the same technological life** as tools change.

## What changes for an agent

An agent can enter a world with a stable, permission-bounded authored ground rather than treating every session as a blank prompt or silently learning a replacement profile.

Availability still does not imply disclosure. A file can exist, be indexable and be retrievable without being loaded into every context. Central supplies the source; systems such as AIKit can decide what is relevant and permitted for the current act.

Agents can also invoke the same canonical Actions humans use. A launcher, shell command, agent tool and future UI do not need separate meanings for the same operation.

The recursive ProjectCentral relation also gives an agent a stable Project-local distinction between human source, Agent governance, and maintained Agent Wiki material. The root Wiki can federate those Project WikiSpaces without turning every Project into one database or requiring existing source to move.

## Relation to the wider {O:I} field

**O:I** is the whole field of technological agency. Central is the durable authored ground within that field, not the owner of every surrounding capability.

**Actuation** defines how situated agency, delegation, authority and Return are constituted. Central can be the world in which an Agency is grounded without becoming the agency runtime.

**AIKit** resolves what is available to an actor now — capabilities, sources, models, sessions, Surfaces and other resources. Central supplies authored ground that AIKit can make addressable without collapsing availability into automatic prompt injection.

**Software Factory** develops projects from authored intention through design, implementation, evidence and Recognition. Project-specific canon stays with the Project; cross-context durable personal ground can remain in Central.

**Workcell** materialises computational worlds. Central can state durable machine intent while Workcell owns runtime placement, services, bindings and lifecycle.

**Quaternal Logic** can treat Central material as a subject of formal or semantic inquiry where requested; Central does not require QL in order to remain a valid authored root.

## Product principles

1. **Human authorship is explicit.** Observation and inference do not silently become authored Control material.
2. **Authored continuity outranks implementation convenience.** A tool may improve access without becoming source authority.
3. **Control stays high-signal.** Persistent material belongs at the narrowest scope where it remains correct.
4. **Availability does not imply disclosure.** Existing or indexed information need not be loaded into every agent context.
5. **Process and context stay distinct.** Skills contain reusable procedure; Control can state durable preference or intent about procedure.
6. **Actions have stable identity.** Human and software Surfaces can invoke the same operation.
7. **Core code depends on abstractions.** Ports state required ability; Connectors bind it to a real environment.
8. **Derived state stays subordinate.** Caches, indexes, projections and observations do not outrank authored source.
9. **Extensions are open-ended.** New environments should be supportable through the public SDK and conformance contracts.
10. **The real installation tests the architecture.** First-party extensions must use the same public seams available to others.

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

Current `main` initialization creates the recursive base shape:

```text
Control/user/
Control/agents/governance/
Control/agents/wiki/wiki.json
Control/machines/
.central/
Work/
```

The authored Control apertures begin empty rather than being populated by guessed personal facts. `Control/agents/wiki/wiki.json` is initialized as the root federation source.

For a Work Project, current `main` also exposes the ProjectCentral lifecycle Actions:

```text
projectcentral.inspect
projectcentral.doctor
projectcentral.init
projectcentral.adopt.preview
projectcentral.adopt
projectcentral.migrate.preview
projectcentral.migrate
```

These operations establish `ProjectCentral/user`, `ProjectCentral/agents/{governance,wiki}`, `ProjectCentral/project.json`, provenance, and root-Wiki federation while preserving heterogeneous existing Project source according to the selected operation.

Current main is the authority for implemented behaviour. Open extension PRs — including richer authored-ground, NOW/DAY, governance and physical-machine lines — remain development state until accepted; they should not be read back into the product vision as completed capability merely because their repository tests are green.

## Documentation

Start at the docs front door: [`docs/README.md`](docs/README.md) — it indexes the corpus by role and gives explicit reading routes.

The primary route for a new reader:

1. [`docs/CENTRAL-VISION.md`](docs/CENTRAL-VISION.md) — why the authored root exists and the experience it should preserve.
2. [`docs/CENTRAL-SYSTEM-SPEC.md`](docs/CENTRAL-SYSTEM-SPEC.md) — normative product and architecture specification.
3. [`docs/CONTROL-CONTENT-PROTOCOL.md`](docs/CONTROL-CONTENT-PROTOCOL.md) — authorship, durable information and disclosure boundaries.
4. [`docs/CONNECTOR-SDK-SPEC.md`](docs/CONNECTOR-SDK-SPEC.md) — Action, Port, Connector, Surface, SDK and conformance architecture.
5. [`docs/INSTALL.md`](docs/INSTALL.md) — native `ctrl` installation and clean-root verification.
