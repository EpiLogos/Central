# Central — Product Vision

**Status:** product vision

## Purpose

Central is a human-owned operating root through which a person's technological world can remain recognisably theirs while the technologies around it change.

A model can be replaced. An Agent harness can disappear. A launcher, editor, package manager, machine or interface can change. Projects can move. What should not have to be rediscovered from scratch each time is the durable authored relation between the person and that world: what they deliberately want carried forward, what a Project is trying to become, how they want Agents to meet them, what their machines are for, and which ordinary work remains theirs independently of the software currently presenting it.

Central exists to give that continuity an ordinary, inspectable source and to let changing knowledge accumulate around it without quietly replacing its authority.

It has four direct product functions:

1. **Preserve authored ground.** Control and ProjectCentral give human-authored meaning a durable ordinary source.
2. **Separate authored meaning from changing knowledge.** Agent Wikis can maintain what is currently known about/across that source without impersonating its author.
3. **Provide stable operation.** `ctrl` exposes canonical Actions whose identity can survive changes in interface and provider.
4. **Keep work ordinary.** Work remains normal filesystem work rather than becoming captive to a proprietary personal-world format.

Machine configuration is one consequence of this architecture, not its definition. Central is not trying to become a universal settings database, documentation suite, memory product or profile store. Its subject is **authored continuity across changing technological agency**.

## Preserve human altitude

The human consequence matters more than the directory shape.

As artificial development capacity grows, a person should not have to spend a corresponding amount of attention on context repair, repeated explanation, routine technical approval, source navigation, state reconstruction and evidence bookkeeping. A system can keep a human continuously "in the loop" while still consuming their attention at the wrong level.

Central provides durable authored ground so the surrounding system can resolve more without repeatedly asking the person to restate what is already theirs.

The human should normally be able to concentrate attention where authorship is consequential:

```text
what is worth making
why it matters
what experience should exist
which distinctions should survive
meaningful design judgement
consequential change of direction
Recognition of returned reality
```

Agents and deterministic systems can carry more of the recoverable developmental body around those determinations:

```text
source discovery
codebase navigation
implementation facts
indexes and Wiki maintenance
tests / evidence / current-state reconstruction
routine reversible engineering
environment mechanics
```

This is not an absolute division of labour. Agents can participate in visioning, design and drafting; humans can work at any technical depth. The distinction is about **attention and authority**. Generated work can help form a human position without automatically becoming one, and reversible machinery should not become human approval work merely because a human can be asked.

## Commission, development and Recognition

The Software Factory vocabulary of Commission and Recognition names the human relation Central helps preserve.

```text
COMMISSION
    authored direction or an already-determinate request/source
        ↓
DEVELOPMENT
    Agent judgement + deterministic machinery + evidence
        ↓
RETURNED REALITY
    implementation, experiment, prototype, resistance, consequence
        ↓
RECOGNITION
    accept / return / redirect / develop intent / leave ground unchanged
```

Commission does not require a ceremony when intention is already stated. Central should help recover existing source before asking the human to state it again.

Recognition is also not equivalent to merge approval or test greenness. It becomes important when returned reality changes what the Project means, how it should feel, what direction is worth taking, or whether the original position should develop.

Central owns neither Factory Runs nor Recognition objects. Its role is to keep the authored side and the possible source-return destination durable enough for that developmental relation to remain intelligible.

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

An observed pattern may be accurate without being something the person wants to define future interactions. An inferred preference may be useful in one context without deserving cross-context persistence. Central therefore allows observation and inference to support a provenance-bearing proposal while reserving the transition into authored ground for actual human authorship/adoption.

The separation prevents convenience from becoming quiet dispossession. A system that learns faster should not thereby acquire the right to rewrite the durable source through which the person or Project is represented to future systems.

The same distinction protects product meaning from implementation hindsight:

```text
vision
    tells us what is meant
    does not prove present capability

code
    tells us what exists now
    does not retroactively author why it exists

evidence
    tells us what happened under a condition
    does not automatically determine direction

Agent synthesis
    can relate all of these
    does not inherit their authority
```

## Why ordinary source matters

Authored ground is ordinary source because durable meaning should remain accessible without the application that last edited it.

A person should be able to inspect and change their ground with normal filesystem tools. Source can be versioned, backed up, searched or projected by optional systems. Those systems can improve use without becoming canonical merely because they maintain an index or richer UI.

At personal scope the recursive relation is:

```text
Control/user/**
    human-authored personal ground

Control/agents/governance/**
    human-authored recurring Agent relation

Control/agents/wiki/wiki.json
    Agent-maintained knowledge around/across the personal world
```

At Project scope the same distinction appears as:

```text
ProjectCentral/user/**
    human-owned Project authorship aperture

ProjectCentral/agents/governance/**
    Project-local human Agent governance

ProjectCentral/agents/wiki/wiki.json
    Agent-maintained Project knowledge
```

Derived indexes, accounts, projections and current interfaces remain subordinate readings or operational state. They can disappear and be rebuilt. Authored meaning should not disappear with them.

## Start where the Project already is

Central is designed to meet an existing working life.

It does not require a specific operating system, launcher, editor, package manager, configuration manager, Agent harness or automation product. Existing Projects remain ordinary directories. Existing tools can remain authoritative for the operations they already own.

Project bootstrap should therefore recover before it questions:

```text
existing repo / local Project / fresh Project
        ↓
inspect existing source and ProjectCentral state
        ↓
recover what is already known
        ↓
establish only genuinely missing authored ground
        ↓
Project is meaningful to enter for human + Agent
```

A mature Project may already have a README, vision, design corpus or other native canon. Central must not infer authorship merely from a filename, but it can help the human recognise and retain those sources in place. A fresh Project may naturally call for more explicit purpose/experience/design authoring. A tiny Project remains valid with very little ground.

The target is not that every Project has the same documents. The target is that a Project can preserve the few human determinations that give its changing implementation meaning.

## Recursive learning without source collapse

The Agent Wiki is valuable because a Project should be able to learn without making the Agent's latest understanding the new human intention.

```text
human-authored ground
        ↓
Agent Wiki understanding
        ↓
Project development / current evidence
        ↓
Agent Wiki learns changed reality
        ↓
difference / tension becomes legible
        ↓
account / explanation / proposal
        ↓
human judgement where warranted
        ↺
```

Difference is not automatically failure. Returned reality can show that implementation is wrong, design needs to change, an original intention needs refinement, or nothing important should change.

The governing law is therefore:

```text
DIFFERENCE
    ≠ automatic source mutation
```

Agents may maintain authorised Agent-Wiki knowledge. Human-authored ground changes through human authorship/adoption or an accepted return.

This is not only a safety constraint. It is what allows the Project to learn from reality while preserving enough continuity for that learning to mean something.

## Information quality and scope

Persistence has a cost. Material should remain at the narrowest scope where it stays correct.

Project-specific meaning belongs with the Project. Temporary task facts belong with the task. Cross-context durable information can enter Control. Reusable procedure belongs in a Skill, script or other capability rather than being copied into always-present source.

The design aim is a small, high-signal authored ground surrounded by selectively retrievable knowledge, not a total personal data lake or giant prompt.

## Availability is not disclosure

A file can exist in Central without entering every Agent context.

Central distinguishes:

```text
source exists
source is known to exist
source can be retrieved
source is relevant
source is permitted for this use
source is loaded now
source is selected for a reading
source is selected for Projection
source is public
```

This matters both for context quality and for human control. Making durable ground available to an Agentic system does not imply broadcasting all of it into every act. Central owns source identity/treatment; an operative resolution layer such as AIKit can determine what is presently relevant and permitted.

## Stable Actions, replaceable implementations

Central introduces stable seams around an otherwise heterogeneous world:

```text
Authored source   what should persist / remain answerable
Actions           what can be done
Ports             what ability an Action requires
Connectors        how that ability exists here
Surfaces          where a human or software actor invokes it
Work              the ordinary local field in which work continues
```

A repeated operation has one canonical Action identity. A launcher entry, shell command, Agent tool or future UI can invoke the same operation without redefining it.

Core Actions depend on Ports rather than branded products. Connectors implement those Ports for real environments. If a provider changes, the Action and authored meaning do not need to change identity with it.

## What Central changes for a human

A person can recover or move their technological world without reconstructing themselves from scattered application state.

They can keep the things they most need to mean and decide in direct ordinary source, keep Projects native, let Agents maintain an inspectable knowledge world around those sources, and return to authored ground only when changed reality actually warrants renewed authorship.

The success condition is not that the person spends more time maintaining Central. It is that they spend **less time repeatedly teaching and supervising machinery that could have retained the relation already**, leaving more room for purpose, experience, judgement, creative exploration and life away from the system.

## What Central changes for an Agent

An Agent can enter a world with stable authored ground instead of treating each session as blank or silently constructing its own replacement profile.

It can discover permitted material, use canonical Actions, inspect native Project reality, traverse Agent-maintained knowledge, and return provenance-bearing proposals where durable human source might usefully change. Existing authored intent should reduce unnecessary questions; exact source/evidence should resolve what can be resolved; genuine unresolved authorship should be surfaced at the meaningful level rather than translated into incidental implementation approval.

Central supplies the source relation. AIKit owns operational resolution and collaboration procedure. Factory owns developmental Runs/Return. O:I owns selective WorldPresentation/Projection.

## Objective Internality

Central makes one practical part of Objective Internality concrete.

A human or Agent does not need to keep the whole operative world inside biological memory or prompt tokens. Authored source, Agent knowledge, machine observation, Project source and history can remain objectively inspectable structures while becoming part of an actor's operative interior when selectively disclosed.

The important architectural choice is not to collapse those exteriorised structures into one generated profile. Their different authority is useful:

```text
human-authored ground
Agent-maintained knowledge
observed reality
derived/indexed state
Projection/public presentation
```

Their mutual legibility lets an actor work through a richer world while preserving where each claim or determination came from.

## Relation to neighbouring products

Central is one centre in the wider {O:I} field.

- **O:I** holds the whole field and shared relations between independently owned worlds. It can project selected faces of Central without becoming Central's source owner.
- **Actuation** constitutes situated Agency, delegation, federation, bounds and Return. Central may be the authored world in which that agency is grounded; it is not the Agent-composition runtime.
- **AIKit** resolves the operative horizon available now. It can make Central/ProjectCentral sources and WikiSpaces addressable without making all of them ambient context.
- **Software Factory** owns Project development, Runs, evidence, Candidates and Recognition. Central supplies durable Project ground/source relations; it does not calculate Factory development state.
- **Workcell** owns materialisation, placement and lifecycle. Central can state intended machine roles without becoming the execution planner.
- **Quaternal Logic** can refract or study Central subjects when explicitly composed; Central remains fully useful without QL.

## Product experience

The product should support explicit and guided use without changing its semantic core.

```text
ctrl open research-canvas
ctrl projectcentral ground inspect research-canvas
ctrl machine inspect
ctrl doctor
```

The intended human Project experience is not a required wizard. It is an obvious optional invitation:

```text
ProjectCentral ready
Agent Wiki ready
Human ground empty | partial | established

What, if anything, is worth authoring or recognising here?
```

For existing Projects, the system should recover useful source before asking. For new Projects, it can help the human form missing high-altitude ground. For both, ordinary direct file editing remains valid.

## Success condition

Central succeeds when a person can establish or recover one human-owned root, understand it without special software, keep ordinary Projects ordinary, preserve a small amount of consequential authored meaning without turning every observation into identity, let Agents maintain evolving knowledge around that source, and receive changed reality back without losing the distinction between what they meant and what the system currently thinks or does.

The deeper success is attentional: increasing technological agency should let the person spend less time as the system's bookkeeping layer and more time where their authorship and Recognition matter.

## Provenance and implementation

This document is product vision. It explains why the distinctions exist; it does not by itself prove a current implementation claim.

[`CENTRAL-SYSTEM-SPEC.md`](CENTRAL-SYSTEM-SPEC.md) is the normative system specification. [`CONTROL-CONTENT-PROTOCOL.md`](CONTROL-CONTENT-PROTOCOL.md) governs durable content and authorship. [`PROJECTCENTRAL-CONTRACT.md`](PROJECTCENTRAL-CONTRACT.md) governs recursive Project source/Wiki identity. [`PROJECTCENTRAL-AUTHORED-GROUND.md`](PROJECTCENTRAL-AUTHORED-GROUND.md) records the #70 authored-ground implementation contract. Current accepted `main`, repository tests and observed local evidence determine what is implemented now; open issues and PRs remain development state until accepted.
