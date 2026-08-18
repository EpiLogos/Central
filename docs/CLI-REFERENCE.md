# Central CLI reference

The `ctrl` binary is a Surface over Central's canonical Action registry. Command aliases are conveniences; the Action ID is the stable product identity.

## Invocation

```text
ctrl [--json] [--root PATH] <command>
ctrl [--json] [--root PATH] action run <action-id> [<json-object>]
```

Root resolution is, in order:

1. `--root PATH` or `--root=PATH`;
2. `CENTRAL_ROOT`;
3. `$HOME/Central` (or the platform home equivalent).

`--json` returns the structured `ActionResult` envelope. Without it, the CLI renders a human-readable projection of the same result.

`action run` is the complete invocation seam. It accepts any registered canonical Action ID and an optional JSON object. Omitting the object is equivalent to `{}`. Arrays, scalars, malformed JSON, extra arguments, and unknown Action IDs are rejected as structured invalid input rather than interpreted as a second API.

## Canonical Actions

The current core registry exposes:

| Action | Purpose | Common CLI projection |
|---|---|---|
| `action.list` | discover Action descriptors | `actions`, `action list` |
| `central.root` | resolve the active Central root | `root` |
| `central.init` | initialise the required Central root shape | `init` |
| `central.doctor` | inspect Central structural health and diagnose a root that is also the product source checkout | `doctor` |
| `central.recovery.plan` | explain recovery for an authored machine role | `recovery plan <role>` |
| `central.recover` | reconcile supported recovery for an authored machine role | `recover <role>` |
| `control.open` | resolve one authored Control source root | `control open <user|agents|machines>` |
| `control.search` | search readable authored Control source | `control search <query>` |
| `machine.declaration` | read authored machine-role intent | `machine declaration <role>` |
| `machine.inspect` | inspect current observed machine state | `machine inspect` |
| `machine.account` | compose the current-machine account (identity, observed state, authored roles, drift) | `machine account` |
| `machine.plan` | compare authored intent with observed state | `machine plan <role>` |
| `machine.apply` | apply the planned portable reconciliation | `machine apply <role>` |
| `machine.verify` | verify authored intent against fresh observation | `machine verify <role>` |
| `work.list` | discover ordinary Work directories | `work list` |
| `work.search` | search discovered Work by name | `work search <query>` |
| `work.open` | open a Work item through `NativeOpen` | `work open <query>`, `open <query>` |
| `work.reveal` | reveal a Work item through `NativeReveal` | `work reveal <query>` |

Every row is also invokable through `action run`. Examples:

```text
ctrl --json action run action.list
ctrl --json action run work.search '{"query":"Central"}'
ctrl --json action run machine.plan '{"role":"home-server"}'
ctrl --json action run central.recover '{"role":"primary-workstation"}'
```

`action.list` is the machine-readable discovery surface. Its descriptors include input definitions, mutation class, preview support, required Ports, and availability metadata. Other Surfaces should consume those descriptors rather than maintain a second Action catalog.

## Guided use

```text
ctrl pick
```

The guided picker reads the same Action descriptors and input-selection metadata. It is a projection of the registry, not a separate command model.

## Result and exit contract

All execution paths produce an `ActionResult`. With `--json`, the result is emitted directly as JSON.

| Result status | Exit code |
|---|---:|
| `success` | 0 |
| `cancelled` | 0 |
| `invalid_input` | 2 |
| `invalid_central_structure` | 3 |
| `unavailable_capability` | 4 |
| `connector_failure` | 5 |
| `partial_completion` | 6 |
| `verification_failure` | 7 |
| `internal_failure` | 1 |

Provider failures remain typed Connector/Port failures inside the result detail. The CLI does not translate Homebrew, chezmoi, Git, Ubuntu, macOS, or other provider semantics into core Action behavior.

## Personal host Surfaces

Optional host/launcher Surfaces may add Actions or Connector composition while preserving this protocol. In particular, the macOS host line uses descriptor-driven `action list` and `action run` for Raycast/Shortcuts integration. Those personal extensions remain outside stock `ctrl`; their provider-specific behavior is not required by core Actions.
