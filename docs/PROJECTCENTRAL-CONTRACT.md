# ProjectCentral filesystem, authorship, and Wiki identity contract

**Status:** current design contract for Central #66  
**Owner:** Central  
**Downstream consumers:** AIKit Knowledge Navigation, Software Factory, O:I local-world projection  
**Wiki grammar:** `okf-wiki/v1` (owned by the interoperable Wiki/AIKit layer, not redefined here)

## 1. Why this contract exists

Central already owns the durable filesystem relation among a person's Control material, ordinary Work projects, resources, and archives. AIKit already owns the operational cognition of knowledge: SemanticWiki, WikiSpace/WikiNode/WikiEdge, ProjectMap, retrieval, routing, framing, explanation, and history.

The missing relation is therefore not another Wiki engine. It is an obvious, durable source relation inside the filesystem:

```text
human-authored project ground
        ↕ provenance / return
Agent-authored or Agent-maintained Wiki knowledge
```

Central owns where that relation lives and how its authority is read. It does not redefine the Wiki graph.

## 2. The project-local convention

An ordinary project in `Work/` remains an ordinary native project. ProjectCentral is a visible orientation layer **inside** it; it is not a wrapper around the project and adoption does not imply moving existing files.

```text
Work/<project>/
├── ProjectCentral/
│   ├── README.md
│   ├── project.json
│   └── Wiki/
│       └── wiki.json
└── <ordinary native project files and directories>
```

The names above are canonical for the Central-owned relation:

- `ProjectCentral/` — the project-local Central aperture;
- `ProjectCentral/README.md` — the human-authored aperture and index;
- `ProjectCentral/project.json` — deterministic binding metadata owned by the Central application contract;
- `ProjectCentral/Wiki/` — durable Agent Wiki source;
- `ProjectCentral/Wiki/wiki.json` — portable `okf-wiki/v1` Wiki objects when Central materialises a native Wiki there.

`ProjectCentral/README.md` may point to existing native `README`, `docs/`, design material, ADRs, Obsidian material, or other authored sources. Those files do not become Central-owned merely because they are referenced from the aperture.

## 3. Root Wiki federation

The enclosing Central world gains one additional visible top-level relation:

```text
Central/
├── Control/
├── Work/
├── Resources/
├── Archive/
└── Wiki/
    └── wiki.json
```

`Wiki/wiki.json` is the durable root WikiSpace/federation source. It may declare child Project WikiSpaces and root/personal nodes, but it **must not copy every child Project Wiki into a universal Central database**.

A Project WikiSpace may name the Central root WikiSpace as a parent while remaining independently loadable. Root traversal resolves child Project WikiSpaces from their actual project-local sources.

The filesystem tree and semantic web therefore remain distinct but interoperable:

```text
filesystem ownership / location       semantic Wiki relations
Central -----------------------+      root WikiSpace
  Work/project-a/              |          |
    ProjectCentral/Wiki/ ------+----------+--> child WikiSpace
  Work/project-b/              |          |
    ProjectCentral/Wiki/ ------+----------+--> child WikiSpace
```

## 4. Authority and authorship

The filesystem path alone does not make every file equally authoritative. Central preserves four source roles explicitly.

| Source role | Canonical location / relation | Authority rule |
|---|---|---|
| **human-authored** | `Control/**`, `ProjectCentral/README.md`, and native project sources referenced by it | Humans author intent, purpose, design judgement, governance, and other source material. Agents may propose changes but do not silently rewrite it. |
| **Agent-authored / Agent-maintained** | `ProjectCentral/Wiki/**`, root `Wiki/**` where an Agent is maintaining knowledge | Durable semantic knowledge ABOUT/ACROSS source material. Every consequential node/edge must retain provenance to source/evidence and its epistemic standing. |
| **observed** | represented in Wiki objects or evidence references | Claims tied to an observation/evidence source; observation is not silently promoted to authored intent. |
| **inferred / derived** | represented in Wiki objects with provenance, or `.central/derived/**` for rebuildable operational indexes | Inference remains distinguishable from observation and authorship. `.central/derived/**` is non-authoritative and may be rebuilt. |

This preserves the existing meaning of `Control/agents`: it remains **human-authored recurring Agent-governance material**. Agent-authored knowledge does not move there.

## 5. `project.json`: binding metadata, not a Wiki schema

`ProjectCentral/project.json` exists so Central can deterministically identify the ProjectCentral relation without parsing prose or inventing semantic-Wiki identity.

Version 1 has this minimal shape:

```json
{
  "schema": "central.project/v1",
  "project_id": "<stable project identity>",
  "wiki": {
    "profile": "okf-wiki/v1",
    "source": "Wiki/wiki.json"
  },
  "human_aperture": "README.md"
}
```

Rules:

1. `project_id` is a stable Project identity, not a repository URL and not a database row id.
2. Relative paths resolve from `ProjectCentral/` and must not escape it.
3. `wiki.profile` names the accepted interoperable Wiki profile. Central does not define its node/edge schema.
4. `wiki.source` may later bind an adopted in-place Wiki outside `ProjectCentral/Wiki/` only through an explicit adoption result that records the binding and provenance; Central must not guess among multiple candidates.
5. The manifest does not declare that human source has been migrated into the Agent Wiki.

## 6. Native Wiki identity

When Central creates a Project Wiki, it writes ordinary portable `okf-wiki/v1` objects. Canonical Wiki identity comes from the Wiki objects (`WikiSpace`, `WikiNode`, `WikiEdge` and their resource refs), not from provider/database identity and not from the filesystem path alone.

Central owns:

- the durable filesystem source location;
- ProjectCentral/project identity binding;
- adoption/migration provenance;
- root federation source location.

AIKit owns:

- parsing/indexing the accepted Wiki grammar;
- SemanticWiki / ProjectMap operational views;
- bounded traversal and retrieval;
- readings/routes/frames;
- stale/conflict detection and Agent Wiki maintenance procedure.

## 7. Human project aperture

`ProjectCentral/README.md` is intentionally small and hand-editable. It is the first authored opening an Agent should be able to encounter before traversing accumulated Wiki knowledge or exact source.

It should make it natural to point to or state, where relevant:

- what the Project is;
- why it exists;
- intended human experience;
- vision;
- design / interaction / visual direction;
- conceptual positions;
- important current human judgements;
- canonical native source locations.

It is an aperture, not a required duplicate of every project document.

## 8. Adoption is not migration

Central #67 must expose materially different outcomes:

```text
already conformant
bind/adopt existing Wiki in place
create ProjectCentral around native project
migrate selected material
unresolved / human Decision required
```

An existing Markdown/Obsidian/docs/design/Wiki/OKF/generated-Wiki structure is first inspected as source. A unique compatible Wiki may be bound in place. Multiple plausible authoritative candidates are ambiguity, not permission to choose. Moving or rewriting source requires an explicit migration plan and preview.

## 9. Source return law

The normal Agent entry path is:

```text
human-authored Project aperture
        ↓
Agent-maintained SemanticWiki
        ↓
bounded traversal
        ↓
exact design / code / evidence / source
```

Returned knowledge may update the Agent Wiki with provenance. If returned reality challenges human-authored ground, the result is a proposal / Decision pressure against that source; it is **not** permission to silently rewrite human intent.

## 10. Projection boundary

ProjectCentral makes material locally addressable; it does not make it public.

These states remain distinct:

```text
exists locally
Agent-readable
selected for local presentation
selected for Projection
public / hosted
```

O:I consumes this distinction. Hosted state is never made canonical by this contract.
