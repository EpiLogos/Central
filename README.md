# Central

Central is a Rust command-line tool, `ctrl`, together with a plain-file layout for a personal root folder, usually `~/Central`. The root keeps what a person has written about themselves, their agents and their machines in `Control/`, and their projects in `Work/`. `ctrl` gives people and agents one set of named, typed **Actions** for reading and changing that material. Central holds O:I's **ground** facet: what a person has written, the projects under way, and what carries over from one session to the next.

An *Action* is a stable, named operation, such as `work.search`, `central.now.read` or `machine.adopt-current`. It declares its inputs, whether it only reads or also changes something, and the structured result it returns. The same Action means the same thing whether a person, a script, a launcher or an agent calls it.

## What it does today

Everything Central keeps is an ordinary file that you can read and edit with any editor; `ctrl` is the shared doorway for software, not a gate. The installed `ctrl` (0.1.0) exposes **188 Actions** (`ctrl actions`): 94 read-only, 91 that change local files, and 3 that act outside the root (`work.open`, `work.reveal`, `central.recover`). They group as:

| Area | What it covers | Example Actions / commands |
|---|---|---|
| Root and health | create, locate and check a personal root | `ctrl init`, `ctrl root`, `ctrl doctor` |
| Projects | find and enter projects under `Work/` | `ctrl work list \| search`, `ctrl pick`, `work.*` |
| Authored ground | read and search what the person wrote in `Control/` | `ctrl control open \| search \| index` |
| Project ground | give an ordinary project its own `ProjectCentral/` folder | `projectcentral.init \| inspect \| doctor \| adopt \| migrate`, `projectcentral.source.*` |
| Files | create and change ordinary files with compare-and-swap writes and revision history | `central.files.*`, `central.document.*`, `central.flow.*` |
| Session continuity | the **NOW** field (live work records for the current day) and the **DAY** close (the dated snapshot when a day ends), at root and per project | `central.now.*`, `central.day.*`, `projectcentral.now.*` |
| Results from agents | agent proposals that a person accepts or rejects before they enter the ground | `projectcentral.source.return*`, `central.receiving.*`, `central.remember` |
| Agents | reusable agent definitions proposed from a stated intent and accepted by the person | `agent-profile.*`, `central.agent-set.*` |
| Machines | declare what each machine is for, compare with what is there, and recover it | `ctrl machine inspect \| account \| adopt-current \| declaration \| plan \| apply \| verify`, `ctrl recover <ROLE>` |
| Discovery | list and describe every Action | `ctrl actions`, `ctrl action describe <ID>`, `ctrl action run <ID> '<json>'` |

Central keeps three kinds of claim apart. **Authored** material is what the person deliberately states, adopts or keeps. **Observed** material is what software measures. **Inferred** material is what software derives from evidence. Records written by agents carry a standing such as `generated-proposal` and stay unrecognised until the person accepts them. Agent sessions cannot write directly into human-authored source; they propose.

**Works now** (CI green on `main`; about 830 Rust tests; read-only commands checked on a live root): root init, doctor and the `mixed_root` check; work discovery; Control open, search and index; ordinary-file Actions; ProjectCentral lifecycle and source Actions; root and project NOW/DAY; agent returns and receiving; machine declaration, adoption, planning and recovery.

**Limits, stated plainly:**

- **Builds.** All builds are pre-releases. Hosted CI accepts the specification's criteria, but physical acceptance on named machines (a workstation and a home server) has not been done.
- **Host-specific connectors.** The stock `ctrl` has no connector for opening or revealing files in the OS, or for reading git state. `work open`, `work reveal` and `ctrl git census | tree | graph` therefore report `unavailable_capability`. They work under the macOS host binary `ctrl-macos`, which is built from `surfaces/macos-host` and is not yet released. `ctrl actions` lists these Actions as available regardless.
- **Unmounted connectors.** The Homebrew and chezmoi connectors pass their conformance tests but are not mounted in any shipped binary.
- **Search on large files.** `ctrl control search` currently fails on a root that contains a source file larger than 4 MiB.
- **In flight.** Open pull requests are listed on GitHub; this page does not track them. NOW/DAY, machine and recovery work is on `main`.

## How it fits O:I

O:I gives each facet of an agent's world its own product. Central holds **ground**: the material every other product reads from but does not own. Other products reach it by running `ctrl --json action run <ID> '<json>'` and by reading its files. Central depends on no other suite product.

| Neighbouring facet | Where they meet |
|---|---|
| Agency / Actuation | Central defines World Positions (`central:position:<world>:<slug>`) that Actuation's occupancy ledger records. Central's harness connector reads `actuation harness detect` and `harness capability` for `machine.inspect`. Actuation writes NOW handoffs through `projectcentral.now.*`. |
| Capability / AIKit | AIKit reads authored ground, NOW and DAY through Actions such as `central.world.here`, `central.now.read`, `central.day.ensure` and `agent-profile.read`, and indexes Central's file maps for knowledge search. The wiki grammar (`okf-wiki/v1`) is AIKit's. `machine.plan` and `machine.verify` take AIKit's worktree-projection reading. Availability is not disclosure: a file existing in Central does not put it in every prompt. |
| Development / Software Factory | Factory allocates NOW records, submits finished work for review (`central.receiving.*`), appends to Flows and reads files through Central Actions. A person reviews Factory's results through Central. |
| Environment / Workcell | `machine.adopt-current` binds a machine to a Workcell (`Control/machines/<role>.json`). `central.now.workcell-root` keeps a NOW per Workcell. The harness connector reads Workcell's instance registry. |
| Reflection / QL | No code dependency. QL reads NOW and DAY through Central Actions where a host passes them in. |
| O:I | `oi install central` and `oi init --personal-ground PATH` set up a root through `ctrl`. `oi central …` (alias `oi ctrl …`) dispatches to `ctrl`. `ctrl system` and `ctrl config-contribution` emit O:I's disclosure contracts. |

**In the Cradle.** In the O:I desktop, Central is the World. It holds real files and real ground, edited normally with full history. Every source reference the Cradle uses is Central's, and every write it makes goes through a Central Action as a compare-and-swap.

## Install and quick start

Through O:I (ordinary route; a personal root needs a current `ctrl`, so this builds Central from source and needs Rust):

```sh
oi install central
oi init --personal-ground "$HOME/Central"
```

From source (developer route; Git and current stable Rust):

```sh
git clone https://github.com/EpiLogos/Central ~/Central/Work/Central
cargo install --path ~/Central/Work/Central/ctrl
```

Release archives (`central-v0.1.0-prelocal.6`, Apple Silicon macOS and x64 Linux) contain `ctrl` only. They predate the root NOW/DAY Actions and are not accepted by `oi init` for a new root.

First commands:

```sh
ctrl --root /path/to/Central init                 # idempotent; creates the empty base shape
ctrl --root /path/to/Central doctor --json
ctrl --root /path/to/Central action list --json
ctrl work list
ctrl control search <term>
ctrl --json action run central.now.list '{}'
ctrl action describe projectcentral.init
```

`ctrl` finds the root from `--root`, then `CENTRAL_ROOT`, then `$HOME/Central`. Subcommands have no `--help` of their own: use `ctrl --help`, [`docs/CLI-REFERENCE.md`](docs/CLI-REFERENCE.md) and `ctrl action describe <ID>`.

---

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

## Initial shape and ProjectCentral

Central is implemented in **Rust**. Executable product code, the public SDK, Connectors and executable product/conformance harnesses use Rust; ordinary data and authored source remain in representations appropriate to their meaning.

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

NOW and DAY are created later, by `central.day.ensure` at the root and `projectcentral.now.init` in a project, not by `init`.

Current `main` is the authority for implemented behaviour. Open pull requests remain development state until they are accepted, and they should not be read back into the product vision as completed capability just because their repository tests are green.

## What changes for a human

A person can recover a new machine or agent environment without having to reconstruct themselves from application settings and scattered memories. They can inspect the source directly, edit it without a special UI, keep project-specific material with the project, and decide explicitly when an observed pattern is worth carrying forward.

The intended result is not more personal configuration work. It is **less repeated re-authoring of the same technological life** as tools change.

## What changes for an agent

An agent can enter a world with a stable, permission-bounded authored ground rather than treating every session as a blank prompt or silently learning a replacement profile.

Availability still does not imply disclosure. A file can exist, be indexable and be retrievable without being loaded into every context. Central supplies the source; systems such as AIKit can decide what is relevant and permitted for the current act.

Agents can also invoke the same canonical Actions humans use. A launcher, shell command, agent tool and future UI do not need separate meanings for the same operation.

The recursive ProjectCentral relation also gives an agent a stable Project-local distinction between human source, Agent governance, and maintained Agent Wiki material. The root Wiki can federate those Project WikiSpaces without turning every Project into one database or requiring existing source to move.

## Relation to the wider O:I field

**O:I** is the whole field of technological agency. Central is the durable authored ground within that field, not the owner of every surrounding capability.

**Actuation** defines how situated agency, delegation, authority and Return are constituted. Central can be the world in which an Agency is grounded without becoming the agency runtime.

**AIKit** resolves what is available to an actor now — capabilities, sources, models, sessions, Surfaces and other resources. Central supplies authored ground that AIKit can make addressable without collapsing availability into automatic prompt injection.

**Software Factory** develops projects from authored intention through design, implementation, evidence and Recognition. Project-specific canon stays with the Project; cross-context durable personal ground can remain in Central.

**Workcell** materialises computational worlds. Central can state durable machine intent while Workcell owns runtime placement, services, bindings and lifecycle.

**Quaternal Logic** can treat Central material as a subject of formal or semantic inquiry where requested; Central does not require QL in order to remain a valid authored root.

## Repository layout

| Path | What it holds |
|---|---|
| `ctrl/` | the core: Action registry, CLI and filesystem protocol (binary `ctrl`) |
| `crates/connector-sdk/` | the public SDK: the `Connector` trait, Port traits and conformance harnesses |
| `connectors/` | reference (always mounted), harness, macOS, Shortcuts, git-sync, Homebrew, chezmoi, Ubuntu, and a template for new connectors |
| `surfaces/macos-host/`, `hosts/ubuntu/` | the `ctrl-macos` and `ctrl-ubuntu` host binaries, which mount platform connectors |
| `surfaces/raycast/`, `surfaces/shortcuts/` | a Raycast extension and Shortcuts guidance; both call `ctrl-macos` |
| `skills/`, `skillsets/` | agent procedures for operating and extending Central |
| `docs/` | the documentation corpus |

## Documentation

Start at the docs front door: [`docs/README.md`](docs/README.md) — it indexes the corpus by role and gives explicit reading routes.

The primary route for a new reader:

1. [`docs/CENTRAL-VISION.md`](docs/CENTRAL-VISION.md) — why the authored root exists and the experience it should preserve.
2. [`docs/CENTRAL-SYSTEM-SPEC.md`](docs/CENTRAL-SYSTEM-SPEC.md) — normative product and architecture specification.
3. [`docs/CONTROL-CONTENT-PROTOCOL.md`](docs/CONTROL-CONTENT-PROTOCOL.md) — authorship, durable information and disclosure boundaries.
4. [`docs/CONNECTOR-SDK-SPEC.md`](docs/CONNECTOR-SDK-SPEC.md) — Action, Port, Connector, Surface, SDK and conformance architecture.
5. [`docs/INSTALL.md`](docs/INSTALL.md) — native `ctrl` installation and clean-root verification.
6. [`docs/CLI-REFERENCE.md`](docs/CLI-REFERENCE.md) — the stock `ctrl` surface, exit codes and root resolution.
7. [`docs/PROJECTCENTRAL-NOW.md`](docs/PROJECTCENTRAL-NOW.md) — the NOW / DAY contract at both registers.
8. [`docs/ARCHITECTURE-NAVIGATION.md`](docs/ARCHITECTURE-NAVIGATION.md) — from governing source to the native operations that implement it.

---

## Background

O:I stands for Objective : Internality. It names the means through which a life knows and acts within a world: memory, language, tools, permissions and other people. Those means are internal because every act proceeds through them, and objective because each can be examined and changed. Central keeps one of those means, the ground a person has written, as a source that the person owns. The idea is developed in the essay [*Confronting the Limit: Determination, Subjectivity and Mind as Objective Internality*](https://oi.epi-logos.org/essay/).

Central is a **human-owned operating root for a technological life**.

It exists so that a person's working world can remain recognisably theirs while models, agent runtimes, applications, interfaces, machines and infrastructure change around it.

That continuity cannot safely be delegated to whichever tool happens to be current. Agent products can observe a person, infer patterns and maintain their own state, but those are not the same thing as the person deliberately saying: **this is part of the ground I want my technological world to carry forward**.

Central therefore gives ordinary authored source a durable home, keeps ordinary work ordinary, and defines stable Actions through which humans and software can operate on that world without taking ownership of it.

Central is not a configuration product. Machine configuration is one thing a durable authored ground may express. The larger product is the relation between **human authorship, continuity, ordinary work and changing technological implementations**.
