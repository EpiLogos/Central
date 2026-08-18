# Central — Product Vision

**Status:** product vision

## Purpose

Central is a human-owned operating root through which a person's technological world can remain recognisably theirs while the technologies around it change.

A model can be replaced. An agent harness can disappear. A launcher, editor, package manager, machine or interface can change. Projects can move. What should not have to be rediscovered from scratch each time is the durable authored relation between the person and that world: what they deliberately want carried forward, how they want agents to meet them, what their machines are for, and which ordinary work remains theirs independently of the software currently presenting it.

Central exists to give that continuity an ordinary, inspectable source.

It has three direct product functions:

1. **Preserve authored ground.** Control stores material the human deliberately chooses to carry forward.
2. **Provide stable operation.** `ctrl` exposes canonical Actions whose identity can survive changes in interface and provider.
3. **Keep work ordinary.** Work remains normal filesystem work rather than becoming captive to a proprietary personal-world format.

Machine configuration is one consequence of this architecture, not its definition. Central is not trying to become a universal settings database. Its subject is **authored continuity across changing technological agency**.

## Why authorship and continuity matter

Software can observe a user and infer useful patterns. Those faculties are valuable, but they answer a different question from authorship.

```text
Authored
    I deliberately state, adopt or retain this.

Observed
    software measured or discovered this.

Inferred
    software derived this from evidence.
```

An observed pattern may be accurate without being something the person wants to define future interactions. An inferred preference may be useful in one context without deserving cross-context persistence. Central therefore allows observation and inference to support a provenance-bearing proposal while reserving the transition into authored ground for explicit human adoption.

The separation prevents convenience from becoming quiet dispossession. A system that learns faster should not thereby acquire the right to rewrite the durable source through which the person is represented to future systems.

## Why ordinary source matters

Control is ordinary source because durable meaning should remain accessible without the application that last edited it.

A person should be able to inspect and change their ground with normal filesystem tools. Source can be versioned, backed up, searched or projected by optional systems. Those systems can improve use without becoming canonical merely because they maintain an index or richer UI.

This gives Central a durable asymmetry:

```text
human-authored source
        ↓ authoritative
indexes / caches / observations / projections
        ↓ derived
current interfaces and agent contexts
```

Derived systems can disappear and be rebuilt. Authored meaning should not disappear with them.

## Non-displacement

Central is designed to meet an existing working life.

It does not require a specific operating system, launcher, editor, package manager, configuration manager, agent harness or automation product. Existing projects remain ordinary directories. Existing tools can remain authoritative for the operations they already own.

Central introduces stable seams around that world:

```text
Control      what should persist
Actions      what can be done
Ports        what ability an Action requires
Connectors   how that ability exists here
Surfaces     where a human or software actor invokes it
Work         the ordinary local field in which work continues
```

The reason for the separation is continuity. If a Connector changes from one provider to another, the Action need not change identity. If a Surface changes from CLI to launcher to agent tool, the operation need not be redefined. If a project moves on disk, the project does not become a different human purpose merely because placement changed.

## Control

Control is selective human-owned source, not an exhaustive inventory of everything software knows.

```text
Control/
├── user/
├── agents/
└── machines/
```

`user/` can carry durable self-description, interests, context objects, tool-use intent and stable working preferences that genuinely matter across contexts.

`agents/` can carry the recurring relation the human wants with software agents: communication preferences, expected initiative, evidence and verification expectations, coding habits, collaboration style and recognised lessons from repeated work.

`machines/` can carry intended computing environments: machine roles, desired tools, package and configuration sources, services, bootstrap mechanisms and other machine intent.

The authored/observed distinction remains important here. "This is my primary workstation" is authored meaning. The current OS version is an observation. A machine declaration may say what should become true without claiming it is already true.

## Information quality and scope

Persistence has a cost. Material should enter Control when it has durable value across the contexts where it applies.

Project-specific facts belong with the Project. Temporary task facts belong with the task. Reusable procedure belongs in a Skill, script or other capability. Control can state durable intent or preference about those things without absorbing them.

The design aim is a small, high-signal authored ground rather than a total personal data lake.

## Availability is not disclosure

A file can exist in Control without entering every agent prompt.

Central distinguishes:

```text
exists
can be indexed
can be retrieved
is relevant
is permitted for this use
is loaded now
```

This matters both for context quality and for human control. Making durable ground available to an agentic system does not imply broadcasting all of it into every act. Central owns source; an operative resolution layer such as AIKit can determine what is presently relevant and permitted.

## One Action, many Surfaces

A repeated operation has one canonical Action identity.

The same `work.open` Action can be invoked through the CLI, an interactive picker, a launcher, OS automation, an agent tool or a future UI. A Surface presents an Action; it does not own the operation.

```text
                  work.open
             /       |       \
          CLI     launcher    agent
```

This is not primarily an interface convenience. It protects semantic continuity. The person should not have to learn that "open this work" means a different operation merely because the current actor or interface changed.

## Ports and Connectors

Core Actions depend on abilities rather than branded products.

A Port states the ability an Action requires. A Connector implements that Port for a specific environment. This makes extension possible without allowing the current provider to define the product ontology.

```text
Action
  ↓ requires
Port
  ↓ implemented by
Connector
  ↓ binds to
platform / tool / service
```

The public SDK and conformance tests exist so first-party and third-party environments can use the same extension law. The project's own preferred environment must not receive a private shortcut that other implementations cannot reproduce.

## What Central changes for a human

A person can recover or move their technological world without reconstructing themselves from scattered application state.

They can see the durable source directly, distinguish what they authored from what software merely observed, keep work in ordinary form, and decide when a learned pattern deserves to become part of the future ground.

The success condition is not that the person spends more time maintaining Control. It is that they spend **less time repeatedly teaching new technological arrangements what should already have remained theirs**.

## What Central changes for an agent

An agent can enter a world with stable authored ground instead of treating each session as blank or silently constructing its own replacement profile.

It can discover permitted material, use canonical Actions, operate against ordinary Work and return provenance-bearing proposals where durable source might usefully change. It can do so without receiving authorship authority merely because it was able to observe or infer something.

## Relation to neighbouring products

Central is one centre in the wider {O:I} field.

- **O:I** holds the whole field and shared relations between independently owned worlds.
- **Actuation** constitutes situated Agency, delegation, federation, bounds and Return. Central may be the authored world in which that agency is grounded; it is not the agent-composition runtime.
- **AIKit** resolves the operative horizon available now. It can make Central material addressable without making all Control material ambient context.
- **Software Factory** owns Project development, Runs, evidence, candidates and Recognition. Project canon remains project-local unless a genuinely cross-context relation is deliberately promoted into Central.
- **Workcell** owns materialisation, placement and lifecycle. Central can state intended machine roles without becoming the execution planner.
- **Quaternal Logic** can refract or study Central subjects when explicitly composed; Central remains fully useful without QL.

## Product experience

The product should support explicit and guided use without changing its semantic core.

```text
ctrl open research-canvas
ctrl machine inspect
ctrl machine apply
ctrl doctor
```

Guided surfaces can search and select the same Actions.

The shared interaction law is:

```text
find Action
→ supply or select inputs
→ preview where consequence requires it
→ execute
→ receive a structured result
```

## Success condition

Central succeeds when a person can establish or recover one human-owned root, understand it without special software, preserve durable authored meaning without turning every observation into identity, keep ordinary work ordinary, operate through stable Actions across changing Surfaces and providers, and let agents participate without transferring ownership of the human source.

It should become more useful as extensions increase while the conceptual core remains small.

## Provenance and implementation

This document is product vision. It explains why the distinctions exist; it does not by itself prove a current implementation claim.

[`CENTRAL-SYSTEM-SPEC.md`](CENTRAL-SYSTEM-SPEC.md) is the normative system specification. [`CONTROL-CONTENT-PROTOCOL.md`](CONTROL-CONTENT-PROTOCOL.md) governs durable content and authorship. [`CONNECTOR-SDK-SPEC.md`](CONNECTOR-SDK-SPEC.md) governs the extension architecture. Current `main`, repository tests and accepted evidence determine what is implemented now; open issues and PRs remain development state until accepted.
