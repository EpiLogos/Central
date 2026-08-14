# Rust Connector SDK developer surface

`central-connector-sdk` is the public Rust extension boundary for Central. Connector packages should depend on that crate directly. They must not import private `central_ctrl` implementation modules in order to satisfy a Port.

The current published proof Port is `WorkDiscovery` v1. Its public contract includes `WORK_DISCOVERY_PORT`, `WorkDiscoveryInput`, `WorkDiscoveryOutput`, `WorkItem`, the `WorkDiscovery` trait, shared `PortError` / `PortErrorCode`, and operation metadata describing mutation and repeat behavior.

A Connector implements the public `Connector` trait and supplies a complete `ConnectorManifest`. `validate_connector_manifest` checks the SDK compatibility version, required metadata, mutation scope, platform declaration, and Port declarations before `ConnectorRegistry::register` accepts the extension. Resolution remains deterministic and returns eligible, ineligible, and selected-Connector diagnostics.

## Conformance

Conformance is part of the Port contract. `run_work_discovery_conformance` is reusable by any Connector package. It checks manifest validity, Port compatibility, the capability probe, typed operation behavior, duplicate/empty item invariants, and repeat stability; an optional expected-name fixture can assert target-independent expected behavior.

Run all published contract and consumer tests with the repository verification operation:

```text
cargo test --workspace
```

The two packages under `connectors/reference` run the same conformance function against different implementations. `connectors/template` is a third compiled SDK consumer and is the starting scaffold for a new Connector.

Real-target behavior still requires target-specific tests in the Connector package. A passing fixture does not prove a provider limitation away.

## Authoring sequence

Read the Port contract first. Inspect the target system. Implement the manifest and safe probe. Implement only the typed Port operation. Run shared conformance continuously, then add target-specific tests. Register the Connector through `ConnectorRegistry`; do not add provider-specific conditions to the Action.

The reference Connectors are examples of the contract, not the definition of it.
