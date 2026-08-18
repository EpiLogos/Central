# ProjectCentral filesystem, authorship, and Wiki identity contract

**Status:** current design contract for Central #66  
**Owner:** Central  
**Downstream consumers:** AIKit Knowledge Navigation, Software Factory, O:I local-world projection  
**Wiki grammar:** `okf-wiki/v1` (owned by the interoperable Wiki/AIKit layer, not redefined here)

## 1. Why this contract exists

Central is the durable filesystem world in which human source and Agent-maintained knowledge can coexist without becoming the same authority.

The missing relation is deliberately simple:

```text
human-authored source
        ↕ provenance / return
Agent-maintained Wiki knowledge
```

The same relation must be recognisable at the enclosing Central scope and at every Project scope. ProjectCentral is therefore not a special aperture document or a second project format. It is a **fractal of the Control authorship relation** placed inside an otherwise ordinary Project.

Central owns the durable filesystem/source relation. AIKit owns the operational cognition of that relation: traversal, retrieval, framing, SemanticWiki indexing, readings, routes, maintenance and source return.

## 2. The recursive filesystem law

At the Central root:

```text
Central/
├── Control/
│   ├── user/
│   │   └── <human-authored material, freely structured>
│   ├── agents/
│   │   ├── governance/
│   │   │   └── <human-authored recurring Agent governance>
│   │   └── wiki/
│   │       └── wiki.json
│   └── machines/
└── Work/
```

At Project scope:

```text
Work/<project>/
├── ProjectCentral/
│   ├── user/
│   │   └── <human-authored Project material, freely structured>
│   ├── agents/
│   │   ├── governance/
│   │   │   └── <optional human-authored Project-local Agent governance>
│   │   └── wiki/
│   │       └── wiki.json
│   └── project.json
└── <ordinary native Project files and directories>
```

The important invariant is not a required file inside `user/`. It is the relation:

```text
user/**  →  agents/wiki/**  →  exact source/evidence as required
```

The human authorship space is intentionally unconstrained below `user/`. A person may write prose, maintain design folders, copy research material, keep diagrams, structured data, images, links, notebooks, or any other ordinary files that carry Project meaning. No README, front page, universal schema or document taxonomy is required.

The Agent Wiki is knowledge about/across those human sources and the wider native Project. It does not become those sources.

## 3. Why `agents/governance` and `agents/wiki` are distinct

Before this contract, `Control/agents/` meant human-authored recurring governance for software agents. That meaning remains real and valuable.

The new Wiki world also needs a durable Agent-authored/Agent-maintained region. Those two authorities must not be confused merely because both concern Agents.

The canonical split is therefore:

```text
agents/
├── governance/   # human-authored: how Agents should relate/behave
└── wiki/         # Agent-maintained: what is known about the world
```

This applies recursively at Central and Project scope.

Existing pre-split material directly under `Control/agents/` remains human-governed source by provenance; Central must not silently reinterpret old human-authored files as Agent-authored Wiki knowledge. Migration into `governance/` may be offered explicitly, but is not required to preserve meaning.

## 4. Human authorship is a space, not an aperture file

`ProjectCentral/user/` is the authored Project ground.

Useful material can include, without prescription:

- what the Project is and why it exists;
- intended experience;
- vision and conceptual positions;
- design, interaction and visual direction;
- important judgements and decisions;
- research material;
- source references;
- data, media or documents in formats meaningful to the human.

A human may structure that space however they wish. AIKit skills should understand the **role of the directory**, not require a particular filename.

Native Project files outside ProjectCentral remain ordinary native sources too. The Project Wiki may point into code, tests, docs, design, evidence, Runs, external research or other WikiSpaces whenever required.

## 5. Privacy and retrieval

Filesystem existence is not the same as Agent readability.

The existing `.no-agent-retrieval` convention remains the portable exclusion marker. It applies recursively to ProjectCentral human source as well as Control source when the consumer is traversing ProjectCentral.

For example:

```text
ProjectCentral/user/
├── public-to-agents/
│   └── vision.md
└── private/
    ├── .no-agent-retrieval
    └── notes.md
```

`private/` is not a magic name. The marker is what establishes the exclusion. This keeps the human free to choose their own structure while giving AIKit a simple auditable traversal rule.

## 6. `project.json`: identity/binding metadata only

`ProjectCentral/project.json` is not the human front page and not a Wiki schema. It exists only so Central and downstream consumers can identify the Project relation deterministically.

Version 1 has the canonical shape:

```json
{
  "schema": "central.project/v1",
  "project_id": "<stable project identity>",
  "human_source": "ProjectCentral/user",
  "wiki": {
    "profile": "okf-wiki/v1",
    "source": "ProjectCentral/agents/wiki/wiki.json"
  }
}
```

When a compatible existing Wiki is adopted in place, Central may additionally record its project-relative source under `wiki.adopted_sources`. The canonical Project Wiki source itself remains nested at `ProjectCentral/agents/wiki/wiki.json`; adoption must not erase the recursive filesystem law.

Rules:

1. `project_id` is stable Project identity, not a database row id.
2. `human_source` is the canonical ProjectCentral human authorship root.
3. `wiki.source` is the canonical ProjectCentral Agent Wiki root.
4. adopted source paths remain project-root-relative and may not escape the Project.
5. the manifest does not make human source Agent-authored, and does not make an adopted/generated Wiki human-authored.

## 7. Root/personal Wiki federation

The Central root Wiki belongs inside the same Agent knowledge region:

```text
Control/agents/wiki/wiki.json
```

It is the durable root/personal WikiSpace and federation source. It may relate knowledge compiled from `Control/user/**`, Work Project descriptors, and child Project WikiSpaces, while retaining source provenance.

It must not copy every Project Wiki into a universal Central database.

The filesystem tree and semantic web remain distinct but interoperable:

```text
Central filesystem                              Semantic Wiki

Control/user/** -----------------------------→ root/personal WikiSpace
Control/agents/wiki/wiki.json ----------------→ root/personal Wiki source
                                                   |
Work/project-a/ProjectCentral/agents/wiki/ --------+→ child WikiSpace
Work/project-b/ProjectCentral/agents/wiki/ --------+→ child WikiSpace
```

## 8. Authority and provenance

The filesystem relation preserves distinct source roles:

| Source role | Canonical relation | Authority rule |
|---|---|---|
| **human-authored** | `Control/user/**`, `*/agents/governance/**`, `ProjectCentral/user/**`, and ordinary native Project source | Human source and judgement. Agents may propose revision but do not silently rewrite it. |
| **Agent-authored / Agent-maintained** | `Control/agents/wiki/**`, `ProjectCentral/agents/wiki/**` | Durable semantic knowledge ABOUT/ACROSS sources. Consequential knowledge retains provenance and epistemic standing. |
| **observed** | evidence/source refs represented through Wiki knowledge or native evidence | Observation is not silently promoted to authored intent. |
| **inferred / derived** | Wiki knowledge with explicit standing, or `.central/derived/**` for rebuildable operational state | Inference remains distinguishable from observation/authorship; rebuildable indexes are non-authoritative. |

`agents/governance/**` is human-authored even though it is stored beneath `agents/`. Directory names do not override provenance.

## 9. Wiki interoperability

Central does not invent a semantic graph format. The Agent Wiki uses the accepted portable `okf-wiki/v1` grammar and stable Wiki concepts such as:

```text
WikiSpace
WikiNode
WikiEdge
```

Central owns:

- filesystem/source identity;
- ProjectCentral identity and canonical roots;
- adoption/migration provenance;
- root federation source location;
- privacy/exclusion treatment at the filesystem boundary.

AIKit owns:

- SemanticWiki parsing/indexing;
- ContextSource binding;
- ProjectMap / KnowledgeApplication views;
- LIST / TREE / GRAPH traversal;
- bounded search/read/relations/route/frame/sources/explain/history;
- stale/conflict detection;
- Agent Wiki maintenance and source-return procedure.

## 10. Adoption is not migration

Central #67 must retain distinct outcomes:

```text
already conformant
adopt/bind compatible existing Wiki source in place
create ProjectCentral around existing native sources
migrate selected Wiki/source material into the canonical agents/wiki region
unresolved / human Decision required
```

Adoption can preserve an existing Wiki file in place while recording it as a participating source of the canonical local Agent Wiki. Migration deliberately moves/copies selected material into `ProjectCentral/agents/wiki/**` with provenance. Neither operation rewrites human-authored source merely because it is useful to the Wiki.

## 11. Source-return law

The normal cognitive path is now structurally obvious at either scale:

```text
human source space
        ↓
Agent-maintained Wiki
        ↓
bounded traversal
        ↓
exact source / code / evidence / observation
```

Returned knowledge may revise the Agent Wiki with provenance.

If returned reality challenges human-authored source, the output is a proposal or Decision pressure against that source. It is not permission to silently mutate the human side.

## 12. Projection boundary

ProjectCentral makes material locally addressable; it does not make it public.

These remain distinct:

```text
exists locally
Agent-readable
selected for local presentation
selected for Projection
public / hosted
```

O:I consumes this distinction. Hosted state is never canonical merely because it is projected.
