# Rust Connector SDK developer surface

`central-connector-sdk` is Central's public Rust extension boundary. Connector packages depend on that crate directly. They must not import private `ctrl` implementation modules in order to satisfy a Port.

The contract is deliberately small:

```text
canonical Action
    ↓
public Port
    ↓
ConnectorRegistry selection
    ↓
Connector implementation
    ↓
real tool / platform
```

Core Actions know typed Port semantics and structured results. They do not know which personal provider will satisfy the Port.

## Published Ports

The current Rust integration surface publishes these `1.0.0` Port contracts:

| Port | Operations | Mutation contract |
|---|---|---|
| `WorkDiscovery` | `list` | read-only, repeat-stable |
| `NativeOpen` | `open` | externally mutating |
| `NativeReveal` | `reveal` | externally mutating |
| `TagStore` | `read`, `replace` | read + idempotent local replacement |
| `MachineInspector` | `inspect` | read-only observation |
| `PackageManager` | `preview`, `apply` | preview before idempotent local reconciliation |
| `ConfigurationManager` | `preview`, `apply` | preview before idempotent local reconciliation |
| `ServiceManager` | `preview`, `apply` | preview before idempotent local reconciliation |
| `Synchronizer` | `preview`, `apply` | preview before idempotent external synchronization |

The public constants are respectively `WORK_DISCOVERY_PORT`, `NATIVE_OPEN_PORT`, `NATIVE_REVEAL_PORT`, `TAG_STORE_PORT`, `MACHINE_INSPECTOR_PORT`, `PACKAGE_MANAGER_PORT`, `CONFIGURATION_MANAGER_PORT`, `SERVICE_MANAGER_PORT`, and `SYNCHRONIZER_PORT`.

The optional macOS Automation/launcher feature line separately proves an `Automation 1.0.0` Port for Shortcuts. It remains a personal feature branch until its named workstation acceptance gate is complete; stock `ctrl` does not depend on it.

## Connector contract

A Connector implements the public `Connector` trait and supplies a complete `ConnectorManifest`. `validate_connector_manifest` checks:

- `central.connector/v1` API compatibility;
- non-empty identity/version/display/entrypoint fields;
- a valid mutation scope;
- at least one supported platform/environment;
- at least one typed Port declaration;
- duplicate and malformed Port declarations.

`ConnectorRegistry::register` accepts implementations through this public contract. Resolution considers compatible Port/version declarations, platform eligibility, and the Connector's read-only capability probe, then returns eligible, ineligible, and selected-Connector diagnostics. Selection is deterministic; core Actions do not branch on provider IDs.

A Connector may implement one or several Ports. It exposes each implementation only through the corresponding optional trait accessor such as `machine_inspector()`, `configuration_manager()`, or `synchronizer()`.

## Typed errors

Provider behavior is translated at the Connector boundary into `PortError` / `PortErrorCode`:

```text
UnsupportedEnvironment
MissingDependency
InvalidConfiguration
CapabilityUnavailable
InvalidInput
ProviderOperationFailed
PermissionFailure
VerificationFailure
UnexpectedConnectorFailure
```

Provider-specific stdout/stderr or operating-system detail belongs in `provider_detail`, not in core branching logic.

## Conformance

Conformance is part of the public Port contract. The SDK exports reusable suites for the published behaviors, including:

- `run_work_discovery_conformance`;
- `run_native_open_conformance`;
- `run_native_reveal_conformance`;
- `run_tag_store_conformance`;
- `run_machine_inspector_conformance`;
- `run_package_manager_conformance`;
- `run_configuration_manager_conformance`;
- `run_service_manager_conformance`;
- `run_synchronizer_conformance`.

The machine inspector also has scoped conformance for authored-resource observation.

Mutating conformance checks must exercise a genuinely changeable fixture when the suite claims to prove mutation. `Synchronizer` specifically requires the first preview to be changeable, first apply to report `changed=true`, the post-apply preview to be satisfied, and repeated apply to be stable. This rule was hardened by the fresh-session #18 Git Connector proof after the earlier suite was found capable of false-positive mutation acceptance.

Run the current integration contract and consumer tests with:

```text
cargo test --workspace
```

Target-specific Connector packages must also run their own real-provider acceptance. A passing shared fixture proves the contract shape; it does not prove a target limitation away.

## Authoring sequence

Use `skills/connector-authoring/SKILL.md` as the procedural front door:

1. choose one published Port and read its typed contract;
2. inspect the target system independently;
3. implement the manifest and safe capability probe;
4. implement only the public typed operation(s);
5. run shared conformance continuously;
6. add target-specific tests against the real provider where remotely safe;
7. register through `ConnectorRegistry`;
8. exercise an unchanged canonical Action through the resulting Connector.

When a real extension exposes friction, use `skills/connector-hardening/SKILL.md`: reproduce, classify the owning layer first, add a regression at that layer, then fix the general contract or provider behavior without adding a personal-stack exception to core.

`connectors/reference` are examples of the contract, not its definition. `connectors/template` is the compiled scaffold for a new Connector. `connectors/git-sync` is the fresh-session external-grade proof that a new provider can be built from the published `Synchronizer` seam and used by canonical recovery without a private integration path.
