# Central machine ↔ Workcell relation

Central's `Control/machines/**` source is the durable authored aperture for the computers that participate in a person's operating world. Workcell supplies the current material actuality of those computers.

The first O:I establishment uses this relation directly:

```text
current computer
    ↓ current material discovery
workcell:local
    ↓ explicit bootstrap adoption
Control/machines/current.json
```

`central-machine-adopt` performs that bootstrap through Central's existing machine-inspection contract. On a fresh Central world it creates the `current` machine declaration and records the Workcell reference as an opaque external binding. Repeating the same adoption is stable. An existing different Workcell binding is returned as a conflict for resolution.

A machine declaration therefore carries two related forms of information:

```text
Central authored machine source
  role
  intended capabilities
  requirements
  external bindings

current observation
  platform
  architecture
  observed capabilities/state
  Connector provenance
```

The current local Workcell is the first material context. Additional machines can later bind to remote Workcells through the same durable machine relation.

## Bootstrap command

```sh
central-machine-adopt \
  --root "$CENTRAL_ROOT" \
  --role current \
  --workcell-ref workcell:local \
  --json
```

The command requires an initialized Central root and uses the same native `machine.inspect` application path already used by Central machine planning and verification.

Coordination: `EpiLogos/O-I#131`, `EpiLogos/Central#87`.
