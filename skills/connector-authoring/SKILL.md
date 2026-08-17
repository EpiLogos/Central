---
name: connector-authoring
description: Author and prove a Central Connector from a published Port contract using only the public Rust SDK.
---

# Connector authoring

A Connector binds a published Central Port to one real technology. The dependency direction is constitutional:

```text
Action → Port → public SDK → Connector → target technology
```

Treat the **published Port contract as authoritative**. Existing Connectors are examples and evidence; they do not redefine the Port. A preferred operating system, local tool, or personal stack does not receive a private path into core.

For the current Rust implementation, read these before changing code:

1. `docs/CENTRAL-SYSTEM-SPEC.md` for the Action/Port/Connector boundary and the authored-versus-observed state rules.
2. `docs/CONNECTOR-SDK-SPEC.md` for the normative SDK contract.
3. The relevant public definitions exported by `central-connector-sdk`, especially the `PortContract`, operation types, Port trait, shared error types, Connector manifest contract, registry, diagnostics, and conformance fixture/function for the Port.
4. `connectors/template` only as a scaffold/example after the public contract is understood.

Do not begin from a provider implementation and infer Central semantics backwards from it.

## Procedure

### 1. Select and read the Port contract

Record the exact public contract identity before implementation:

- Port identifier and version;
- purpose and semantic boundary;
- operations;
- typed input and output structures;
- mutation class;
- whether preview is required and what preview guarantees;
- repeat/idempotence requirements;
- shared error surface;
- capability-probe expectations;
- reusable conformance function and fixture.

If the required capability does not fit an existing Port without changing its semantic meaning, classify that as a **Port-contract question**. Do not silently widen an existing Port inside one Connector.

### 2. Inspect the target as a target

Read the target technology's authoritative documentation and, when possible, inspect the real environment. Establish:

- supported platforms/environments;
- required binaries, APIs, services, permissions, or runtime facilities;
- configuration prerequisites;
- safe positive capability probes;
- explicit negative/off-platform conditions;
- provider-specific failure modes;
- which operations are observable and which actually mutate state.

A capability probe must be safe and meaningful. The mere existence of a Rust module, package name, or manifest entry is not proof that the target capability is usable.

### 3. Depend only on the public SDK

The Connector package should depend directly on `central-connector-sdk` for shared contracts. Import the public Port trait and typed request/result/error structures from that crate.

Do **not**:

- import private `ctrl` implementation modules;
- call an Action implementation directly;
- reach into another Connector's private implementation as the contract;
- add the provider name to core Action branching;
- parse provider terminal prose in core;
- create a second registration, selection, or error path just for this extension.

If the public SDK is genuinely insufficient, fix the public seam and add regression/conformance evidence there before continuing the Connector.

### 4. Declare the manifest truthfully

The Connector manifest must declare at least:

```text
id
version
display name
implemented Port ids + compatible versions
supported platforms/environments
entrypoint
runtime requirements
dependency probes
configuration requirements
mutation scope
```

For a Connector implementing several Ports, declare every Port/version it actually supports and expose the corresponding public Connector accessor for each implementation.

The manifest's mutation scope must describe the strongest mutation the package can perform. A read-only inspection Connector must not be marked mutating merely because the target technology can mutate; a package that performs local mutation must not claim to be read-only.

Never declare a Port merely to make resolution succeed. Declared support means the implementation is intended to pass that Port's shared conformance suite.

### 5. Implement safe eligibility first

Implement `Connector::probe` before relying on the Connector in an Action path.

The probe should distinguish at least:

- supported environment and dependencies available → eligible candidate;
- unsupported platform/environment → explicit ineligibility;
- missing dependency → explicit ineligibility;
- invalid/missing required configuration → explicit ineligibility where detectable safely.

Do not mutate the host while probing.

### 6. Implement typed Port operations

Implement the public Port trait exactly. Keep provider conversion at the Connector boundary:

```text
Central typed request
    ↓
Connector
    ↓
provider API / command / native facility
    ↓
Connector parses and classifies
    ↓
Central typed result or PortError
```

If the target only supplies text, the Connector owns parsing that text. Core Actions must receive structured values and shared `PortError` classifications, with provider detail attached only as diagnostic detail.

For mutating Ports:

- preserve the Port's preview semantics;
- preview must not perform the requested mutation;
- apply must report whether it actually changed state;
- obey repeat/idempotence rules;
- make verification observable through the appropriate inspection Port rather than trusting provider success prose.

### 7. Run shared conformance continuously

Shared conformance is part of the Port contract, not optional test decoration.

Run the public conformance function for every implemented Port using a fixture that cannot damage a developer's real environment. For mutating Ports, use an in-memory, temporary, throwaway, or otherwise isolated target unless the conformance contract explicitly requires a real target.

Conformance should prove, where applicable:

- manifest validity;
- compatible Port identity/version;
- capability probe;
- typed operation behavior;
- required failure behavior;
- preview non-mutation;
- apply behavior;
- post-apply state;
- repeat/idempotence;
- useful diagnostics.

Add target-specific tests for behavior the shared fixture cannot prove. Do not weaken the shared suite to accommodate one provider.

### 8. Register through the normal registry and inspect diagnostics

Register the Connector through `ConnectorRegistry`. Resolve the public Port in an explicit `ConnectorContext` and inspect:

- eligible Connectors;
- ineligible Connectors and reasons;
- selected Connector;
- deterministic behavior when more than one Connector implements the Port.

Do not bypass eligibility or inject a Connector directly into core Action logic because it is the preferred local provider.

### 9. Invoke a canonical Action through the Connector

Prove the full route using a canonical Action whose required Port is implemented by the Connector:

```text
canonical Action
    ↓
Port resolution
    ↓
Connector selection
    ↓
real/reference target
    ↓
typed ActionResult
```

The proof must show that the Action remains provider-neutral. If the extension requires changing the Action to mention the provider, stop and classify the architecture failure before continuing.

### 10. Prove real-target behavior and the negative boundary

Shared fixtures prove the contract; they do not prove every property of a native integration.

When practical, finish with harmless real-target acceptance covering the provider behavior that cannot be simulated. For a platform-specific Connector, include both:

- a positive test on the real supported platform/target;
- an explicit off-platform or unavailable-capability case proving that the Connector rejects unsupported environments rather than silently degrading or pretending success.

For externally visible operations such as native open/reveal, automation, or host metadata, exercise the actual target facility rather than only a mock command builder.

### 11. Prove removability and core independence for optional extensions

An optional platform extension must not become a hidden dependency of core.

Where the Connector lives outside the core workspace dependency path, add an isolation gate appropriate to the repository. The gate should demonstrate that:

- core builds/tests without the optional extension;
- unrelated Actions remain available;
- core has no dependency edge on the provider package;
- the provider package can be removed without requiring provider-specific cleanup in core Action code.

This is especially important for host-specific extension sets.

### 12. Feed general lessons back into the public procedure

Real extension work is an SDK hardening exercise. Before fixing a failure, classify it:

```text
Action behavior
Port contract
SDK support
Connector implementation
Target limitation
Connector configuration
```

A general problem belongs in the public Port/SDK/procedure with regression or conformance evidence. A target-specific problem belongs in the Connector.

When a real extension exposes a reusable authoring lesson—multi-Port declaration, mutation/preview semantics, removability, off-platform rejection, verification, diagnostics—improve this Skill rather than leaving that knowledge trapped in one Connector's implementation history.

## Completion evidence

Do not call a Connector complete without recording:

- Port id/version for every implemented Port;
- target documentation or real observations used;
- Connector manifest;
- public SDK imports/dependencies;
- positive and negative capability-probe evidence;
- shared conformance result for every Port;
- target-specific tests where shared conformance is insufficient;
- registry eligibility/selection diagnostics;
- one canonical Action result through the Connector;
- real-target acceptance when target-native behavior matters;
- optional-extension removability/core-independence evidence when applicable;
- classification and regression/conformance evidence for every general contract change made while authoring it.

## Rust reference proof

The repository contains two complementary WorkDiscovery references:

1. `connectors/template` shows the smallest public Rust scaffold. `TemplateWorkConnector` imports `WorkDiscovery`, `WORK_DISCOVERY_PORT`, manifest types, `PortError`, and conformance support from `central-connector-sdk`; it compiles and passes `run_work_discovery_conformance` without private `ctrl` implementation knowledge.
2. `connectors/reference::FilesystemWorkConnector` binds the same published `WorkDiscovery` Port to an ordinary filesystem target. The shared `run_work_discovery_conformance` suite proves the Port behavior; `ConnectorRegistry` performs normal eligibility/selection; canonical `work.list` completes the `Action → Port → Connector → target` route.

`ctrl/tests/connector_authoring_skill.rs` is the executable repository proof of this procedure. It checks the Skill's required architectural constraints and runs the public WorkDiscovery reference through shared conformance, registry resolution, and canonical Action invocation.

## macOS hardening lessons carried forward

The macOS extension set developed for #14 established reusable requirements now made explicit above: a platform Connector may implement several public Ports; each Port must remain public and independently conformant; platform eligibility must reject off-platform use explicitly; native behavior must be exercised on the real target; core must remain free of macOS branches; and an optional platform crate must be removable from the core dependency graph. Those are authoring rules for future extensions, not macOS-specific exceptions.
