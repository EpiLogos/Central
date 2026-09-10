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
- [DEVELOPMENT-FIELD-SOURCE.md](DEVELOPMENT-FIELD-SOURCE.md) — the Central-owned
  Development Field source contract: root/Project `self` apertures, stable 0..5
  tier bindings, UX/EX source relations, retained-native source relations and
  opaque machine O:I suite-policy intent.
- [CONTROL-CONTENT-PROTOCOL.md](CONTROL-CONTENT-PROTOCOL.md) — authorship,
  durable information, disclosure boundaries and the Control source roots.
- [PROJECT-CONTEXT-PROTOCOL.md](PROJECT-CONTEXT-PROTOCOL.md) — the sixfold
  Project-context movement (Ground → World → Praxis → Intent → Context Frame →
  Return), the context-source form, and the boundary between source standing,
  authority, disclosure/activation and runtime precedence.
- [CAPABILITY-MATRIX-PROTOCOL.md](CAPABILITY-MATRIX-PROTOCOL.md) — the shared
  product-account and capability-matrix contract (`ql-capability-matrix/1`):
  seed-led HTML accounts, matrix manifests, CSV interchange, the
  reviewed-basis reconciliation plan, and product-ground validation.
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
- [PERSONAL-SURFACE-AND-NOTIFICATION.md](PERSONAL-SURFACE-AND-NOTIFICATION.md) —
  the composable Personal authored-ground Action extension and provider-neutral
  user-notification Port, including the distinction between delivery and human
  acknowledgement.

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
- [PROJECTCENTRAL-NOW.md](PROJECTCENTRAL-NOW.md) — Central NOW/DAY temporal
  contract (root register `control:root` and ProjectCentral), DAY snapshots,
  bounded Agent returns, promotion and rollover.
- [PROJECTCENTRAL-FLOW.md](PROJECTCENTRAL-FLOW.md) — Flow continuity identity
  across registers, optional `project` (absent = root), naming law, revision-safe
  writes, Source Change Horizon participation and exact DAY revision snapshots.

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
| Working with Development Field source | `DEVELOPMENT-FIELD-SOURCE.md` → `CONTROL-CONTENT-PROTOCOL.md` → `PROJECTCENTRAL-AUTHORED-GROUND.md` → `CLI-REFERENCE.md` |
| Working with authored Project ground | `PROJECT-CONTEXT-PROTOCOL.md` → `PROJECTCENTRAL-CONTRACT.md` → `PROJECTCENTRAL-AUTHORED-GROUND.md` → `CLI-REFERENCE.md` |
| Working with Project context / Intent / disclosure | `PROJECT-CONTEXT-PROTOCOL.md` → `PRODUCT-GROUND-CONVENTION.md` → `AGENT-GOVERNANCE-SOURCES.md` → AIKit ContextResolution/Explain contracts |
| Working with NOW / DAY | `PROJECTCENTRAL-NOW.md` → `CLI-REFERENCE.md` |
| Working with a live Flow | `PROJECTCENTRAL-FLOW.md` → `PROJECTCENTRAL-NOW.md` → `CLI-REFERENCE.md` |
| Working with Agent governance source | `PROJECT-CONTEXT-PROTOCOL.md` → `AGENT-GOVERNANCE-SOURCES.md` → `CONTROL-CONTENT-PROTOCOL.md` → AIKit operational composition docs |
| Working with Personal notification/proposal extensions | `PERSONAL-SURFACE-AND-NOTIFICATION.md` → `CONNECTOR-SDK-SPEC.md` → `CONTROL-CONTENT-PROTOCOL.md` |
| Recovering a machine | `RECOVERY-PROTOCOL.md` → `machine.declaration` Skill → `CONTROL-CONTENT-PROTOCOL.md` |
| Accepting the product | `PRODUCT-ACCEPTANCE-MATRIX.md` → `CENTRAL-SYSTEM-SPEC.md` |

The root [README](../README.md) carries product meaning and the compact
dependency rule; this index carries the corpus.
- [NATIVE-FILESYSTEM-READING.md](NATIVE-FILESYSTEM-READING.md) — owner-resolved directory and text readings over the actual Central tree.

- [Native ordinary-file recovery](NATIVE-ORDINARY-FILE-RECOVERY.md): filesystem CAS, history, authority refusal and interruption recovery.

- [Source Return](SOURCE-RETURN.md): native proposal/CAS acceptance, provenance, and explicit human-authority availability.

- [Chosen root recognition](ROOT-RECOGNITION.md): bounded metadata-only recognition before explicit desktop binding.
