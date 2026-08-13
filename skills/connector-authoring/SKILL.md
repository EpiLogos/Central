---
name: connector-authoring
description: Procedure for implementing a published Central Port as a Connector.
---

# Connector authoring

Treat the published Port contract as authoritative. Preserve `Action → Port → Connector → target` and use only the public SDK. Existing Connectors are examples, not definitions of the Port.

## Procedure

1. Read the current Port contract first: identifier, version, purpose, operations, typed inputs/outputs, mutation class, preview and repeat rules, failures, and conformance checks.
2. Read authoritative documentation for the target technology and establish supported environments, dependencies, configuration requirements, limitations, and capability-probe conditions.
3. Use public SDK exports and the Connector template. Do not import private `ctrl/core/*` modules.
4. Declare the Connector manifest, implement its safe capability probe, then implement typed Port operations.
5. Run shared conformance tests throughout the work and add target-specific tests where shared fixtures are insufficient.
6. Register through the normal registry, inspect eligibility/selection diagnostics, and invoke a canonical Action through the Connector.
7. Finish with a harmless real-target proof when practical.

## Failure discipline

Classify a failure before selecting a fix: Action behavior, Port contract, SDK support, Connector implementation, target limitation, or Connector configuration. A general contract problem belongs in the public Port/SDK with a regression or conformance test. A target-specific problem belongs in the Connector.

Do not make one Connector pass by adding its provider name to core Action logic, parsing provider prose in core, bypassing normal registration, or creating a private integration path.

## Completion evidence

Record the Port contract/version, target documentation or observations used, Connector manifest, public SDK imports, capability-probe result, shared conformance result, target-specific tests where required, resolver diagnostics, one canonical Action result through the Connector, and the classification/test evidence for any general contract change.

## Reference proof

The repository's `WorkDiscovery` slice is the executable reference. `createWorkDiscoveryConnectorTemplate` implements the published contract from public SDK material; `runWorkDiscoveryConformance` proves shared behavior; `ConnectorRegistry` proves normal eligibility and selection; canonical `work.list` proves the complete Action → Port → Connector path. The SDK test suite executes this route.
