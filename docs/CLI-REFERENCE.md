# Central CLI reference

The `ctrl` binary is a Surface over Central's canonical Action registry. Command aliases are conveniences; the Action ID is the stable product identity.

`ctrl help`, `--help`, and `-h` print the stable command doorway: the top-level families and a pointer to capability discovery. They are not Actions and do not enumerate the Action field. `ctrl capabilities [--json]` is an exact alias of `ctrl actions` / `action.list`; it does not introduce a second catalogue.

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

The current composed registry exposes 45 Actions:

| Action | Purpose | Common CLI projection |
|---|---|---|
| `action.list` | discover Action descriptors | `capabilities`, `actions`, `action list` |
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
| `projectcentral.inspect` | inspect ProjectCentral without mutating Project source | `action run projectcentral.inspect` |
| `projectcentral.doctor` | verify ProjectCentral structure and bindings | `action run projectcentral.doctor` |
| `projectcentral.init` | initialise ProjectCentral for an existing Project | `action run projectcentral.init` |
| `projectcentral.adopt.preview` | preview Wiki adoption in place | `action run projectcentral.adopt.preview` |
| `projectcentral.adopt` | adopt a selected Wiki without source migration | `action run projectcentral.adopt` |
| `projectcentral.migrate.preview` | preview selected Wiki migration | `action run projectcentral.migrate.preview` |
| `projectcentral.migrate` | migrate a selected Wiki explicitly | `action run projectcentral.migrate` |
| `projectcentral.ground.inspect` | inspect authored Project ground and provenance standing | `action run projectcentral.ground.inspect` |
| `projectcentral.ground.plan` | propose reviewable source-ground relations | `action run projectcentral.ground.plan` |
| `projectcentral.ground.apply` | record an explicitly human-accepted source-ground relation | `action run projectcentral.ground.apply` |
| `projectcentral.change.horizon` | reconcile participating Project sources and read the deterministic Source Change Horizon | `action run projectcentral.change.horizon` |
| `projectcentral.change.reconcile` | reconcile authoritative Project source revisions | `action run projectcentral.change.reconcile` |
| `projectcentral.change.ack` | advance one named consumer cursor without changing source | `action run projectcentral.change.ack` |
| `projectcentral.now.inspect` | inspect the opt-in Project NOW field | `action run projectcentral.now.inspect` |
| `projectcentral.now.init` | initialise Project NOW / DAY state | `action run projectcentral.now.init` |
| `projectcentral.now.return` | write a bounded Agent return into the Project Wiki relation | `action run projectcentral.now.return` |
| `projectcentral.now.update` | update NOW lifecycle material | `action run projectcentral.now.update` |
| `projectcentral.now.promote` | promote selected NOW material with lineage | `action run projectcentral.now.promote` |
| `projectcentral.now.rollover` | close a DAY snapshot and roll NOW forward | `action run projectcentral.now.rollover` |
| `projectcentral.flow.list` | list stable Flow identities and current source/revision state | `action run projectcentral.flow.list` |
| `projectcentral.flow.read` | read current Flow source by FlowRef and reconcile external edits | `action run projectcentral.flow.read` |
| `projectcentral.flow.create` | create a blank ordinary-file Flow with stable identity | `action run projectcentral.flow.create` |
| `projectcentral.flow.adopt` | adopt an existing retained ordinary file as a Flow without moving it | `action run projectcentral.flow.adopt` |
| `projectcentral.flow.write` | perform a revision-safe human/Agent Flow write | `action run projectcentral.flow.write` |
| `projectcentral.flow.rename` | rename the retained source while preserving FlowRef | `action run projectcentral.flow.rename` |
| `projectcentral.flow.lifecycle` | set active/dormant/closed lifecycle without changing source revision | `action run projectcentral.flow.lifecycle` |
| `projectcentral.flow.history` | read exact Flow revision provenance/history | `action run projectcentral.flow.history` |

Every row is invokable through `action run`. Examples:

```text
ctrl --json action run action.list
ctrl --json action run work.search '{"query":"Central"}'
ctrl --json action run machine.plan '{"role":"home-server"}'
ctrl --json action run projectcentral.ground.inspect '{"project":"Central"}'
ctrl --json action run projectcentral.change.horizon '{"project":"Central"}'
ctrl --json action run projectcentral.now.inspect '{"project":"Central"}'
ctrl --json action run projectcentral.flow.create '{"project":"Central","actor":"human:local","actor_kind":"human","local_stamp":"2026-08-23-2310"}'
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
