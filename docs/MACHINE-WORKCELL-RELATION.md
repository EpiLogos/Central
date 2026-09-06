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

Coordination: `EpiLogos/O-I#131`, `EpiLogos/Central#87`.
