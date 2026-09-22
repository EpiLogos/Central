# Central machine ↔ Workcell relation

Central's `Control/machines/**` source is the durable authored aperture for the computers that participate in a person's operating world. Workcell supplies the current material actuality of those computers.

The first O:I establishment uses this relation directly:

```text
current computer
    ↓ observed through native machine inspection (MachineInspector Port)
current local Workcell (`workcell:local`, opaque to Central)
    ↓ explicit accepted bootstrap adoption (machine.adopt-current)
Control/machines/current.json
```

`machine.adopt-current` is the native owner Action for that bootstrap. It inspects the current machine through the same MachineInspector Port used by planning and verification, writes the role declaration when absent, seeds the observed capabilities as the initial accepted capability intent, records the Workcell reference as an opaque external binding, and returns the authored declaration together with the observed evidence. Repeating the same adoption is a no-op success (`unchanged`); an existing declaration that names a different Workcell binding is returned as an explicit conflict for resolution, never rewritten.

## Default O:I bootstrap call

O:I first-suite establishment calls:

```text
role          current
workcell_ref  workcell:local
```

Both inputs are optional and default to exactly these values, so the default call is either of:

```sh
ctrl --json action run machine.adopt-current '{"role":"current","workcell_ref":"workcell:local"}'
ctrl --root "$CENTRAL_ROOT" machine adopt-current
```

The role remains an ordinary Central machine role and can later be revised through the native authored-source path. The Workcell ref remains an opaque relation to the material owner; Central never interprets its content.

## Declaration layout and the binding

The declaration is the file `Control/machines/<role>.json`, exactly as the canonical declaration reader addresses it. A machine role may also have a directory `Control/machines/<role>/` (for example machine-scoped skills under `Control/machines/current/skills/`); the file and the directory are distinct entries and coexist — the reader resolves `<role>.json` and never treats the directory as a declaration.

The Workcell relation is retained as an explicit, additive binding list on the declaration:

```json
"bindings": [
  { "kind": "workcell", "reference": "workcell:local" }
]
```

Bindings are typed `{ kind, reference }` entries and opaque to Central. Declarations authored before bindings existed parse and serialise identically — an absent or empty binding list is simply absent — and adoption adds the Workcell binding to such declarations without disturbing their authored capabilities, requirements, or unknown authored fields.

A machine declaration therefore carries two related forms of information:

```text
Central authored machine source
  role
  intended capabilities
  requirements
  external bindings

current observation (evidence in the adoption result)
  platform
  architecture
  observed capabilities/state
  Connector provenance
```

The current local Workcell is the first material context. Additional machines can later bind to remote Workcells through the same durable machine relation.

## O:I suite policy is a second opaque authored relation

Development Field S1 does not turn Central into a suite/version manager. The existing generic binding shape is sufficient for an authored machine role to express desired O:I suite policy alongside its Workcell relation:

```json
"bindings": [
  { "kind": "workcell", "reference": "workcell:local" },
  { "kind": "oi-suite-policy", "reference": "mainline" }
]
```

`oi-suite-policy` is the Central-side binding kind; its reference is deliberately opaque here. O:I owns the semantics of a channel/policy reference and the active installed-suite receipt. Workcell/native observation owns material executable actuality. Central owns only the authored intent carried by the machine declaration.

The native read surface is:

```sh
ctrl --json action run machine.oi-suite-policy '{"role":"current"}'
```

It reports `absent`, one `authored-intent`, or `ambiguous-human-decision-required` when several different policy refs are authored. It never interprets the reference and never reports installed product versions as though they were authored machine intent. `machine.adopt-current` continues to touch only the `workcell` binding kind, so the two relations remain independent.

## Worktree-projection policy is authored desired state, verdict is AIKit's

Where `oi-suite-policy` is an *opaque* binding, a machine role may also carry a *structured* `projection` policy: the desired state that this machine keeps its suite repository checkouts projected onto one canonical target (e.g. `origin/main`).

```json
"projection": {
  "target": "origin/main",
  "projects": []
}
```

`target` is the canonical revision every covered checkout tracks; `projects` names the suite project keys the policy covers, and an empty list means the whole suite. The field is additive — declarations authored before it existed parse and serialise identically because a `None` is skipped.

This is authored intent only. **Central declares the target; it never runs git or computes the drift.** Repository/worktree state — and the projection drift verdict — belong to AIKit (`aikit worktree project`, `aikit.worktree-projection/v1`), per the suite ownership law (`Work/Workcell/docs/DEVELOPMENT-WORLDS.md`). `machine.plan` / `machine.verify` surface the verdict for the policy only from an AIKit reading supplied to Central:

```text
no reading supplied      Unsupported — the verdict is computed by
                         `aikit worktree project --json`; Central names the
                         verifier rather than inventing a git answer, exactly as
                         an empty capability slot is Unsupported when no
                         reconciliation Port observes it
every covered checkout   Satisfied
  projected
any covered checkout     Missing — the drifted repo keys are named, and the
  not projected (or a     repair is `aikit worktree project --apply` (AIKit owns
  named project absent,    it). Never Changeable: Central owns no git Port, so
  or the reading against   `machine.apply` has nothing to run for it
  another target)
```

The reading is passed as the `projection_reading` input (an inline `aikit.worktree-projection/v1` object), or through the CLI as `machine plan <role> --projection-reading <file.json>` / `machine verify <role> --projection-reading <file.json>` — ctrl reads that caller-supplied file and passes it through; ctrl never shells to git.

## Bootstrap binary

The `central-machine-adopt` binary (shipped with `ctrl`) performs the same bootstrap through the same `machine.inspect` application path and writes the same ground; it predates the native Action and remains available for scripted first establishment:

```sh
central-machine-adopt \
  --root "$CENTRAL_ROOT" \
  --role current \
  --workcell-ref workcell:local \
  --json
```

Both surfaces produce the declaration form documented above; `machine.adopt-current` is the canonical one.

Coordination: `EpiLogos/O-I#131`, `EpiLogos/Central#87`, `EpiLogos/Central#136`.
