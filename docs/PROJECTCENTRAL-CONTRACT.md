# ProjectCentral filesystem, authorship, and Wiki identity contract

**Status:** current Central ProjectCentral contract  
**Owner:** Central  
**Downstream consumers:** AIKit Knowledge Navigation, Software Factory, O:I local-world projection  
**Wiki grammar:** `okf-wiki/v1` (owned by the interoperable Wiki/AIKit layer, not redefined here)

## 1. Why this contract exists

Central is the durable filesystem world in which human source and Agent-maintained knowledge can coexist without becoming the same authority.

The relation is deliberately simple:

```text
human-authored / human-adopted source
        ↕ provenance / return
Agent-maintained Wiki knowledge
```

The same relation must be recognisable at the enclosing Central scope and at every Project scope. ProjectCentral is therefore not a special aperture document or a second project format. It is a **fractal of the Control authorship relation** placed inside an otherwise ordinary Project.

Central owns durable filesystem/source identity and accepted source relations. AIKit owns operational cognition of that relation: traversal, retrieval, framing, SemanticWiki indexing, readings, routes, maintenance and source return.

## 2. The recursive filesystem law

At the Central root:

```text
Central/
├── Control/
│   ├── user/
│   │   └── <human-owned personal authorship material, freely structured>
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
│   │   └── <human-owned Project authorship aperture, freely structured>
│   ├── agents/
│   │   ├── governance/
│   │   │   └── <optional human-authored Project-local Agent governance>
│   │   └── wiki/
│   │       └── wiki.json
│   ├── relations/
│   │   └── source-relations.json   # optional accepted source/provenance relations
│   └── project.json
└── <ordinary native Project files and directories>
```

The important invariant is not a required file inside `user/`. It is the relation:

```text
human-owned authored aperture / recognised native human source
        ↓
Agent Wiki
        ↓
exact source/evidence as required
```

The human authorship space is intentionally unconstrained below `user/`. A person may write prose, maintain design folders, keep diagrams, structured data, images, links, notebooks or any other ordinary files that carry Project meaning. No README, front page, universal schema or document taxonomy is required.

The Agent Wiki is knowledge about/across those human sources and the wider native Project. It does not become those sources.

### Filesystem location and actual authorship

`ProjectCentral/user` is a **human-owned authorship aperture**, but a filesystem path cannot prove who generated particular content.

Therefore:

```text
source lives in ProjectCentral/user
        ≠
Central may infer human authorship from location alone
```

An unclassified file can be reported as `unresolved` until direct human authorship or adoption is recognised. This is particularly important for AI-assisted drafts. A generated suggestion does not acquire human authority merely because an Agent wrote it into the human aperture.

This refinement does not create per-keystroke approval. Once a source is recognised as human-authored/adopted, ordinary direct editing remains ordinary source editing.

## 3. Why `agents/governance` and `agents/wiki` are distinct

Before this contract, `Control/agents/` meant human-authored recurring governance for software agents. That meaning remains real and valuable.

The Wiki world also needs a durable Agent-authored/Agent-maintained region. Those two authorities must not be confused merely because both concern Agents.

The canonical split is:

```text
agents/
├── governance/   # human-authored: how Agents should relate/behave
└── wiki/         # Agent-maintained: what is known about the world
```

This applies recursively at Central and Project scope.

Existing pre-split material directly under `Control/agents/` retains its known provenance. Central must not silently reinterpret old human-authored files as Agent-authored Wiki knowledge. Migration into `governance/` may be offered explicitly, but is not required to preserve meaning.

## 4. Human authorship is a space and relation, not an aperture file

`ProjectCentral/user/` is the preferred authored Project ground aperture.

Useful material can include, without prescription:

- what the Project is and why it exists;
- intended experience;
- vision and conceptual positions;
- design, interaction and visual direction;
- important judgements and decisions;
- recognised changes of direction;
- research material;
- source references;
- data, media or documents in formats meaningful to the human.

A human may structure that space however they wish. AIKit skills should understand the **role of the aperture and accepted source relations**, not require a particular filename.

Native Project files outside ProjectCentral remain ordinary native sources too. A human can explicitly recognise a native file as human-authored Project ground while retaining it in place. The Project Wiki may point into code, tests, docs, design, evidence, Runs, external research or other WikiSpaces whenever required.

The optional `ProjectCentral/relations/source-relations.json` ledger provides machine-readable refs/provenance/standing/roles without copying source. Its schema is `central.project.ground-relations/v1`. It is Central-owned relation metadata, not Project prose and not a second content store.

See [`PROJECTCENTRAL-AUTHORED-GROUND.md`](PROJECTCENTRAL-AUTHORED-GROUND.md) for the executable authored-ground UX.

## 5. Privacy and retrieval

Filesystem existence is not the same as Agent readability.

The existing `.no-agent-retrieval` convention remains the portable exclusion marker. It applies recursively to ProjectCentral human source as well as Control source when the consumer is traversing ProjectCentral.

For example:

```text
ProjectCentral/user/
├── available-to-agents/
│   └── vision.md
└── private/
    ├── .no-agent-retrieval
    └── notes.md
```

`private/` is not a magic name. The marker is what establishes the exclusion. This keeps the human free to choose their own structure while giving Agent-facing traversal a simple auditable rule.

## 6. `project.json`: identity/binding metadata only

`ProjectCentral/project.json` is not the human front page, not the source-relation ledger and not a Wiki schema. It exists only so Central and downstream consumers can identify the Project relation deterministically.

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
2. `human_source` is the canonical ProjectCentral human authorship aperture.
3. `wiki.source` is the canonical ProjectCentral Agent Wiki root.
4. adopted source paths remain project-root-relative and may not escape the Project.
5. the manifest does not make human source Agent-authored, and does not make an adopted/generated Wiki human-authored.
6. source/provenance/standing relations remain outside `project.json` so identity metadata does not become a document taxonomy.

## 7. Root/personal Wiki federation

The Central root Wiki belongs inside the same Agent knowledge region:

```text
Control/agents/wiki/wiki.json
```

It is the durable root/personal WikiSpace and federation source. It may relate knowledge compiled from eligible `Control/user/**`, Work Project descriptors and child Project WikiSpaces while retaining source provenance.

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
| **human-authored / human-adopted** | recognised material in `Control/user/**`, `*/agents/governance/**`, `ProjectCentral/user/**`, or explicitly related native Project source | Human source and judgement. Agents may propose revision but do not silently rewrite it. |
| **human-edited draft** | ordinary source or aperture material whose final adoption is not yet claimed | Human participation is real, but draft standing is not silently promoted to adopted Project position. |
| **generated suggestion / derived** | proposal/derived source or explicitly classified material | Generated content remains generated until human adoption; account/HTML/Projection output is not source merely because it is useful. |
| **Agent-authored / Agent-maintained** | `Control/agents/wiki/**`, `ProjectCentral/agents/wiki/**` | Durable semantic knowledge ABOUT/ACROSS sources. Consequential knowledge retains provenance and epistemic standing. |
| **observed** | evidence/source refs represented through Wiki knowledge or native evidence | Observation is not silently promoted to authored intent. |
| **inferred** | Wiki knowledge or explicit source relation with inference standing | Inference remains distinguishable from observation/authorship. |

`agents/governance/**` is human-authored even though it is stored beneath `agents/`. Directory names do not override provenance.

The source-relation read model also preserves truth standing such as authored human position, design commitment, architecture contract, implementation fact, observed evidence, current development state and Agent inference. These standings are relations of authority for a question; they are not required directories.

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
- accepted source/provenance/standing relations;
- adoption/migration provenance;
- root federation source location;
- privacy/exclusion treatment at the filesystem boundary.

AIKit owns:

- SemanticWiki parsing/indexing;
- ContextSource binding;
- ProjectMap / KnowledgeApplication views;
- LIST / TREE / GRAPH traversal;
- bounded search/read/relations/route/frame/sources/explain/history;
- stale/conflict/drift detection;
- Agent Wiki maintenance and source-return procedure;
- account/document authoring procedure over those sources.

## 10. Adoption is not migration

Central lifecycle retains distinct outcomes:

```text
already conformant
adopt/bind compatible existing Wiki source in place
create ProjectCentral around existing native sources
migrate selected Wiki/source material into the canonical agents/wiki region
unresolved / human Decision required
```

Authored-ground treatment adds a parallel but distinct source choice:

```text
retain useful native human source in place and relate it
use ProjectCentral/user as the authored home going forward
leave material as ordinary Project source
classify as generated/observed/inferred
explicitly reorganise only after review
unresolved / human judgement required
```

Neither Wiki adoption nor authored-ground establishment means “move everything into ProjectCentral”.

## 11. Source-return law

The normal cognitive path is structurally obvious at either scale:

```text
human source space / recognised native human source
        ↓
Agent-maintained Wiki
        ↓
bounded traversal
        ↓
exact source / code / evidence / observation
```

Returned knowledge may revise the Agent Wiki with provenance.

If returned reality challenges human-authored source, the output is a proposal or Decision pressure against that source. It is not permission to silently mutate the human side.

A difference can mean implementation is wrong, design should change, intent should develop, or no authored change is warranted. Difference is not synonymous with failure.

## 12. Projection boundary

ProjectCentral makes material locally addressable; it does not make it public.

These remain distinct:

```text
exists locally
Agent-readable
retrieved for a reading
selected for local presentation
selected for Projection
public / hosted
```

The accepted presentation relation is:

```text
Central / ProjectCentral source
        ↓ selected provenance-aware reading
WorldPresentation
        ↓ explicit ratification
Projection
```

Presentation refinement does not mutate Central source. A difference that should return to human-authored ground follows proposal/review/accepted source revision.

O:I consumes this distinction. Hosted state is never canonical merely because it is projected, and there is no shadow Profile database that outranks the selected projected face of Central.
