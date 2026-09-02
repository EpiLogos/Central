# Central — layered human-authored Agent governance sources

**Status:** Central #72 source/read-model contract  
**Owner:** Central source identity, authorship and filesystem relation  
**Operational consumer:** AIKit ContextSource/Profile/ContextResolution  
**Builds on:** `CONTROL-CONTENT-PROTOCOL.md`, `PROJECTCENTRAL-CONTRACT.md`, #70 authored Project ground, #71 human-altitude framing  
**Whole-context relation:** `PROJECT-CONTEXT-PROTOCOL.md` — governance commonly participates at P2/Praxis while operative disclosure remains an independent AIKit/harness dimension

## 1. Why this source exists

A person may want some judgements about working with software Agents to survive individual Projects, sessions, models and harnesses:

- recurring collaboration expectations;
- evidence and verification standards;
- stable vocabulary;
- initiative/authority boundaries;
- tool or environment quirks worth carrying forward;
- compact architectural or working truths that prevent repeated misunderstanding.

Those are not the same thing as the Agent Wiki's changing knowledge about the world, and they are not the same thing as long reusable procedures.

The human-authored source relation is:

```text
Control/agents/governance/**
    cross-project human Agent relation
            ↓ available to scoped resolution
ProjectCentral/agents/governance/**
    Project-specific human Agent relation
            ↓
AIKit ContextSource / Profile / ContextResolution
            ↓
operative guidance + selected Skills/context
```

Central owns the source identity and scope relation. **AIKit owns operational composition and precedence.**

## 2. Governance is human-authored source; Wiki is Agent-maintained knowledge

Keep the established split visible:

```text
agents/governance/**
    human-authored: how Agents should relate/work

agents/wiki/**
    Agent-maintained: what is known about/across the world
```

The reason is authority, not file organisation. Agent Wiki knowledge can change as implementation and evidence change. Human governance changes because the person authored/adopted a changed working relation.

An observed failure may motivate a governance proposal. It does not silently become governance merely because it happened repeatedly.

## 3. Root and Project scopes

Central exposes two durable authored scopes:

```text
Control/agents/governance/**
    cross-project source

ProjectCentral/agents/governance/**
    Project-local source
```

A later task/session/Focus-specific instruction layer may exist operationally, but it is not a new Central durable source root in this contract.

The existence of the scope relation does **not** mean Central defines a universal precedence algorithm such as “Project always overrides root”. Real composition may depend on AIKit Profile, source eligibility, explicit selection, conflicts, model/harness adaptation and the current act.

Central's read model therefore states:

```text
operational_resolution_owner                 AIKit
operational_precedence_defined_by_central    false
conflicts_must_remain_explainable            true
```

That boundary is deliberate. Source ownership should not silently become runtime prompt ownership.

## 4. Stable source identity

The public Rust read model exposes canonical governance files with deterministic Central refs.

Root source example:

```text
Control/agents/governance/collaboration.md
    → central:agent-governance:root:<stable-path-hash>
```

Project source example:

```text
ProjectCentral/agents/governance/project.md
    → central:agent-governance:<project-id>:<stable-path-hash>
```

The file remains ordinary human-editable source. The ref exists so consumers can preserve source identity/provenance without copying the text into a second canonical store.

## 5. Existing Project instruction files

Projects often already contain Agent-facing instruction files such as:

```text
AGENTS.md
CLAUDE.md
GEMINI.md
.github/copilot-instructions.md
```

A conventional filename is a discovery signal, not proof of human governance authority.

The read model first reports such a file as:

```text
possible-project-agent-governance
provenance: unresolved
```

If the human recognises it as Project Agent governance, Central can record a retain-in-place relation at:

```text
ProjectCentral/relations/governance-relations.json
```

Schema:

```text
central.agent-governance-relations/v1
```

The relation records:

- stable Central governance source ref;
- Project-relative path;
- `human-authored | human-adopted` provenance;
- `retain-native-in-place` treatment;
- optional free semantic roles;
- explicit human-accepted recognition provenance.

Application changes relation metadata only:

```text
source bytes mutated          false
source path mutated           false
relation metadata mutated     true
operational precedence        unchanged
```

Migration is not the default. Native Project source remains native.

A README or another ordinary document may also be explicitly adopted if it genuinely plays the governance role; Central does not classify every README as governance merely because some Projects put instructions there.

### Ambient harness activation is a different dimension

Some harnesses make well-known instruction files operative through their own native discovery/precedence rules. That runtime fact is independent of whether Central has recognised the file as human governance.

The truthful relation may therefore be:

```text
AGENTS.md
    Central semantic standing: possible governance / provenance unresolved
    Codex activation: harness-native auto-loaded

CLAUDE.md
    Central semantic standing: human-adopted Project governance
    Claude Code activation: harness-native auto-loaded
```

The governing law is:

```text
source role
    != source authority
    != disclosure / activation
    != runtime precedence
```

Central should preserve the source/ref/provenance side. AIKit/harness adapters should account for actual activation, scope and precedence in ContextResolution/Explain rather than implying that Central selected text which the harness loaded independently.

## 6. Persistent governance versus Skills

Use governance for relatively stable human facts/care/boundaries/vocabulary/collaboration stance.

Use Skills for reusable procedure.

```text
governance
    “For product-meaning changes, consult recognised authored Project ground before inferring intent from code.”

Skill
    the reusable procedure for provenance-aware product understanding
```

This is a semantic/context-economy distinction rather than a mandatory filename taxonomy.

Governance can point to a Skill. It should not copy a long procedure into always-present context when an addressable capability can carry it.

## 7. Language and self-description

Human governance may define compact recurring terms such as:

```text
Agent
we
Project
source
evidence
proposal
human-authored change
```

The purpose is practical: consistent reference improves collaboration and lets an Agent describe its own actions/state without repeatedly renegotiating vocabulary.

No claim of Agent phenomenal consciousness follows from this. Language conditioning and operative salience are sufficient practical reasons to care about instruction craft.

## 8. Privacy / retrieval treatment

The stock `.no-agent-retrieval` marker applies to governance source exactly as it applies to other Central human source.

A marked subtree remains ordinary human source but is excluded from the stock Agent-facing governance read model.

```text
source exists
    ≠ Agent-readable
    ≠ selected for operative context
```

The Project Context Protocol further distinguishes selected context from harness-native activation. AIKit must preserve source eligibility/treatment when resolving guidance and should report native harness activation separately when it can occur outside Central selection.

## 9. Iterative refinement without instruction sediment

Real interaction can reveal missing or poor governance:

```text
session / Run / interaction evidence
        ↓
recurring friction or useful behaviour
        ↓
classify the cause
    missing fact
    poor procedure
    ambiguous term
    stale instruction
    tool/environment quirk
    model/harness-specific behaviour
        ↓
smallest appropriate proposal
    governance source change
    Skill/procedure change
    narrower Project-local change
    no durable change
        ↓
human review/adoption where governance changes
```

The public read model makes these maintenance laws explicit:

```text
observation automatically mutates governance    false
proposal then human adoption                     true
pruning is normal maintenance                    true
procedures should prefer Skills                  true
```

A one-off failure should not automatically become a permanent global instruction. Deletion is part of governance quality: if removing an instruction does not materially change the desired behaviour/evidence, it may not deserve permanent context cost.

## 10. Relation to authored Project ground

Keep two human-authored domains distinct:

```text
ProjectCentral/user/**
    what the Project means / intends / should become

ProjectCentral/agents/governance/**
    how Agents should relate to/work within this Project
```

A design position should not be duplicated into Agent governance merely because it affects implementation. Governance can instead instruct the Agent to consult recognised authored Project ground when the task touches product meaning.

This keeps the reason for a distinction in its owning source while making the operational consultation rule reusable.

Within the sixfold Project Context Protocol, authored Ground commonly participates at P0, bounded Intent at P3, and governance at P2/Praxis. These positions describe contextual function; they do not replace the source/provenance classes above.

## 11. Public read models

Central exposes:

```text
inspect_root_governance(central_root)
inspect_project_governance(project_root)
plan_project_governance(project_root)
apply_project_governance_relation(project_root, source, provenance, roles)
```

They provide stable source refs, scope, provenance, privacy exclusions, unresolved native candidates, retain-in-place relations, maintenance policy and the explicit AIKit composition boundary.

No new prompt renderer, Profile, precedence engine or Skill store is introduced.

## 12. Acceptance

The contract is healthy when:

1. root and Project governance files are discoverable as distinct stable human source refs;
2. Agent Wiki source is not confused with governance source;
3. an existing `AGENTS.md`-style file is unresolved until human recognition, then can remain native with provenance;
4. harness-native activation of a convention file can be accounted for independently from Central recognition/adoption;
5. relation adoption changes no source bytes/path and no operational precedence;
6. `.no-agent-retrieval` excludes marked source from the stock Agent read model;
7. Central states AIKit as operational resolution owner rather than inventing its own precedence;
8. observed interaction can create proposal pressure but not automatic governance mutation;
9. pruning and Skill extraction remain normal maintenance outcomes;
10. Project meaning and Agent-governance source remain distinct but mutually legible.
