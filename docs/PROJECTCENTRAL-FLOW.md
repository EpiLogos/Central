# Flow

```yaml
standing: architecture-contract
register: episteme
provenance: human-adopted
temporal: docs update after Central #134
```

A Flow is the continuity identity of one developing linguistic or conceptual thread over an ordinary file. Central owns that identity, revision-safe mutation, revision provenance, Source Change Horizon participation, and the DAY snapshot relation.

This contract is the continuity law of the temporal working field. The ordinary file remains the human-facing source. Structured owner state lives under `.central/` at the register root and is derived operational metadata rather than visible frontmatter.

## Register

A Flow belongs to a **register**. One implementation serves both. They differ in where the NOW field sits and how the register names itself:

```text
Central root `control:root`     Control/agents/now/flows
                                refs: central:flow:control:root:
                                      central:source:control:root:

Work project                    ProjectCentral/now/flows
                                refs: central:flow:project:<id>:
                                      central:source:project:<id>:
```

`project` is optional on every `projectcentral.flow.*` Action. **Absent `project` names the root register.** A supplied `project` names `Work/<project>`.

## Identity and naming

`FlowRef` is stable across ordinary file rename and across DAY boundaries. `SourceRef` names the current ordinary-file relation and therefore changes when the source path changes.

The naming law is slug-first. Create writes:

```text
<slug>-YYYY-MM-DD-HHMM.md
```

`slug` is a kebab-case reading of the title (at most six words). An empty title yields the stamp alone (`YYYY-MM-DD-HHMM.md`). `local_stamp`, when supplied, is `YYYY-MM-DD-HHMM`; otherwise Central writes the local civil stamp. Path is not identity.

NOW day grouping reads the civil date from the **end** of the stem. A stem that is only the stamp still reads. A filename that carries no stamp is honestly undated; Central never guesses a civil date from a timezone.

Create writes a blank ordinary file in the register's Flow directory. Adopt gives an existing retained file a FlowRef without moving it.

## Native actions

`project` is optional on each Action below. Omit it to address `control:root`.

- `projectcentral.flow.inspect`
- `projectcentral.flow.list`
- `projectcentral.flow.read`
- `projectcentral.flow.create`
- `projectcentral.flow.adopt`
- `projectcentral.flow.write`
- `projectcentral.flow.rename`
- `projectcentral.flow.lifecycle`
- `projectcentral.flow.history`
- `projectcentral.flow.now`

Human and Agent callers use the same `projectcentral.flow.write` whole-file mutation semantics. The caller supplies `expected_revision`; a stale revision fails rather than overwriting newer source. Known actor kind is `human`, `agent`, or `system`, with optional `agent_session_ref`. A direct external editor change is reconciled as `actor_kind=unknown-external` rather than assigned invented authorship.

`projectcentral.flow.now` is the owner NOW reading of registered Flows: live / held / closed, day groups by the embedded stamp, caller-supplied `current_day` DAY facts, rest-vs-thinking disclosure. A local civil date boundary does not close a Flow and does not mint a new identity. Day grouping is presentation, never semantic identity.

## Source Change Horizon

Every registered Flow whose retained file is present is a first-class `flow-source` participant in `central.source-change-horizon/v1`, in the horizon of its register. Root Flows reconcile through Central's own root horizon. A Flow whose retained file has gone is absent from the horizon, never a failure of it.

Flow operations and horizon reconciliation expose `automatic_agent_or_model_invocation=false`; source change never causes model invocation.

`FlowRef` and Horizon `SourceRef` compose without collapsing:

```text
FlowRef
  continuity of the thread
      ↓ current source relation
SourceRef @ content revision
      ↓
Source Change Horizon of that register
```

## NOW and DAY

DAY close snapshots the exact current revision of every registered Flow into the DAY source snapshot and records FlowRef, SourceRef, source path, revision, lifecycle and snapshot source. The live Flow is not moved, closed, renamed, or re-identified by rollover.

A Flow can remain active through several DAY boundaries while each DAY preserves the exact revision present at close.

Implementation standing: `projectcentral.now.rollover` currently requires `project` and snapshots the project register. Root DAY close follows the same snapshot law through the root field's mirroring procedure until a native root NOW Action exists. `projectcentral.flow.now` already reads both registers.

## Authority boundaries

Flow is working/collaborative source. It is distinct from authored Ground, Agent Wiki/WikiReading, Claims/Evidence, Run identity, and AgentSession identity. Flow revision history preserves source continuity; it does not confer Claim standing or silently promote material into authored Ground or Wiki canon.

Placement refuses overlap with existing Central authority containers. At the root: `Control/user`, `Control/agents/governance`, `Control/agents/wiki`, `Control/agents/now/user`, `Control/agents/now/agents`, `Control/machines`. In a project: the Project's human source, `ProjectCentral/agents/governance`, `ProjectCentral/agents/wiki`, `ProjectCentral/now/user`, `ProjectCentral/now/agents`.

## ProjectCentral

The project register is a Work project's ProjectCentral. Default create path:

```text
Work/<project>/ProjectCentral/now/flows/<slug>-YYYY-MM-DD-HHMM.md
```

Operational state lives at the project root:

```text
Work/<project>/.central/flows.json
Work/<project>/.central/flow-revisions/
```

`projectcentral.flow.adopt` can give an existing retained Project file a FlowRef without moving it; for example `notes/2026-08-23-2310.md` remains in `notes/`.

`projectcentral.now.init` opts a valid ProjectCentral into the NOW field. It does not create `flows/`. The first create (or an explicit path) materialises the directory. The current Flow file remains freely refinable.
