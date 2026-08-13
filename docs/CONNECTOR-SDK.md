# Central Connector SDK

**Status:** development reference for the public extension contract

The public SDK is exported from `ctrl/sdk/index.js` and from the package subpath `@epilogos/central/sdk`. A Connector consumes this SDK. It does not import `ctrl/core/` modules.

```text
Action
  ↓
public Port contract
  ↓
public Connector metadata + implementation
  ↓
target technology
```

## First published Port

`WorkDiscovery` contract version `1.0.0` defines the `list` operation. The public type surface includes `WorkDiscoveryListInput`, `WorkDiscoveryListOutput`, and `WorkItem`. Runtime validators enforce the same boundary for JavaScript implementations.

## Connector definition

Use `defineConnector()` with a public manifest and implementation object. Manifests declare the SDK API version, Connector identity and version, compatible Port contracts, supported platforms, runtime and dependency requirements, configuration requirements, and mutation scope.

A minimal starting implementation is available at `connectors/template/work-discovery.js`. The reference filesystem and static Connectors use the same public imports.

## Conformance

`runWorkDiscoveryConformance()` is the reusable conformance suite for the first Port. It checks public manifest validity, compatible Port identity and contract version, safe capability-probe shape, typed operation output, and stable repeat behavior for this read-only Port.

Connector-specific tests can add real target behavior. Shared Port behavior belongs in the conformance suite.

Run all current checks with `npm test` from the repository root. Both reference Connectors and the template are exercised through the public SDK and shared conformance path.
