---
name: projectcentral-ontology
description: The ProjectCentral fractal — the one shape Central repeats at the root and at every project. Teaches the canonical directories, their sources, and the ctrl commands that instantiate and navigate real filesystem trees. Use when reading, initialising, inspecting, or reprojecting any ProjectCentral, or when indexing the world.
---

# ProjectCentral ontology

## The fractal

Central organises a world by repeating one shape — at the root of the install and again inside every project. The same three-part distinction holds at both registers: **human source**, **agent material**, and the **wiki** that carries agent-generated cognition relative to the human source. Both live; the wiki is not a reflection of the human side, it is the agent's living cognition around it.

```text
Root register (the install)                 Project register (Work/<Name>)
────────────────────────────                ─────────────────────────────────
Control/                    root ground     Work/<Name>/ProjectCentral/
  user/                     human source      project.json                 manifest
  agents/                   agent material    user/                        human source
    governance/             agent rules       agents/
    wiki/wiki.json          root wiki           governance/                agent rules
    expressions/            skill bots          wiki/wiki.json             project wiki (okf-wiki/v1)
  machines/                 machine roles     now/                         opt-in living field
  relations/                ground relations    user/  agents/  day/
Work/                       projects            policy.json  promotions.json
                                              relations/source-relations.json
                                              .central/flows.json           flow registry
                                              .central/flow-revisions/      flow history
```

## The directories and what they carry

| Path | What it is |
|---|---|
| `Control/user` | Human-authored self-description, interests, preferences, decision criteria. Authored, never generated into. |
| `Control/agents/governance` | Durable human-agent relationship rules. |
| `Control/agents/wiki/wiki.json` | The root wiki — one `okf-wiki/v1` WikiSpace. |
| `Control/agents/expressions` | Skill bots (expressions). |
| `Control/machines` | Portable machine roles and intended environment state. |
| `Control/relations/source-relations.json` | Declared source relations overriding the tree fallback. |
| `Work/<Name>/ProjectCentral/project.json` | Manifest (`central.project/v1`). |
| `ProjectCentral/user` | Human source of the project. May be empty; a single recognised note establishes ground. |
`ProjectCentral/agents/governance` | Agent governance at project scope. |
| `ProjectCentral/agents/wiki/wiki.json` | The project wiki — `okf-wiki/spaces/:project-wiki` ref. |
| `ProjectCentral/now/` | The opt-in living field: `user/`, `agents/`, `day/`, `policy.json`, `promotions.json`. |
| `ProjectCentral/relations/source-relations.json` | Project-scoped declared source relations (`central.project.ground-relations/v1`). |
| `.central/flows.json` | Stable Flow identities and revisions. |
| `.central/flow-revisions/` | Content revisions behind each Flow. |

## What each fault means

`central.world` reports faults as data, never repairs them. A **mixed root** (install root is also a source checkout) is data. A **dangling child ref** in a wiki space is data. A **partial ProjectCentral** names its missing canonical pieces by ground-relative path. **Unresolved provenance** names the source. The map never moves, renames, or fixes anything; reprojection stamps only missing canonical scaffolding.

## Instantiation

Initialise for an existing project:

```text
central action run projectcentral.init                    # the canonical fractal, nothing imposed on user/
central action run projectcentral.adopt.preview           # wiki adoption in place
central action run projectcentral.adopt                   # adopt a selected wiki
central action run projectcentral.migrate.preview         # selected wiki migration preview
central action run projectcentral.migrate                     # migrate a selected wiki explicitly
```

The init creates the fractal without imposing a human document into `user/`. Adoption keeps an existing wiki in place while creating the canonical agent wiki. A human document in `user/` is authored by a human, never by location or generation.

## Reprojection

When a project's ProjectCentral is partial, reproject additively:

```text
central world plan <project>      # what is missing, what would be stamped, what would never happen
central world apply <project>     # stamp ONLY missing canonical scaffolding
```

The plan and receipt state what reprojection would never do: move, rename, delete, relabel, write into anything that exists, or author content. Litter and out-of-place files stay exactly where they are and are classified honestly by provenance in the plan. This works relative to any system doing this — no file damage, honest reporting, clean slate for agent-user engagement.

## Navigation

```text
central world                     # the whole world as one readable index
central world <project>           # the world projected at one ProjectCentral
central doctor                    # required structure only
central action list               # what actions exist
central action run <action>       # canonical operations, real authority result
central control.search <term>     # authored Control source
```

The map is read-only. It reads no source content beyond small wiki, relations, flow-registry and policy JSON objects. Every fault is data about the world, not a repair or a prompt.

## Boundary

This skill teaches the shape. It does not author content into any of these directories — durable authored material is human-accepted via [control-maintenance](../control-maintenance/SKILL.md); wiki writes go through aikit wiki commands (see the aikit wiki-write base); the NOW lifecycle (`now.init`, `now.return`, `now.update`, `now.promote`, `now.rollover`) runs through canonical actions under `projectcentral.now.*`.

## Opt-in product-ground collection

When the human explicitly chooses the sixfold documentation convention, route composition through [docs-methodology](../docs-methodology/SKILL.md#opt-in-sixfold-product-ground-authoring). For the Central pilot, `ProjectCentral/user/central.html` is the primary editable source collection, accompanied by matrix MD/CSV and a view manifest. Its six recursive composition coordinates organise typed, addressable document units; the thirteen vessel types and claim standing are separate dimensions. The HTML opens with a canonical 0/1 overview containing the human’s six-question seed; #0–#5 expand the corresponding answers. #0 carries the needs-led vision. The companion matrix contains addressable functional capabilities and declared relational views using the same matrix protocol; its proposed #0 vessel classification is separate from the opening narrative and filesystem structure.

This arrangement uses Central's existing source aperture and relation metadata. It does not change init/reprojection, impose documents on other Projects, or establish authorship by path. User-authorized generated consolidation can occupy the pilot path as an explicitly generated `draft-for-review`; actual human adoption remains separate. Distinguish that HTML source from any derived preview/export. Preserve stable unit identities, `[[resource|label]]` references, stated relation meanings and tags so later disclosure can traverse the authored relations without replacing their sources.

Run `python3 tools/check_product_ground.py` from the Central repository root when validating this pilot. Central owns the filesystem structure and durable source identity for document retirement; AIKit owns assessment, consolidation, retirement, archive and restore operations. Selected matrix carriers conform in place; broader retirement requires successor accounting under the [matrix protocol](../../docs/CAPABILITY-MATRIX-PROTOCOL.md). The shared primitive definitions remain with the O:I and native-product sources referenced by docs-methodology; this collection relates Central to them rather than creating another ontology.
