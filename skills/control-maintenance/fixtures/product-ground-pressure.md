# Fixture — returned reality pressures product ground without rewriting it

## Existing authored source

```text
Control/user/products/example/positions/INTERACTION.md
source class: authored

I want the primary interaction to remain directly manipulable by the person rather than becoming an agent-only command surface.
```

The exact path below `Control/user/products/example/` is conventional, not schema-required.

## Returned implementation evidence

```text
native repository: example-product
source class: implementation fact
revision: current reviewed head
finding: the only implemented mutation path is currently exposed through an agent tool; the human surface is read-only
```

## Returned experimental evidence

```text
source class: experimental finding
finding: in the current usability fixture, a person cannot complete the intended manipulation without asking an agent to act
```

Neither finding is human-authored Control source.

## Correct Control-maintenance result

```text
Finding
  classification: conflicting / pressure-on-authored-ground
  target source: Control/user/products/example/positions/INTERACTION.md
  evidence:
    - implementation fact at current reviewed revision
    - experimental finding from current usability fixture
  reason:
    current reality does not realise the authored interaction position

Proposed durable change
  target: Control/user/products/example/positions/INTERACTION.md
  reason: only if the human decides the intended product relation itself should change
  supporting context: exact implementation + experimental provenance above
  final diff: shown for review, not applied

Project/local exclusion
  the implementation gap, test mechanics and current branch state remain native-repository evidence; they are not copied into Control as durable authored preference

Acceptance
  pending
```

## Required invariant

Before explicit human acceptance:

```text
authored source mutated: false
proposal exists: true
returned evidence preserved with its own source class: true
```

The alternative resolution is equally legal: the human may keep the authored position unchanged and direct the implementation to move toward it. The evidence creates pressure for an explicit return; it does not choose which side becomes canonical.
