# Central documentation

This index explains the documentation corpus by **role** rather than alphabetical
filename. Each document has one authority class: normative source law, product
meaning, implementation evidence, or supporting guidance. Read the class that
matches what you are doing.

## The personal root and the product source are separate

A Central **personal root** is the lived authored world:

```text
~/Central/
├── Control/    durable human-authored source (user, agents, machines)
├── Work/       ordinary work
├── .central/   derived local state (implementation-owned)
└── .obsidian/  local editor state when the person opens the root in Obsidian
```

The Central **product source checkout** (this repository) is a developer artifact
and does not own, track, or require any of those personal directories. A
convenient home for the checkout is `~/Central/Work/Central` on a machine whose
personal root is `~/Central` — the same convention used for the other suite
products under `Work/`. `ctrl doctor` diagnoses a personal root that also
resembles the product checkout (`mixed_root` in the structured health report).

## Product meaning

- [CENTRAL-VISION.md](CENTRAL-VISION.md) — why the authored root exists, the
  experience it preserves, and what Central is (and is not) inside the {O:I}
  field. **Start here if you are new.**
- [CENTRAL-PUBLIC-HANDOFF.md](CENTRAL-PUBLIC-HANDOFF.md) — concise outward-facing
  framing for O:I/site descriptions without replacing the canonical vision.

## Normative system / source law

- [CENTRAL-SYSTEM-SPEC.md](CENTRAL-SYSTEM-SPEC.md) — the normative product and
  architecture specification: authored source classes, Actions, Ports,
  Connectors, and the observation/authoring boundary.
- [CONTROL-CONTENT-PROTOCOL.md](CONTROL-CONTENT-PROTOCOL.md) — authorship,
  durable information, disclosure boundaries and the Control source roots.
- [CONTROL-RETRIEVAL-TREATMENT.md](CONTROL-RETRIEVAL-TREATMENT.md) — the
  executable `.no-agent-retrieval` treatment that implements the protocol's
  disclosure boundary (supporting the Content Protocol).
- [AGENT-GOVERNANCE-SOURCES.md](AGENT-GOVERNANCE-SOURCES.md) — layered
  human-authored root/Project Agent-governance sources and their relation to
  AIKit operational composition.

## Personal / projected world

- [PRODUCT-GROUND-CONVENTION.md](PRODUCT-GROUND-CONVENTION.md) — the optional
  human-authored product-ground convention under `Control/user/products/` and
  the returned-reality proposal boundary.
- [PERSONAL-WORLD-PROJECTION.md](PERSONAL-WORLD-PROJECTION.md) — how selected
  personal/world material is projected publicly and explicitly returned to
  Central source.
- [PERSONAL-EXTENSION-SPEC.md](PERSONAL-EXTENSION-SPEC.md) — the first real
  extension set, used to prove and harden the public extension architecture.

## Actions / Connectors / public implementation

- [CONNECTOR-SDK-SPEC.md](CONNECTOR-SDK-SPEC.md) — the Action, Port, Connector,
  Surface, SDK and conformance architecture.
- [CONNECTOR-SDK-RUST.md](CONNECTOR-SDK-RUST.md) — the executable Rust SDK
  reference: published Ports, versions and conformance harnesses.
- [CLI-REFERENCE.md](CLI-REFERENCE.md) — the stock `ctrl` command surface:
  canonical Actions, invocation seams and the result/exit contract.

## Installation / recovery

- [INSTALL.md](INSTALL.md) — native `ctrl` installation and clean-root
  verification.
- [MACHINE-WORKCELL-RELATION.md](MACHINE-WORKCELL-RELATION.md) — how the current
  computer becomes the first durable `Control/machines` relation and binds to
  its current Workcell material context.
- [RECOVERY-PROTOCOL.md](RECOVERY-PROTOCOL.md) — recovery of Central machine
  state through the canonical Actions and Ports.

## Evidence / acceptance

- [PRODUCT-ACCEPTANCE-MATRIX.md](PRODUCT-ACCEPTANCE-MATRIX.md) — the #19
  hosted/remote acceptance matrix against the normative criteria, and the
  explicit separation of hosted evidence from named physical-machine evidence.

## ProjectCentral

- [PROJECTCENTRAL-CONTRACT.md](PROJECTCENTRAL-CONTRACT.md) — the authored Project
  ground and ProjectCentral ↔ Central integration contract.
- [PROJECTCENTRAL-AUTHORED-GROUND.md](PROJECTCENTRAL-AUTHORED-GROUND.md) —
  conservative inspection and explicit accepted source/provenance/standing
  relations for existing or ProjectCentral-local human Project source.
- [PROJECTCENTRAL-NOW.md](PROJECTCENTRAL-NOW.md) — the opt-in NOW temporal field,
  DAY source snapshots/closure, bounded Agent returns, promotion lineage and
  rollover semantics over an already-valid ProjectCentral.

## Supporting product understanding

- [VISUAL-PRODUCT-UNDERSTANDING.md](VISUAL-PRODUCT-UNDERSTANDING.md) — the
  canonical visual understanding of the product.

## Reading routes

| If you are... | Read in this order |
|---|---|
| New to Central | `CENTRAL-VISION.md` → `CENTRAL-SYSTEM-SPEC.md` → `CONTROL-CONTENT-PROTOCOL.md` |
| Implementing a Connector | `CONNECTOR-SDK-SPEC.md` → `CONNECTOR-SDK-RUST.md` → `CONNECTOR-SDK-RUST.md` conformance harnesses → `skills/connector-authoring` |
| Operating `ctrl` | `CLI-REFERENCE.md` → `INSTALL.md` |
| Establishing the current machine | `MACHINE-WORKCELL-RELATION.md` → `CLI-REFERENCE.md` |
| Working with authored Project ground | `PROJECTCENTRAL-CONTRACT.md` → `PROJECTCENTRAL-AUTHORED-GROUND.md` → `CLI-REFERENCE.md` |
| Working with Project NOW / DAY | `PROJECTCENTRAL-CONTRACT.md` → `PROJECTCENTRAL-NOW.md` → `CLI-REFERENCE.md` |
| Working with Agent governance source | `AGENT-GOVERNANCE-SOURCES.md` → `CONTROL-CONTENT-PROTOCOL.md` → AIKit operational composition docs |
| Recovering a machine | `RECOVERY-PROTOCOL.md` → `machine.declaration` Skill → `CONTROL-CONTENT-PROTOCOL.md` |
| Accepting the product | `PRODUCT-ACCEPTANCE-MATRIX.md` → `CENTRAL-SYSTEM-SPEC.md` |

The root [README](../README.md) carries product meaning and the compact
dependency rule; this index carries the corpus.
