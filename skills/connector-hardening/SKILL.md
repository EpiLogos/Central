---
name: connector-hardening
description: Turn a reproducible Central extension failure or concrete friction point into a correctly classified regression and a fix at the architecture layer that owns the behavior.
---

# Connector hardening

Use this Skill only when there is a **reproducible failure or concrete friction report** in an Action/Port/SDK/Connector/Surface path. Do not start from a vague desire to “make the integration better”.

The purpose is to turn real extension work into public architectural hardening without creating a personal-stack exception in core.

The governing relation remains:

```text
canonical Action
      ↓
public Port
      ↓
public SDK / conformance
      ↓
Connector
      ↓
real target technology

Surface → canonical Action
```

Read before changing code:

1. the canonical Action descriptor/handler involved, if an Action is involved;
2. the published Port contract and reusable conformance suite;
3. `docs/CONNECTOR-SDK-SPEC.md`, especially Connector hardening and SDK invariants;
4. `skills/connector-authoring/SKILL.md` for the intended public authoring path;
5. the failing Connector or Surface implementation and its target-specific tests;
6. target-system documentation or observed target behavior where the failure depends on the real provider.

An existing personal Connector is evidence and an example. It is not the definition of a Port contract.

## Procedure

### 1. Reproduce before classifying

Start with one concrete failure/friction record containing:

```text
operation
  canonical Action and/or public Port operation

expected behavior
  what the published contract says should happen

observed behavior
  structured result, conformance failure, target output, or Surface failure

reproduction
  smallest deterministic fixture or real-target sequence that demonstrates it

provenance
  branch/revision, platform/target, Connector/Surface identity, and relevant configuration
```

If the problem cannot yet be reproduced, gather evidence until it can be made concrete. Do not select an architecture fix merely from a symptom description.

### 2. Classify the owner before proposing a change

Choose exactly the owning layer for the primary defect. Record secondary consequences separately.

The classification field is:

```text
core Action
    the semantic operation or provider-neutral Action behavior is wrong

Port contract
    the public capability contract is missing, contradictory, underspecified, or semantically wrong for every conforming implementation

SDK support
    the Port contract is sound, but the public SDK lacks the machinery needed for an external implementation to satisfy or prove it cleanly

Connector implementation
    the public contract and SDK are adequate; this implementation translates/probes/invokes/verifies its target incorrectly

Surface implementation
    the canonical Action is sound, but a CLI/launcher/native/UI Surface discovers, gathers input, confirms, invokes, or projects results incorrectly

target limitation
    the target technology genuinely cannot supply the requested contract behavior in the current environment

local configuration
    code and target capability are sound; local setup, permissions, credentials, dependency configuration, or operator-provided values are wrong/missing
```

Do not classify by which repository file is easiest to edit. Classify by who owns the promised behavior.

### 3. Use discriminating questions

Before changing code, answer:

- Does the same canonical Action fail with a different conforming Connector?
- Does the failing Connector violate a published Port guarantee?
- Can a second implementation satisfy the current Port without private knowledge?
- Does the reusable conformance suite expose the defect?
- Is the failure only in how a Surface invokes or renders the canonical Action?
- Does the target expose the necessary primitive at all?
- Does the failure disappear when local configuration is corrected without code changes?

These questions distinguish general architecture problems from provider-local faults.

### 4. Put the regression at the owning layer

Before or with the fix, add/update a regression that fails for the reproduced defect at the layer that owns it.

Examples:

```text
core Action
    canonical Action acceptance test using provider-neutral test Connector(s)

Port contract
    shared conformance test or contract fixture that every implementation must satisfy

SDK support
    SDK-level test proving the public implementation path

Connector implementation
    Connector-specific regression plus the relevant shared conformance suite

Surface implementation
    Surface contract/acceptance test proving descriptor-driven Action identity and structured-result equivalence

target limitation
    explicit capability-probe / unsupported-target test and useful typed diagnostic

local configuration
    configuration validation/probe test where code can validate it; otherwise record operator evidence without changing semantics
```

A test that merely snapshots the accidental provider output is not sufficient when the public semantic promise belongs elsewhere.

### 5. Fix only the owning layer unless the failure proves a general gap

Use the classification to constrain the patch.

If the defect is a **Connector implementation** problem, fix the Connector. Do not add:

```text
if macos { ... }
if ubuntu { ... }
if homebrew { ... }
if this_personal_connector { ... }
```

to a provider-neutral core Action merely to make one extension pass.

If real extension work proves a **Port contract** or **SDK support** gap, fix the public contract/SDK and add public regression/conformance evidence before considering the provider fixed. Then adapt affected implementations through the same public seam.

If the issue is a **Surface implementation** problem, preserve canonical Action identity and result semantics; repair only discovery/input/confirmation/invocation/projection behavior.

If it is a **target limitation** or **local configuration** problem, do not falsify capability. Return explicit ineligibility/typed failure and document the real requirement.

### 6. Run the relevant conformance suite after the fix

Every Connector hardening pass that touches or depends on a public Port must rerun the shared conformance suite for that Port.

Target-specific regression tests supplement conformance; they do not replace it.

Where the failure is target-specific, also run the strongest safe real-target or target-faithful acceptance available. Keep named physical-machine acceptance distinct when the ticket explicitly requires that deployment.

### 7. Invoke the canonical Action when applicable

After a Connector fix, prove the normal product path when a canonical Action consumes the Port:

```text
Action registry
→ Port resolution
→ selected Connector
→ operation
→ structured ActionResult
```

Do not accept “the Connector unit test passes” as the whole proof when the issue concerns product behavior through an Action.

### 8. Check for architectural leakage

Before completion, inspect the diff and dependency graph for a workaround at the wrong layer.

Explicitly check:

- no provider/tool/platform condition was added to a provider-neutral core Action without a genuine semantic reason;
- core does not acquire a dependency on an optional personal Connector/Surface;
- authored Control intent was not changed merely to fit provider behavior;
- structured Port/Action results remain provider-neutral at public boundaries;
- personal extension registration still uses the normal public SDK path;
- shared conformance remains reusable by another implementation.

### 9. Record the hardening packet

A complete hardening result includes:

```text
Failure
  minimal reproduction and observed evidence

Classification
  owning layer
  discriminating evidence

Regression
  test/conformance change and why that layer owns it

Fix
  files/layer changed
  public-contract change, if any

Verification
  relevant regression
  shared conformance
  target-specific acceptance
  canonical Action proof, where applicable

Leakage check
  evidence no personal-stack exception entered core

Remaining external evidence
  named physical/manual acceptance only where still genuinely required
```

## Controlled reference proof

The repository proof for this Skill uses a deliberately broken reference `WorkDiscovery` Connector:

```text
canonical work.list
→ public WorkDiscovery
→ eligible fixture Connector
→ provider failure
```

The defect is classified as **Connector implementation** because the Action, Port contract, SDK, and reference conformance path are already sufficient. The regression demonstrates that shared `WorkDiscovery` conformance rejects the failing implementation at `typed-operation`; a corrected implementation then passes the same public conformance and the unchanged canonical `work.list` Action succeeds.

The correct fix is therefore in the Connector implementation. No `work.list` special case is permitted.

## Decision rules

### Failure first, taxonomy second

Do not manufacture a failure to justify an architecture change. Start from reproducible evidence.

### Classification precedes patch selection

A patch that works is still architecturally wrong when it changes a layer that does not own the promise.

### Public gaps become public tests

If one personal extension reveals a general Port/SDK problem, another implementation must benefit from the fix. Capture it in reusable contract/conformance evidence.

### Provider faults stay provider-local

A target command, output parser, dependency probe, target permission, or provider-specific verification bug belongs in its Connector unless it exposes a genuine general contract defect.

### Surfaces do not own semantic Actions

A Surface discovers and invokes Actions. It must not fork a second implementation of the domain operation.

### Target truth outranks optimistic capability

If a technology cannot do the requested operation, report the limitation explicitly. Do not claim conformance by silently doing less.

### Local setup is not core semantics

A missing executable, credential, permission, or operator setting does not justify changing the canonical Action meaning.

## Completion checklist

A complete hardening pass can answer:

- What exact failure or friction was reproduced?
- Which of core Action, Port contract, SDK support, Connector implementation, Surface implementation, target limitation, or local configuration owns it?
- What evidence distinguishes that classification from the neighbouring alternatives?
- What regression was added at the owning layer?
- Did the relevant shared conformance suite run after the change?
- Did target-specific acceptance run where appropriate?
- Did a canonical Action prove the corrected Connector path where applicable?
- Did the diff avoid a personal-stack/platform/provider exception in core logic?
- If a general public gap was found, did the public contract/test/documentation improve before the provider was accepted?
- Is any remaining evidence genuinely external/manual rather than an implementation gap?

If not, the failure has not been fully hardened.
