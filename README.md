# Central

Central is a human-owned operating root for an agentic computing life.

Central gives a person one durable place for three things:

- **Control** — authored information and reproducible intent that should persist.
- **ctrl** — one executable action language over the Central environment.
- **Work** — ordinary local project directories and work material.

The product uses a simple dependency rule:

> Control says what should persist. `ctrl` says what can be done. Connectors say how it can be done here.

Central does not require a specific operating system, launcher, package manager, configuration manager, editor, agent harness, or automation product. The core defines stable actions and extension contracts. Connectors bind those contracts to the technology that exists on a given machine.

## Repository shape

```text
~/Central/
├── Control/
│   ├── user/
│   ├── agents/
│   └── machines/
├── ctrl/
├── .central/
└── Work/
```

`Central` is the repository root. `Work/` and `.central/` are local runtime areas and are not part of the portable repository history.

```text
Control/     durable human-owned source
ctrl/        executable product and extension SDK
.central/    derived local state
Work/        ordinary work
```

## Product principles

1. **Human authorship is explicit.** Observation and inference do not silently become authored Control material.
2. **Control stays high-signal.** Persistent information must have durable value across the contexts where it applies.
3. **Availability does not imply disclosure.** Systems can index or discover information without loading all of it into every agent context.
4. **Process and context stay distinct.** Control can express preferences about methods and capabilities. Skills contain reusable procedure.
5. **Actions have stable identity.** A shell command, launcher entry, shortcut, agent call, and future UI can invoke the same action.
6. **Core code depends on abstractions.** A connector binds an abstract port to a specific tool, platform, or service.
7. **Native platform functions form the base.** Optional tools extend the user experience. They do not redefine the product.
8. **Derived state stays rebuildable.** Cache, indexes, generated projections, and observations do not outrank authored source.
9. **Extensions are open-ended.** The SDK and conformance tests must let a developer or agent add support for a new environment without changing core action logic.
10. **The real installation tests the architecture.** The personal extension set must use the same public SDK and connector contracts that other users use.

## Documentation

Read the package in this order:

1. [`docs/CENTRAL-VISION.md`](docs/CENTRAL-VISION.md) — product purpose, experience, and system boundaries.
2. [`docs/CENTRAL-SYSTEM-SPEC.md`](docs/CENTRAL-SYSTEM-SPEC.md) — normative product and architecture specification.
3. [`docs/CONTROL-CONTENT-PROTOCOL.md`](docs/CONTROL-CONTENT-PROTOCOL.md) — durable information, authorship, disclosure, and skill boundaries.
4. [`docs/CONNECTOR-SDK-SPEC.md`](docs/CONNECTOR-SDK-SPEC.md) — Action, Port, Connector, Surface, SDK, and conformance architecture.
5. [`docs/PERSONAL-EXTENSION-SPEC.md`](docs/PERSONAL-EXTENSION-SPEC.md) — first real extension set used to prove and harden the open architecture.

The issue tracker contains the development map and implementation tickets.
