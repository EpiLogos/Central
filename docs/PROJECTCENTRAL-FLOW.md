# ProjectCentral Flow

**Status:** native owner implementation for O:I Flow · Central #93

A Flow is the continuity identity of one developing linguistic or conceptual thread over an ordinary Project file. Central owns the Project-local source identity, revision-safe mutation, revision provenance, Source Change Horizon participation, and DAY snapshot relation.

The ordinary file remains the human-facing source. Structured owner state lives under `.central/` and is derived operational metadata rather than visible frontmatter.

## Identity and source relation

`FlowRef` is stable across ordinary file rename and across DAY boundaries. `SourceRef` names the current ordinary-file relation and therefore changes when the source path changes.

The preferred first-party creation convention is:

```text
ProjectCentral/now/flows/YYYY-MM-DD-HHMM.md
```

The convention is not identity. `projectcentral.flow.adopt` can give an existing retained Project file a FlowRef without moving it; for example `notes/2026-08-23-2310.md` remains in `notes/`.

Central records the registry in `.central/flows.json` and exact prior/current revision bytes beneath `.central/flow-revisions/`. The current Flow file remains freely refinable.

## Native actions

- `projectcentral.flow.list`
- `projectcentral.flow.read`
- `projectcentral.flow.create`
- `projectcentral.flow.adopt`
- `projectcentral.flow.write`
- `projectcentral.flow.rename`
- `projectcentral.flow.lifecycle`
- `projectcentral.flow.history`

Human and Agent callers use the same `projectcentral.flow.write` whole-file mutation semantics. The caller supplies `expected_revision`; a stale revision fails rather than overwriting newer source. Known actor kind is `human`, `agent`, or `system`, with optional `agent_session_ref`. A direct external editor change is reconciled as `actor_kind=unknown-external` rather than assigned invented authorship.

## Source Change Horizon

Every registered Flow's current ordinary file is a first-class `flow-source` participant in `central.source-change-horizon/v1`, wherever that file lives inside the Project world. The Horizon remains the change authority. Flow operations and horizon reconciliation expose `automatic_agent_or_model_invocation=false`; source change never causes model invocation.

`FlowRef` and Horizon `SourceRef` therefore compose without collapsing:

```text
FlowRef
  continuity of the thread
      ↓ current source relation
SourceRef @ content revision
      ↓
Source Change Horizon
```

## NOW and DAY

`projectcentral.now.rollover` snapshots the exact current revision of every registered Flow into the DAY source snapshot and records FlowRef, SourceRef, source path, revision, lifecycle and snapshot source. The live Flow is not moved, closed, renamed, or re-identified by rollover.

A Flow can therefore remain active through several DAY boundaries while each DAY preserves the exact revision present at close.

## Authority boundaries

Flow is working/collaborative source. It is distinct from authored Ground, Agent Wiki/WikiReading, Claims/Evidence, Run identity, and AgentSession identity. Flow revision history preserves source continuity; it does not confer Claim standing or silently promote material into authored Ground or Wiki canon.
