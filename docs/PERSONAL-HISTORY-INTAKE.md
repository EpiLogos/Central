# Personal-history intake through the adoption path

Central #242 / continues #76. The narrow source/identity/coverage binding
published early so knowledge (AIKit/QL) and UI (O:I) work can proceed against
real native operations.

## What exists and is reused

Personal-history intake is a supported use of the existing project/world
adoption machinery, not a second migration controller:

- **Source identity** — the `central.file-map.*` family: per-source SourceRefs,
  content revisions, registration, resolve/locate/search, managed links,
  move-plan/apply/rollback. Both root and Project scopes already share it.
- **Source roles** — ground relations (`Control/relations/source-relations.json`
  or the Project equivalent, schema `central.control.ground-relations/v1` /
  `central.project.ground-relations/v1`): `provenance`, `standing`, `roles`,
  `treatment`, written only by an explicitly human-accepted apply.
- **Person anchor** — `central.pasu.identity-manifest/v1` at
  `Control/user/identity/manifest.json`; subject `central:pasu:nara:local`;
  the root ground-relations `subject_ref` is the World binding and is checked
  for agreement.
- **Placement law** — retain in place, register through a source relation, copy
  into a chosen home, or explicitly relocate. A folder is not automatically a
  Project; a personal archive does not masquerade as one.
- **Ordinary-file CAS** — copies and the collection record are ordinary files
  under the same revision/history authority as every other source.

## The collection record

`central.personal-collection/v1` — one ordinary JSON file per collection,
carrying the durable intake facts (dispositions, identity, person binding,
import receipts). In copy mode it lives beside the material at
`Control/user/collections/<collection-id>/collection.json`; in retain mode at
the same path in the world that keeps the relation. It is registered as a
source with role `personal-collection-record`.

Fields (v1):

```json
{
  "schema": "central.personal-collection/v1",
  "collection_id": "field-notes",
  "title": "Field notes",
  "world_ref": "control:root",
  "person_ref": "central:pasu:nara:local",
  "author_ref": "central:pasu:nara:local",
  "retained_by": "ctrl central.personal.collection.apply",
  "placement": {
    "mode": "copy",
    "home": "Control/user/collections/field-notes",
    "origin": "/elsewhere/field-notes",
    "accepted_plan_revision": "central.content-fnv1a64/v1:…"
  },
  "entries": [
    {
      "entry_id": "journal/2026-03-14-first-thaw.md",
      "role": "entry",
      "path": "Control/user/collections/field-notes/journal/2026-03-14-first-thaw.md",
      "source_ref": "central:source:control:root:…",
      "entry_type": "journal",
      "event_date": "2026-03-14",
      "date_basis": "frontmatter date",
      "date_approximate": false,
      "content_revision": "central.content-fnv1a64/v1:…",
      "bytes": 623,
      "disposition": "retained",
      "origin_revision": "central.content-fnv1a64/v1:…",
      "first_import": 1
    }
  ],
  "imports": [
    {
      "sequence": 1,
      "applied_at_unix_seconds": 1790700000,
      "adapter": "markdown-frontmatter@1",
      "entries_added": 6,
      "entries_changed": 0,
      "entries_removed": 0,
      "accepted_plan_revision": "central.content-fnv1a64/v1:…"
    }
  ]
}
```

- `person_ref` is the `about`/subject binding; `author_ref` the narrator.
  Importer (`retained_by`), author and subject may differ. Neither a machine
  path nor any natal quaternion is a person identifier.
- `disposition` ∈ `retained | excluded-by-selection | unreadable |
  unsupported`. Every selected member has exactly one.
- Entry identity is `collection_id + entry_id`; the SourceRef is derived and
  never stored as truth. Re-import at the same content revision is a no-op;
  changed content is a new `content_revision` on the same entry with the
  import sequence recorded; the previous revision stays in ordinary-file
  history.

## Time model

`event_date` (with `date_basis`, `date_approximate`) is the entry's own time;
file mtimes are writing/revision time; import receipts carry import time;
interpretation time belongs to the downstream knowledge stage, never here.
Intake manufactures no Day documents and no current activity.

## Actions

Namespace `central.personal.*`, registered in the root Action registry
(`project` input selects the Project scope where that scope keeps its own
collections; omit for the root register):

| Action | Class | Purpose |
|---|---|---|
| `central.personal.anchor.inspect` | read-only | Person/world/installation/collections readback: pasu manifest state, ground-relations subject agreement, Workcell binding, registered collections. |
| `central.personal.collection.inspect` | read-only | Enumerate a candidate directory through an adapter: entries, types, dates, dispositions (incl. unreadable), bytes/revisions, overlap with existing collections. |
| `central.personal.collection.plan` | read-only | The placement plan for a chosen person and placement mode: per-entry operations, identities, conflicts, undo summary. Pure read model. |
| `central.personal.collection.apply` | locally-mutating | Execute an accepted plan (`acceptance:"human-accepted"` required). Journal-based; interrupted applies resume. Registers per-entry sources via `central.file-map.register` and upserts relations with roles. |
| `central.personal.collection.status` | read-only | Collection record + apply-journal state for resume. |
| `central.personal.collection.verify` | read-only | Readback: bytes vs recorded revisions, SourceRef resolution, bounded content readability. |
| `central.personal.collection.rollback` | locally-mutating | Undo one import's owned effects whose bases still match; later human edits are preserved and reported, not reverted. |

## Placement modes

- `retain-in-place` — the collection stays where it is (inside or outside the
  world); per-entry sources are registered with `allow_external` as needed.
  Useful when the material is already a legitimate Work project: it stays one,
  participates through its ProjectCentral/file-map, and relates to the person.
- `copy` — bytes are copied into `Control/user/collections/<collection-id>/`
  (never overwriting), originals remain untouched at the origin.
- `registered` — a world-relative directory already inside `Control/user` or a
  Project is adopted without copying.

## Processing boundary (for packets B/C)

Intake proves **retained-source completeness** (dispositions, bytes,
readback). Extraction/contemplation coverage — what was considered, what
remains pending — is the AIKit stage's ledger, keyed by the SourceRefs and
content revisions this stage registers; it must never be inferred from
checksums. Derived knowledge cites `source_ref + content_revision (+ span)`;
an unreadable member stays visibly incomplete downstream.
