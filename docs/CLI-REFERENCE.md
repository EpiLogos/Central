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

The current composed registry exposes 68 Actions:

| Action | Purpose | Common CLI projection |
|---|---|---|
| `central.files.list` | list actual Central directories without adoption | `action run central.files.list` |
| `central.files.read` | read a validated owner location as bounded UTF-8 text | `action run central.files.read` |
| `central.files.write` | atomically replace ordinary text under owner CAS | `action run central.files.write` |
| `central.files.history` | page native ordinary-file revision history | `action run central.files.history` |
| `central.files.recovery_preview` | preview exact historical bytes against current basis | `action run central.files.recovery_preview` |
| `central.files.restore` | restore an ordinary-file revision through the same CAS | `action run central.files.restore` |
| `action.list` | discover Action descriptors | `capabilities`, `actions`, `action list` |
| `central.root` | resolve the active Central root | `root` |
| `central.init` | initialise the required Central root shape | `init` |
| `central.doctor` | inspect Central structural health and diagnose a root that is also the product source checkout | `doctor` |
| `central.world` | index the full world centred in the active root: Control source areas, root and Project WikiSpaces with child refs, declared source-relations overrides, and per-Project ProjectCentral state. Read-only; every fault is reported as data | `world` |
| `central.world.project` | project the world onto one ProjectCentral: its fractal ground, Wiki, Flows, NOW folder, participating sources with provenance, and position under Work. Read-only | `world <project>` |
| `central.world.reproject.plan` | list the canonical ProjectCentral scaffolding that is missing, classify everything else by source provenance, and state what reprojection would never do. Read-only | `world plan <project>` |
| `central.world.reproject.apply` | stamp only the missing canonical ProjectCentral scaffolding; never moves, renames, deletes, relabels, or writes into anything that exists | `world apply <project>` |
| `central.recovery.plan` | explain recovery for an authored machine role | `recovery plan <role>` |
| `central.recover` | reconcile supported recovery for an authored machine role | `recover <role>` |
| `control.open` | resolve one authored Control source root | `control open <user|agents|machines>` |
| `control.search` | search readable authored Control source | `control search <query>` |
| `control.skills.inspect` | disclose the authored skill surface at every Control scope with scope, standing, provenance and retirement records; empty scopes are disclosed honestly as absent | `action run control.skills.inspect` |
| `control.skills.retire` | write standing retired into a skill's ground manifest with who/when/why provenance; the directory and body are never deleted | `action run control.skills.retire` |
| `control.skills.restore` | reverse a retirement: return the manifest to standing active and clear the retirement record | `action run control.skills.restore` |
| `machine.declaration` | read authored machine-role intent | `machine declaration <role>` |
| `machine.inspect` | inspect current observed machine state | `machine inspect` |
| `machine.account` | compose the current-machine account (identity, observed state, authored roles, drift) | `machine account` |
| `machine.adopt-current` | adopt the current machine into an authored role declaration under `Control/machines/<role>.json`, seeding observed capabilities and recording the Workcell reference as an opaque binding; idempotent, conflict-surfacing | `machine adopt-current [<role>]` |
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
| `projectcentral.source.read` | read one participating World source with its exact revision and provenance | `action run projectcentral.source.read` |
| `projectcentral.source.write` | revise one participating World source under compare-and-swap with an attributed change record | `action run projectcentral.source.write` |

Every row is invokable through `action run`. Examples:

```text
ctrl --json action run action.list
ctrl --json action run work.search '{"query":"Central"}'
ctrl --json action run machine.plan '{"role":"home-server"}'
ctrl --json action run machine.adopt-current '{"role":"current","workcell_ref":"workcell:local"}'
ctrl --json action run projectcentral.ground.inspect '{"project":"Central"}'
ctrl --json action run control.skills.inspect
ctrl --json action run control.skills.retire '{"scope":"control-user","name":"central-ground-keeping","retired_by":"owner-in-session","retirement_reason":"superseded"}'
ctrl --json action run projectcentral.change.horizon '{"project":"Central"}'
ctrl --json action run projectcentral.now.inspect '{"project":"Central"}'
ctrl --json action run projectcentral.flow.create '{"project":"Central","actor":"human:local","actor_kind":"human","local_stamp":"2026-08-23-2310"}'
ctrl --json action run central.recover '{"role":"primary-workstation"}'
```

`action.list` is the machine-readable discovery surface. Its descriptors include input definitions, mutation class, preview support, required Ports, and availability metadata. Other Surfaces should consume those descriptors rather than maintain a second Action catalog.

## Live World source

`projectcentral.source.read` and `projectcentral.source.write` are the owner
Actions for opening and revising live World source by `SourceRef`, the same
address the Source Change Horizon publishes. `read` returns one source's exact
content revision alongside its provenance, standing, treatment and retrieval
eligibility; `write` is a whole-file compare-and-swap whose emitted Horizon
change carries the declared `actor`, `actor_kind` and optional
`agent_session_ref`. A stale `expected_revision` fails without mutating.

The gates are Central's own, and they are the point of the seam: sources
excluded by `.no-agent-retrieval` are neither read nor written here; and a
write that declares `actor_kind` human never carries an `agent_session_ref` —
a caller declaring both is refused before anything is written. Recognised
human-authored or human-adopted sources, human-source aperture material and
agent-governance sources refuse declared non-human callers and refuse every
agent-session write, who propose instead of writing. Attribution is declared,
not proven: provenance and role recognisers are machine-checked from the
Project's ground relations, and a bare self-declaration of human authorship is
recorded verbatim as declared. Working sources (Flow sources, agent-maintained
Wiki material) remain open to attributed human and Agent callers through the
same Action. No Action invokes an Agent or model.

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

Ordinary file authority, conflict, bounded history and interruption semantics: [Native ordinary-file recovery](NATIVE-ORDINARY-FILE-RECOVERY.md).

Returned work and Flow provider inspection:

- `projectcentral.flow.inspect`: native descriptor, last-observed revision, and retrieval/write/history availability without source body.
- `projectcentral.source.return`: persist exact-basis returned work as a proposal.
- `projectcentral.source.returns`: bounded metadata pages of native returns.
- `projectcentral.source.return_read`: proposal, current source, conflict and native acceptance availability.
- `projectcentral.source.return_accept`: explicitly apply a collaborative-source proposal through its existing native authority/CAS route; acceptance strings do not grant human authority.
- `projectcentral.source.return_reject`: retain rejection without source mutation.

See [Source Return contract](SOURCE-RETURN.md). Flow read accepts optional `expected_revision`; source bodies are bounded to 4 MiB UTF-8 without NUL and retrieval-excluded material is refused.

`central.recognize` takes an explicit absolute `path` and returns bounded structural recognition, canonical directory identity and read-only OS access observations without initialization/adoption or changing the active root. See [Chosen root recognition](ROOT-RECOGNITION.md).
