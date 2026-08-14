# Rust Connector SDK

The normative architecture remains `docs/CONNECTOR-SDK-SPEC.md`.

The implemented public Rust import path is `central_ctrl::sdk`. It publishes the Port contracts, structured types, Connector metadata, and shared conformance used by external extensions and by the repository reference Connectors.

`WorkDiscovery/v1` implementations provide `probe` and `list`, declare the exact Port contract in `ConnectorMetadata`, and run the reusable `sdk::conformance::work_discovery` check against a representative fixture.

See `examples/work_discovery_connector.rs` for the minimal compiled example.

Repository verification is `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test --all-targets`.
