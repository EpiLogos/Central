# Local endpoint declarations and Central registry

**Status:** Central operational source contract  
**Owner:** Central  
**Project source:** `ProjectCentral/local-endpoints.json`  
**Derived Central reading:** `.central/local-endpoints.json`

## Why this exists

A Work Project often needs several stable localhost sockets: a development HTTP server, a database, an RPC service, a debugger, a local model endpoint, or another process that must coexist with the rest of the person's Work field.

Those numbers are small facts, but without a shared owner they become environmental folklore. One Agent chooses `3000`, another Project already owns it, a database is running on a port that is not visible in the current task context, and the next session has to rediscover the machine by failure.

Central already owns the durable relation between the personal root and its Work Projects. The Local Endpoints feature gives that relation a small operational register:

```text
ProjectCentral declaration
        ↓ aggregate
Central allocation field
        ↕ compare
current machine occupancy
```

The three terms retain different standing.

- A **Project declaration** says which localhost socket a Project expects to use.
- The **Central registry** relates declarations across Projects and detects declared collisions.
- **Occupancy** is a current observation of this machine. It does not rewrite Project source and does not establish which process owns the socket.

This is deliberately not a service manager. Workcell remains the material owner of process/service lifecycle. It is also not the Connector `Port` abstraction: Connector Ports are capability contracts; Local Endpoints are network socket allocations.

## Project-local source

The optional source is:

```text
Work/<project>/ProjectCentral/local-endpoints.json
```

Its v1 schema is `central.project.local-endpoints/v1`:

```json
{
  "schema": "central.project.local-endpoints/v1",
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
      "id": "web",
      "kind": "http",
      "protocol": "tcp",
      "scope": "localhost",
      "port": 3000
    }
  ]
}
```

`id` is stable only inside the Project. `kind` is intentionally open-ended so Projects can describe the role that matters to them without Central becoming a catalogue of service types. `service` and `description` are optional human/Agent-readable refinements.

Version 1 deliberately fixes `protocol` to `tcp` and `scope` to `localhost`. The network surface can be widened later without pretending that remote/listen-all bindings have the same safety semantics as a local development socket.

A missing file means **no declared endpoints**, not an invalid ProjectCentral. Existing Projects therefore require no migration.

`ProjectCentral/project.json` remains identity/binding metadata only. Mutable endpoint coordination does not enter that manifest.

## Validation and allocation law

Within one Project:

- endpoint ids are unique;
- the same localhost TCP port may not be declared twice under different ids;
- ports are in `1..=65535`;
- malformed source is surfaced as a verification failure rather than repaired silently.

Across Projects, Central groups declarations by `(protocol, scope, port)`. A collision is a real relation between Project declarations, not proof that either service is currently running.

`projectcentral.local-endpoints.set` re-reads the Central-wide allocation field before mutation. If another Project already declares that socket, the action refuses the write by default. Intentional overlap requires `allow_conflict=true`, which keeps the conflict visible in subsequent Central readings.

Mutating actions take a short-lived Central-wide filesystem lock under `.central/local-endpoints.lock`. The lock is implementation state, not Project source; abandoned locks become recoverable after a bounded stale interval. This prevents two ordinary Agent mutations from both accepting the same allocation merely because they inspected a moment apart.

## Current occupancy

For v1 Central probes the loopback TCP socket by attempting a non-persistent bind on the available IPv4/IPv6 loopback families. The result is one of:

- `available` — Central could bind the socket during the observation;
- `occupied` — a loopback bind reported `AddrInUse`;
- `unknown` — the machine could not establish either fact safely.

The probe does not retain the socket. It also does not claim process identity. An occupied declaration may be the Project's own already-running service or some unrelated process; that attribution belongs to deeper machine/runtime observation rather than being guessed by Central.

Because occupancy is temporal evidence, `ProjectCentral/local-endpoints.json` never stores it.

## Central-wide reading and cache

The canonical current reading is the Action:

```sh
ctrl --json action run central.local-endpoints.inspect '{}'
```

It scans conformant ProjectCentral children under `Work/`, validates each declared source, reports source errors without hiding the rest of the field, marks declared collisions, and attaches the current occupancy observation.

For tools that benefit from a file projection:

```sh
ctrl --json action run central.local-endpoints.refresh '{}'
```

writes the same reading to:

```text
.central/local-endpoints.json
```

That file is explicitly derived and timestamped. It may be deleted or rebuilt at any time and never outranks the Project declarations it was compiled from.

## Agent-facing Actions

Inspect one Project:

```sh
ctrl --json action run projectcentral.local-endpoints.inspect \
  '{"project":"Central"}'
```

Declare or update a database endpoint:

```sh
ctrl --json action run projectcentral.local-endpoints.set \
  '{"project":"Central","id":"db","kind":"database","service":"postgres","port":5432}'
```

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

Suggestion is advisory. The mutation Action remains the allocation boundary and rechecks the declaration field before it writes.

## Relation to later runtime integration

The v1 module intentionally stops before process ownership and service lifecycle. A later Workcell/native observation can enrich the observed side with process/service identity, PID, container or Workcell reference without changing the declaration law:

```text
ProjectCentral expected endpoint
        /
Central cross-Project allocation
        /
Workcell material listener/process
```

That extension should add evidence to the reading rather than copy transient runtime state into Project source.

Coordination: `EpiLogos/Central#182`.
