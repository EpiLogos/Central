# Rust Connector template

This crate is the smallest compiled example of the public Central Connector contract.

To author a `WorkDiscovery` Connector:

1. depend on `central-connector-sdk`, not `central_ctrl`;
2. construct a complete `ConnectorManifest` and declare the exact `WORK_DISCOVERY_PORT` compatibility identity;
3. implement the typed `WorkDiscovery` trait;
4. implement the public `Connector` trait and a safe capability `probe`;
5. run `run_work_discovery_conformance` with a representative fixture;
6. add target-specific tests for behavior the shared fixture cannot prove.

Replace the template identity and the empty `list` implementation. Do not copy provider behavior into a core Action.
