---
name: machine-declaration
description: Review a real environment and propose a portable human-authored Central machine-role declaration without confusing observed state with intended state.
---

# Machine declaration

Use this Skill when the user wants to describe, review, bootstrap, or revise a durable machine role under `Control/machines/`.

A machine declaration is **authored intent**. A machine inspection is **observed current state**. They are deliberately different sources of authority:

```text
existing authored role ──────┐
                             ├── review + reasons ──► proposed authored role
current host observation ────┘                            │
                                                         ▼
                                                explicit human acceptance
                                                         │
                                                         ▼
                                                   Control/machines
```

Never turn an installed-package inventory, host name, current service list, or successful inference directly into Control source. Observation can support a proposal; it cannot silently promote itself into authored truth.

Read these before proposing a machine declaration:

1. `docs/CONTROL-CONTENT-PROTOCOL.md`, especially `Control/machines/`, source classes, durable change proposals, and the Machine declaration Skill section.
2. `docs/CENTRAL-SYSTEM-SPEC.md`, especially the authored-source/Observation invariant, machine Actions, Port contracts, and Connector eligibility.
3. `docs/PERSONAL-EXTENSION-SPEC.md` when reviewing one of the proving personal environments.
4. The current public machine declaration type and machine Actions in `ctrl`.
5. `skills/connector-authoring/SKILL.md` only when the review discovers a missing extension capability.

## Procedure

### 1. Establish the durable role, not the transient host identity

Name the intended environment by a durable role such as:

```text
primary-workstation
home-server
portable-laptop
```

A current hostname, IP address, serial number, runner ID, or temporary VM name is an observed binding unless the human has a durable reason to author it.

Record the target role and the user's stated purpose for it. If the role already exists, **read the existing machine-role source before proposing changes**. Use the canonical `machine.declaration <role>` Action where available, or read the authored file directly when the Action cannot run.

Do not start by generating a replacement declaration from the current machine.

### 2. Keep three records distinct during the review

Maintain three explicit working sections:

```text
A. Existing authored intent
B. Current observation
C. Proposed authored intent
```

For A, cite the Control source path and preserve the authored values exactly.

For B, use `machine.inspect` or the relevant public `MachineInspector` path and record its Connector/provenance. Current observations can include platform, architecture, capabilities, packages, configurations, services, host bindings, and other supported measured state.

For C, include only statements that are intended to remain true for the durable role. Each proposed durable statement must have a reason. A statement can be supported by user intent, existing authored source, a portability requirement, or an observation that the user explicitly chooses to retain as intent.

Never copy B wholesale into C.

### 3. Review purpose before mechanism

Ask what the role must provide, not merely what happens to be installed today.

Prefer portable intent such as:

```text
home-server requires package X to be present
primary-workstation requires configuration Y
service Z should be running and enabled
```

over provider implementation details such as:

```text
use one particular package manager because it is installed today
run this private shell command
encode a provider-specific cache path
```

A declaration can reference a real package/configuration source when that reference is itself part of the intended portable state. The provider mechanism that satisfies the declaration belongs behind a public Port/Connector.

### 4. Identify the public Ports implied by the intended state

Translate desired machine behavior into public capability seams before selecting implementations.

The current machine reconciliation field includes:

```text
MachineInspector
    observe the current environment

PackageManager
    preview/apply package presence differences

ConfigurationManager
    preview/apply portable configuration differences

ServiceManager
    preview/apply service running/enablement differences
```

Treat these as semantic capabilities, not preferred-tool names.

For every proposed requirement, record:

- requirement kind and stable identifier;
- intended state;
- public Port required to reconcile a difference;
- any authored source reference;
- why the requirement belongs in durable role intent.

`MachineInspector` is required to compare the authored role with a real host. A mutating Port is required only where that class of intended difference must be reconcilable.

### 5. Inspect Connector coverage through normal resolution

Use `machine.plan <role>` and/or public `ConnectorRegistry` diagnostics to determine implementation coverage.

For every required Port distinguish:

```text
eligible
    at least one Connector is eligible here; record the selected Connector and diagnostics

available elsewhere / ineligible here
    implementation exists but the current target fails platform, dependency, or configuration eligibility

missing
    no eligible Connector currently supplies the required Port
```

Connector eligibility is observation about the present operative environment. It is **not machine declaration content** unless the human separately authors a durable provider preference or requirement at an appropriate scope.

The proposal should make coverage inspectable, for example:

```text
PackageManager
  selected: personal.homebrew
  reason: eligible on this host

ConfigurationManager
  selected: none
  reason: no eligible Connector implements the required Port
```

Do not hide missing coverage by deleting the intended requirement from the declaration.

### 6. Hand missing extension work to Connector authoring

When intended state requires a public Port but no eligible Connector exists, produce an explicit extension handoff:

```text
missing Port
+ target environment facts needed by an implementer
+ intended operation
+ relevant declaration requirement
→ skills/connector-authoring/SKILL.md
```

Do **not** embed provider setup instructions, private shell recipes, implementation code, or provider-specific workaround fields into the machine declaration to make the gap disappear.

If the desired capability does not fit an existing Port semantically, classify it as a Port-contract question before authoring a Connector. Do not invent a personal exception in the machine source.

### 7. Produce a review packet before any durable mutation

The output of the Skill is first a proposal, not an edit.

Include:

```text
Target
  role
  current authored source path (if any)

Existing authored intent
  exact current declaration or "none"

Current observation
  observation source / Connector
  relevant observed values only

Proposed declaration
  complete reviewable declaration

Reasons
  one reason for each added, removed, or materially changed durable statement

Required Ports
  Port → requirements that need it

Connector coverage
  eligible/selected and missing/ineligible Ports with diagnostics

Extension handoffs
  missing Port → connector-authoring task, where needed

Final diff
  existing authored source → proposed authored source
```

The packet must make it possible for the human to distinguish **what is**, **what is intended**, and **what implementation is currently available** without inference from prose.

### 8. Require explicit acceptance before writing Control

Show the complete proposed declaration and final diff. Ask the human to accept, revise, or reject it.

Only an explicit acceptance authorizes durable mutation of `Control/machines/`.

If the user asks only for an audit or proposal, stop at the review packet. Do not treat silence, successful planning, current observation, or an agent's confidence as acceptance.

After an accepted edit, re-read `machine.declaration <role>` to prove the authored source is valid. This validates syntax/structure; it does not mean the host already satisfies the intent.

### 9. Compare and plan without weakening intent

Run `machine.plan <role>` after the proposal is accepted, or against a temporary review fixture when proving the proposal before mutation.

Classify plan entries as the canonical machine planner does:

- satisfied;
- changeable;
- missing capability;
- unsupported.

A missing Connector is implementation work, not evidence that the authored requirement was wrong. An unsupported requirement can reveal either a genuinely unsupported intent or a public-contract gap; classify it before changing the declaration.

### 10. Preserve provenance in completion evidence

Record enough evidence to reconstruct the review:

- role reviewed;
- existing authored source and revision/path;
- observation source/Connector and target context;
- relevant observations;
- intended requirements and reasons;
- required Ports;
- eligible/selected Connectors;
- missing Ports and extension handoffs;
- proposed declaration;
- final diff;
- human acceptance if mutation occurred;
- post-write declaration validation;
- resulting `machine.plan` summary where applicable.

Do not report “machine configured” merely because a declaration was written. Declaration authoring, planning, reconciliation, and verification are different operations.

## Decision rules

### Observation is not intent

```text
observed: package foo is installed
```

does not entail:

```text
authored: package foo must remain installed
```

Ask whether the role needs it. Keep it out if the answer is no or unclear.

### Intent is not provider choice

```text
authored: package foo must be present
```

does not entail:

```text
provider: Homebrew / apt / another implementation
```

Port resolution chooses an eligible implementation. Provider preference belongs elsewhere unless the provider itself is genuinely part of durable intended semantics.

### Missing capability is not permission to smuggle implementation into Control

If `machine.plan` says a difference needs `ConfigurationManager` and none is eligible, preserve the intended configuration requirement and create a Connector-authoring handoff.

### Existing source has standing

An existing declaration is authored source. Do not overwrite it because current observation differs. Explain the discrepancy and propose the smallest justified authored change.

## Completion checklist

A complete machine-declaration review can answer all of these from evidence:

- What durable role is being reviewed?
- What did its existing authored source say before the review?
- What did the target environment actually report, and through which observation path?
- Which observed facts are merely current state?
- What does the user intend the role to provide durably?
- Why does each proposed declaration statement belong there?
- Which public Port owns each reconcilable class of difference?
- Which Connectors are currently eligible and selected for those Ports?
- Which Ports remain missing or ineligible?
- Was missing implementation handed to `connector-authoring` rather than embedded in Control?
- What exact diff is proposed?
- Did the human explicitly accept any durable mutation?
- If accepted, does `machine.declaration` validate the resulting authored source?

## Repository proof

`ctrl/tests/machine_declaration_skill.rs` provides two executable fixtures around this procedure:

1. a `primary-workstation` review where a package difference is observable and an eligible reference `PackageManager` makes the plan changeable through the public Port;
2. a `home-server` review where a configuration difference is observable but no `ConfigurationManager` is eligible, so the plan preserves the authored requirement and reports a missing public capability suitable for Connector-authoring handoff.

The fixtures prove that existing authored source, current observation, Port demand, Connector eligibility, and missing implementation remain separate dimensions of one review.
