# Git Synchronizer Connector

`central-git-sync-connector` is the fresh-session proving Connector for Central's public `Synchronizer` Port.

It was authored from the published `Synchronizer` contract, `docs/CONNECTOR-SDK-SPEC.md`, `skills/connector-authoring/SKILL.md`, and Git's public command behavior. The runtime crate depends only on `central-connector-sdk`; it does not import private `ctrl` implementation modules or another Connector as its contract.

## Public contract

The Connector declares:

```text
id       personal.git-sync
Port     Synchronizer 1.0.0
scope    externally-mutating
source   kind = git
```

A recovery declaration can therefore use:

```json
{
  "schema": "central.recovery",
  "version": 1,
  "role": "example-role",
  "synchronization": {
    "id": "central-authored-source",
    "source": {
      "kind": "git",
      "reference": "/path/or/url/to/repository.git"
    }
  }
}
```

The selected Connector interprets the provider-defined source fields. Core recovery does not know Git semantics.

## Target configuration

Set:

```text
CENTRAL_GIT_SYNC_TARGET=/path/to/existing/git-working-tree
```

`CENTRAL_GIT_EXECUTABLE` can optionally name a non-default Git executable.

The capability probe is read-only. It requires an available Git executable and an existing Git working tree at the configured target.

## Preview and apply

`preview` compares:

```text
local target HEAD
    ↕
source HEAD from git ls-remote
```

It does not fetch or mutate the working tree.

When the source is ahead or otherwise different, `apply`:

1. refuses a dirty working tree;
2. fetches only the source `HEAD` into `FETCH_HEAD`;
3. performs `git merge --ff-only FETCH_HEAD`;
4. reruns the public preview and requires the source to be satisfied.

Divergent history is therefore an explicit provider failure rather than an implicit merge, reset, or overwrite. Repeating a satisfied synchronization is a no-op.

## Acceptance

The target-specific acceptance suite uses the real Git executable and temporary repositories. It proves:

- a genuinely changeable Git target passes the shared `Synchronizer` conformance suite;
- shared conformance performs and verifies a real fast-forward mutation;
- a dirty target is rejected without overwriting local work;
- canonical `central.recover` resolves `personal.git-sync` through the public Port and synchronizes the real Git target;
- repeated canonical recovery is stable.

## Fresh-session hardening result

The black-box exercise exposed one general public conformance gap before provider acceptance: the original mutating `Synchronizer` conformance fixture was allowed to begin already satisfied, and the suite did not require a changeable preview to produce `StateChangeResult.changed = true`. A Connector could therefore pass the mutating path without conformance proving any mutation.

The problem was classified as **SDK support / public conformance**, not as a Git-specific exception. The shared suite now:

- requires the fixture to begin changeable;
- requires the first successful apply to report the mutation it performed;
- still requires post-apply satisfaction and repeat idempotence.

SDK-level regression tests prove both former false-positive cases are rejected. No Git/platform branch was added to a canonical Action.
