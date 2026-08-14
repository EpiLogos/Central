# Central recovery protocol

**Status:** implementation contract for the Rust Central programme

Central recovery restores an authored machine role by composing existing public architecture. It is not a second package/configuration engine and it does not define synchronization in terms of one provider.

## Ownership

The recovery path is:

```text
optional authored synchronization intent
        ↓
central.recovery.plan / central.recover
        ↓
public Synchronizer Port
        ↓
selected Synchronizer Connector
        ↓
existing machine.apply
        ↓
PackageManager / ConfigurationManager / ServiceManager Ports
        ↓
existing machine.verify
```

`central.recover` owns orchestration and result composition only. `machine.apply` continues to own package, configuration, and service reconciliation. A Synchronizer owns only the configured synchronization relation it implements.

Recovery must not contain platform, package-provider, configuration-provider, or synchronization-provider branches in core logic.

## Authored recovery source

A machine role may optionally declare a synchronization relation beside its machine declaration:

```text
Control/machines/<role>.recovery.json
```

Version 1 has the shape:

```json
{
  "schema": "central.recovery",
  "version": 1,
  "role": "primary-workstation",
  "synchronization": {
    "id": "central-authored-source",
    "source": {
      "kind": "provider-defined-kind",
      "reference": "provider-defined-reference"
    }
  }
}
```

This file is **authored Control source**. It names semantic synchronization intent and an optional source reference. The public contract does not assign meaning to provider-defined `kind` or `reference`; the selected Synchronizer Connector validates and interprets them.

Absence of `<role>.recovery.json` means no synchronization is configured. It is not an error and recovery proceeds directly to ordinary machine reconciliation.

A present malformed declaration is an explicit input failure. Central must not silently treat malformed authored recovery intent as absent.

## Public Synchronizer Port

`Synchronizer` v1 exposes two typed operations:

```text
preview(SynchronizationRequest) -> StateChangePreview
apply(SynchronizationRequest)   -> StateChangeResult
```

The public invariants are:

- preview is read-only;
- apply is externally mutating;
- apply is previewable;
- both operations are idempotent at the semantic contract level;
- a successful apply must be followed by a preview that reports `changed = false`;
- repeating apply after satisfaction must report `changed = false`;
- capability probing determines whether a Connector is eligible in the current environment.

The shared `run_synchronizer_conformance` suite proves those invariants for implementations.

## Actions

### `central.recovery.plan`

Input:

```json
{ "role": "<machine-role>" }
```

Behavior:

1. invoke canonical `machine.plan` for the role;
2. read the optional authored recovery declaration;
3. when synchronization is configured, resolve public `Synchronizer` and request a non-mutating preview;
4. return machine and synchronization plans in one structured result.

Synchronization status is one of:

```text
not_configured
satisfied
changeable
unavailable
```

A configured but unavailable Synchronizer is visible in the plan with Connector diagnostics.

### `central.recover`

Input:

```json
{ "role": "<machine-role>" }
```

Behavior:

1. build the same recovery plan;
2. if configured synchronization is unavailable, fail before machine mutation;
3. apply synchronization only when its preview is changeable;
4. invoke canonical `machine.apply` unchanged;
5. invoke canonical `machine.verify` unchanged;
6. return one structured recovery report.

This ordering prevents a missing required synchronization provider from leaving the machine partially reconciled before the missing prerequisite is discovered.

## Result semantics

Recovery preserves Central's existing structured outcome meanings:

```text
success
    configured synchronization and machine reconciliation are satisfied, and final verification passes

unavailable_capability
    a required configured synchronization or machine capability cannot be supplied

partial_completion
    some mutation completed but the authored role remains only partially satisfiable, or a later stage fails after an earlier mutation

verification_failure
    mutations ran but observed state still does not satisfy authored machine intent

connector_failure
    a selected provider operation failed

invalid_input
    authored recovery input is malformed or inconsistent
```

A recovery implementation must never convert a partial or verification failure from machine reconciliation into success merely because synchronization succeeded.

## Stability

When synchronization and machine state are already satisfied:

```text
central.recover
    synchronization: no mutation
    machine.apply: zero operations
    machine.verify: satisfied
```

Repeating a complete recovery must therefore be a stable no-op.

## Platform proving

Core recovery semantics are provider-neutral. macOS and Ubuntu proving lines should consume the same recovery Actions with their public Connectors; platform-specific acceptance belongs in those extension stacks rather than in recovery core.

Named physical-machine acceptance remains separate from hosted implementation evidence. A GitHub macOS/Linux runner must not be presented as proof of the user's named workstation or home server.
