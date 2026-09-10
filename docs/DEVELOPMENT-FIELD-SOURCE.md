# Development Field source contract

Status: implementation contract for Central S1 (`EpiLogos/Central#136`), reconciled on current Central ground for PR #145. The O:I whole-level Wayfinder remains the cross-product context; this document states only what Central owns.

Central owns durable source identity, filesystem aperture, source revision, provenance/standing and authored machine intent. It does not own Wiki cognition, Factory developmental execution, Git semantics, Workcell material actuality, Actuation, or O:I suite selection.

## Self-description aperture

The additive source homes are:

```text
Control/self/**
Work/<project>/ProjectCentral/self/**
```

`self/**` is what the scoped World or Project says it is. It remains distinct from ordinary human writing (`user/**`), Agent governance, Agent Wiki source, Skills, Methods and the temporal NOW/DAY field. Creating the directory does not create an authored document.

A file acquires no human authority merely by being placed below `self/`. An unbound file is disclosed as unresolved location-only source. Generated accounts, HTML or matrices therefore remain generated/unresolved until an explicit source relation says otherwise.

Existing Project documents need not move into `ProjectCentral/self`. `projectcentral.self.retain-tier` records an accepted source relation and keeps the native bytes/path in place.

## Six stable tier identities

The machine-level tier identity is the integer `0..5`. Central does not invent replacement display labels or mandatory filenames. A binding may optionally retain a canonical label/path supplied by authored source.

The semantic offices used for validation/readability are:

```text
0  originating authored ground / why / positions / intent
1  intended experience / vision
2  design / capabilities / functional form
3  architecture / contracts / owned relations
4  implementation / active development / operational form
5  evidence / returned reality / Recognition pressure
```

These offices do not imply six files. Each tier may bind zero or more canonical Central `SourceRef`s. Readings resolve current provenance, standing and revision from the source owner rather than copying them into the Development Field relation ledger.

The relation carrier is additive metadata:

```text
Control/relations/development-field.json
ProjectCentral/relations/development-field.json
```

Protocol: `central.development-field-sources/v1`.

## UX and EX

A `ux_ref` binds intended human experience to an already human-authored/adopted source. UX is automatically related to tier `1`; it is explicitly not an implementation fact, test result, Agent inference or EX return.

An `ex_ref` binds actual human experiential return to an already human-authored/adopted source carrying `observed-evidence` standing. EX may name linked `ux_ref`s and artifact/evidence refs, but does not copy their payloads. EX is automatically related to tier `5`.

`central.self.ex.relate` and `projectcentral.self.ex.relate` refuse Agent-only promotion: the call must declare a human actor, contain no `agent_session_ref`, and explicitly acknowledge `human-accepted`. More importantly, the source must already resolve through Central with human-authored or human-adopted provenance. A generated/Agent/observed file cannot become human experiential truth because it sits near self-description source.

## Doctor and additive migration

`central.self.inspect` and `projectcentral.self.inspect` report one of:

```text
present
legacy-migratable-absence
invalid-broken-source-state
ambiguous-source-relation
```

Legacy absence is non-fatal. `central.self.ensure` and `projectcentral.self.ensure` only create the missing aperture; they do not move documentation or matrices. Broken directories/symlinks or malformed relation carriers are invalid. Relations whose `SourceRef`s no longer resolve are ambiguous and require source reconciliation/human judgement rather than silent rebinding.

Ordinary `central.init` retains the pre-S1 root floor and does not create or require `Control/self`; the S1 ensure Actions own that additive aperture. An otherwise valid old root therefore stays valid until the aperture is explicitly introduced.

## Public Action surface

The native Action field includes:

```text
central.self.inspect
central.self.ensure
central.self.source.create
central.self.tier.relate
central.self.ux.relate
central.self.ex.relate
central.self.resolve

projectcentral.self.inspect
projectcentral.self.ensure
projectcentral.self.source.create
projectcentral.self.retain-tier
projectcentral.self.tier.relate
projectcentral.self.ux.relate
projectcentral.self.ex.relate
projectcentral.self.resolve

machine.oi-suite-policy
```

The inspect/resolve readings expose source refs, paths, provenance, standing and exact current content revision while withholding source payloads. Consumers therefore do not need to infer arbitrary filesystem conventions.

## Machine → O:I policy intent

Machine declarations already carry opaque typed external bindings. S1 reserves the Central-side binding kind:

```json
{
  "kind": "oi-suite-policy",
  "reference": "mainline"
}
```

The reference remains opaque to Central; O:I decides what `mainline` (or another policy ref) means. `machine.oi-suite-policy` returns authored desired policy only. It explicitly does not report or own the active O:I suite receipt, installed product revisions, or Workcell/material executable observation.

The existing `workcell` binding remains unchanged and opaque. A machine can carry both relations because they answer different questions:

```text
workcell binding       where material actuality is owned
O:I suite-policy       what suite/channel the human intends for this machine
O:I install receipt    what software is actually installed (external to Central)
```

## Deliberate deferrals

S1 does not bulk move existing docs or capability matrices, create Development Field Skills/Methods, implement AIKit intelligence traversal, implement Factory Run/Recognition semantics, or make Central a suite/version manager. S7 or a later intelligence tranche can reconcile existing carriers against these accepted source relations without inventing provenance during cleanup.
