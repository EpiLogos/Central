# Local endpoint declarations and Central registry

**Status:** Central operational source contract  
**Owner:** Central  
**Project source:** `ProjectCentral/local-endpoints.json`  
**Derived Central reading:** `.central/local-endpoints.json`

## Why this exists

A Work Project often needs several stable network sockets: a development HTTP
server, a database, an RPC service, a debugger, a local model endpoint, or
another process that must coexist with the rest of the person's Work field —
bound to loopback, to a tailnet interface, or to every interface at once.

Those numbers are small facts, but without a shared owner they become
environmental folklore. One Agent chooses `3000`, another Project already owns
it, a database is running on a port that is not visible in the current task
context, and the next session has to rediscover the machine by failure.

Central already owns the durable relation between the personal root and its
Work Projects. The Local Endpoints feature gives that relation a small
operational register:

```text
ProjectCentral declaration
        ↓ aggregate
Central allocation field
        ↕ compare
current machine occupancy, per scope
```

The three terms retain different standing.

- A **Project declaration** says which TCP socket — at which scope — a Project
  expects to use.
- The **Central registry** relates declarations across Projects and detects
  declared collisions within the same scope.
- **Occupancy** is a current observation of this machine, taken on the scope's
  actual interfaces. It does not rewrite Project source and does not establish
  which process owns the socket.

This is deliberately not a service manager. Workcell remains the material
owner of process/service lifecycle. It is also not the Connector `Port`
abstraction: Connector Ports are capability contracts; Local Endpoints are
network socket allocations.

## Project-local source

The optional source is:

```text
Work/<project>/ProjectCentral/local-endpoints.json
```

Its schema is `central.project.local-endpoints/v2`:

```json
{
  "schema": "central.project.local-endpoints/v2",
  "endpoints": [
    {
      "id": "db",
      "kind": "database",
      "service": "postgres",
      "protocol": "tcp",
      "scope": "localhost",
      "port": 5432,
      "description": "Local development database"
    },
    {
      "id": "mesh",
      "kind": "sync",
      "service": "spacetimedb",
      "protocol": "tcp",
      "scope": "tailnet",
      "port": 3000,
      "description": "Tailnet-only SpaceTimeDB"
    },
    {
      "id": "web",
      "kind": "http",
      "protocol": "tcp",
      "scope": "any",
      "port": 8080
    }
  ]
}
```

`id` is stable only inside the Project. `kind` is intentionally open-ended so
Projects can describe the role that matters to them without Central becoming a
catalogue of service types. `service` and `description` are optional
human/Agent-readable refinements.

### The scope field

`protocol` is fixed to `tcp`. `scope` names the interfaces the endpoint binds,
and is one of:

| scope        | interfaces probed and bound |
|--------------|-----------------------------|
| `localhost`  | loopback only: `127.0.0.1`, `::1` |
| `tailnet`    | the machine's tailnet interface addresses: Tailscale IPv4 CGNAT `100.64.0.0/10` and Tailscale IPv6 ULA `fd7a:115c:a1e0::/48`, discovered from the live interfaces (`ip -o addr show` on Linux, `ifconfig -a` on macOS) |
| `any`        | the wildcard binding: `0.0.0.0`, `::` — reachable on every interface |

`scope` defaults to `localhost` when absent. A machine with no tailnet
interface has no observable `tailnet` scope: probing reports `unknown` for it
rather than inventing a result.

**Compatibility.** v1 sources (`central.project.local-endpoints/v1`) remain
readable. Their scope vocabulary is fixed to `localhost`; a v1 file carrying a
non-loopback scope is a validation failure that names the v2 schema. The first
mutation of a v1 source normalises it onto v2 — a strict vocabulary superset
with the same `localhost` default — so no migration is required for existing
Projects.

A missing file means **no declared endpoints**, not an invalid ProjectCentral.

`ProjectCentral/project.json` remains identity/binding metadata only. Mutable
endpoint coordination does not enter that manifest.

## Validation and allocation law

Within one Project:

- endpoint ids are unique;
- the same `(protocol, scope, port)` socket may not be declared twice under
  different ids;
- ports are in `1..=65535`;
- `scope` must be one of `localhost`, `tailnet`, `any` (v2) or `localhost`
  (v1);
- malformed source is surfaced as a verification failure rather than repaired
  silently.

Across Projects, Central groups declarations by `(protocol, scope, port)`. A
collision exists only within the **same scope**: two Projects declaring port
3000 — one `localhost`, one `tailnet` — coexist, because the bindings touch
different interfaces. Such a coexistence is not a collision, and the set
Action's success payload makes it visible (`same_port_other_scopes`, with the
note that a same-port-different-scope relation is not a collision).

`projectcentral.local-endpoints.set` re-reads the Central-wide allocation
field before mutation. If another Project already declares the same
`(protocol, scope, port)` socket, the action refuses the write by default.
Intentional overlap requires `allow_conflict=true`, which keeps the conflict
visible in subsequent Central readings.

Mutating actions take a short-lived Central-wide filesystem lock under
`.central/local-endpoints.lock`. The lock is implementation state, not Project
source; abandoned locks become recoverable after a bounded stale interval.
This prevents two ordinary Agent mutations from both accepting the same
allocation merely because they inspected a moment apart.

## Current occupancy

Occupancy is probed on the scope's own interfaces by attempting a
non-persistent TCP bind on each of the scope's addresses (loopback families
for `localhost`, discovered tailnet addresses for `tailnet`, wildcard binds
for `any`). The probe binds strictly — no `SO_REUSEADDR`/`SO_REUSEPORT` — so
`AddrInUse` means a real address conflict on every platform; a lax bind would
let a wildcard probe report `available` while a specific loopback binding
held the port on macOS. The first `AddrInUse` reports `occupied`; addresses
that cannot exist on this machine are skipped; any other bind failure reports
`unknown`.
The result is one of:

- `available` — Central could bind every probed address of the scope during
  the observation;
- `occupied` — one of the scope's addresses is currently held (the detail
  names the address, e.g. `100.92.62.101:3000 is currently occupied`);
- `unknown` — the machine could not establish either fact safely (for
  example, a `tailnet` declaration on a machine with no tailnet interface).

The probe does not retain the socket. It also does not claim process identity.
An occupied declaration may be the Project's own already-running service or
some unrelated process; that attribution belongs to deeper machine/runtime
observation rather than being guessed by Central.

Because occupancy is temporal evidence, `ProjectCentral/local-endpoints.json`
never stores it.

## Central-wide reading and cache

The canonical current reading is the Action:

```sh
ctrl --json action run central.local-endpoints.inspect '{}'
```

It scans conformant ProjectCentral children under `Work/`, validates each
declared source, reports source errors without hiding the rest of the field,
marks declared same-scope collisions, and attaches the current occupancy
observation for each declaration's scope.

For tools that benefit from a file projection:

```sh
ctrl --json action run central.local-endpoints.refresh '{}'
```

writes the same reading to:

```text
.central/local-endpoints.json
```

That file is explicitly derived and timestamped. It may be deleted or rebuilt
at any time and never outranks the Project declarations it was compiled from.

## Agent-facing Actions

Inspect one Project:

```sh
ctrl --json action run projectcentral.local-endpoints.inspect \
  '{"project":"Central"}'
```

Declare or update a tailnet-scoped database endpoint:

```sh
ctrl --json action run projectcentral.local-endpoints.set \
  '{"project":"Central","id":"db","kind":"database","service":"postgres","port":5432,"scope":"tailnet"}'
```

`scope` is optional and defaults to `localhost`.

Remove a declaration:

```sh
ctrl --json action run projectcentral.local-endpoints.remove \
  '{"project":"Central","id":"db"}'
```

Find a currently bindable and undeclared port:

```sh
ctrl --json action run central.local-endpoints.suggest \
  '{"start":3000,"end":9999}'
```

Suggestion probes the wildcard binding first: a port held on **any** interface
this machine exposes is never suggested, even when loopback alone would look
free. The returned payload reports `free_in` — every scope where the port was
probed and found bindable, with the concrete addresses — and
`unverified_in` — scopes this machine cannot observe. `scope` remains the
default declaration scope (`localhost`) for consumers that declare the
suggestion directly.

Suggestion is advisory. The mutation Action remains the allocation boundary
and rechecks the declaration field before it writes.

## Relation to later runtime integration

The module intentionally stops before process ownership and service
lifecycle. A later Workcell/native observation can enrich the observed side
with process/service identity, PID, container or Workcell reference without
changing the declaration law:

```text
ProjectCentral expected endpoint
        /
Central cross-Project allocation
        /
Workcell material listener/process
```

That extension should add evidence to the reading rather than copy transient
runtime state into Project source.

Coordination: `EpiLogos/Central#182`; interface-aware scoping delivered after
#183.
