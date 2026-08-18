# Central

Central is a human-owned operating root for an agentic computing life.

Central gives a person one durable place for authored control material, executable control actions, extension code, agent procedures, and ordinary work.

The product uses a simple dependency rule:

> Control says what should persist. `ctrl` says what can be done. Connectors say how it can be done here.

Central does not require a specific operating system, launcher, package manager, configuration manager, editor, agent harness, or automation product. The core defines stable Actions and extension contracts. Connectors bind those contracts to the technology that exists on a given machine.

## Implementation language

Central is implemented in **Rust**.

Executable product code, the public SDK, Connectors, and executable product/conformance test harnesses must use Rust. Markdown, JSON, YAML, shell, and platform metadata can be used where they are the natural representation for documentation, authored source, fixtures, configuration, packaging, or integration, but they must not become an alternative implementation of Central semantics.

This is a repository-level implementation constraint. New product behavior must extend the Rust implementation rather than introduce a second application language.

## Repository shape

```text
~/Central/
├── Control/
│   ├── user/
│   ├── agents/
│   └── machines/
├── ctrl/
│   ├── core/
│   ├── sdk/
│   └── tests/
├── connectors/
│   ├── reference/
│   └── personal/
├── skills/
│   ├── control-maintenance/
│   ├── machine-declaration/
│   ├── connector-authoring/
│   └── connector-hardening/
├── docs/
├── .central/
└── Work/
```

`Central` is the repository root. `Work/` and `.central/` are local runtime areas and are not part of the portable repository history.

```text
Control/      durable human-owned source
ctrl/         executable core and public SDK
connectors/   implementations of public Port contracts
skills/       reusable agent procedures
.central/     derived local state
Work/         ordinary work
```

The separation is intentional. Control content is not Skill procedure. Skills are not Connectors. Connectors do not define core Actions.

## Product principles

1. **Human authorship is explicit.** Observation and inference do not silently become authored Control material.
2. **Control stays high-signal.** Persistent information must have durable value across the contexts where it applies.
3. **Availability does not imply disclosure.** Systems can index or discover information without loading all of it into every agent context.
4. **Process and context stay distinct.** Control can express preferences about methods and capabilities. Skills contain reusable procedure.
5. **Actions have stable identity.** A shell command, launcher entry, shortcut, agent call, and future UI can invoke the same Action.
6. **Core code depends on abstractions.** A Connector binds an abstract Port to a specific tool, platform, or service.
7. **Native platform functions form the base.** Optional tools extend the user experience. They do not redefine the product.
8. **Derived state stays subordinate.** Cache, indexes, generated projections, and observations do not outrank authored source.
9. **Extensions are open-ended.** The SDK and conformance tests must let a developer or agent add support for a new environment without changing core Action logic.
10. **The real installation tests the architecture.** The personal extension set must use the same public SDK and Connector contracts that other users use.

## Source installation

The base `ctrl` command has one native source-install contract:

```sh
cargo install --path ctrl
```

After installation, `ctrl --version` exposes the package version. A clean root can be initialized and inspected with:

```sh
ctrl --root /path/to/Central init
ctrl --root /path/to/Central doctor --json
ctrl --root /path/to/Central action list --json
```

Initialization creates only `Control/user`, `Control/agents`, `Control/machines`, `.central`, and `Work`. The Control roots start empty. See [`docs/INSTALL.md`](docs/INSTALL.md) for the complete interoperability proof and isolated-prefix form.

## Documentation

Read the package in this order:

1. [`docs/CENTRAL-VISION.md`](docs/CENTRAL-VISION.md) — product purpose, experience, and system boundaries.
2. [`docs/CENTRAL-SYSTEM-SPEC.md`](docs/CENTRAL-SYSTEM-SPEC.md) — normative product and architecture specification.
3. [`docs/CONTROL-CONTENT-PROTOCOL.md`](docs/CONTROL-CONTENT-PROTOCOL.md) — durable information, authorship, disclosure, and Skill boundaries.
4. [`docs/PERSONAL-WORLD-PROJECTION.md`](docs/PERSONAL-WORLD-PROJECTION.md) — how selected Central material becomes the person's O:I world/profile without creating a shadow profile or mutating Control.
5. [`docs/CONNECTOR-SDK-SPEC.md`](docs/CONNECTOR-SDK-SPEC.md) — Action, Port, Connector, Surface, SDK, and conformance architecture.
6. [`docs/PERSONAL-EXTENSION-SPEC.md`](docs/PERSONAL-EXTENSION-SPEC.md) — first real extension set used to prove and harden the open architecture.
7. [`docs/INSTALL.md`](docs/INSTALL.md) — native `ctrl` source installation and clean-root verification.

The issue tracker contains the development map and implementation tickets.
