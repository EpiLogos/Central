# Persistent file maps and the AIKit consumer

Central #154 / Central #159 / AIKit #287.

Central uses bkmr as a durable searchable file map at the root and at each
ProjectCentral. The root searches across participating maps; each Project can
operate independently. Existing source relations keep the source identity,
current location and managed-link relationships. AIKit queries these native
operations for knowledge and revisioned skill-source trees, then owns its
contextual selection and target materialisation.

## Native contract

Use the existing native Action surface. All sixteen operations below return the
normal ActionResult envelope. Its `data` is:

```json
{"schema":"central.file-map/v1","operation":"inspect","result":{},
 "automatic_agent_or_model_invocation":false}
```

```sh
ctrl --json --root "$CENTRAL_ROOT" action run central.file-map.inspect '{}'
ctrl --json --root "$CENTRAL_ROOT" action run central.file-map.refresh '{}'
ctrl --json --root "$CENTRAL_ROOT" action run central.file-map.refresh '{"project":"alpha"}'
ctrl --json --root "$CENTRAL_ROOT" action run central.file-map.search '{"query":"quartz","federated":true}'
```

Omit `project` for root, or provide the registered Project name/qualified World
ref. On an independent Project root the selected map is that Project. `scope-register`
adds an existing independent Project to root federation without moving it;
absolute paths require `allow_external:true`. `inspect` reports the participating
scope paths/Worlds, selected resources, source-relations `revision`, managed links
and actual provider availability. With `federated:true`, its provider reading and
resources cover the participating maps, even when root itself has no database.
A Project reading includes its local map and specifically linked foreign sources,
not the rest of the foreign Project. Native World exclusions further narrow it.

`register` takes `path`, optional `native_import`, `title`, `tags`, and the current
`expected_revision` from inspect (`absent` for an absent relation document).
Absolute external resources require `allow_external:true`. Files are retained in
place, without frontmatter rewriting. Root encounters of existing Project sources
return their existing SourceRef. `resolve` takes that `source_ref`, optional
`expected_revision` and `content:true`; `content_encoding:"base64"` reads bounded
binary material. `locate` resolves a registered location or verified managed link.
Neither operation searches by guessed basename or rewrites a SourceRef.

`link` takes `source_ref`, a destination `path`, `owner`, and current ground
`expected_revision`. It creates a relative symlink exclusively and retains its
inode/target binding. Repeated identical requests are idempotent; foreign files,
foreign replacement links and traversal cycles are refused. No general symlink
following is enabled in other Central filesystem readers.

## Persistence and native bkmr use

Each scope keeps its database at `.central/bkmr/index.db` and derived bindings at
`.central/bkmr/bindings.json`. Ordinary source relations remain outside that
cache, in `Control/relations/source-relations.json` or the Project equivalent.
The additive `file_map` member carries selected resources, links and scope
bindings while preserving the rest of the existing document.

`refresh` creates an absent map or incrementally updates its own native records.
It never drops an existing database. Unknown bookmarks remain untouched. Edited
titles/tags survive refresh; a changed managed description requires explicit
reconciliation rather than being overwritten. Identical file content does not
merge distinct SourceRefs. A missing or excluded source withdraws only owned rows.

Ordinary files/directories use native file-URI bookmarks, with bounded text indexed
in their descriptions. `native_import:true` additionally uses bkmr's tracked
`import-files --update --base-path WORLD` for compatible metadata-bearing `.md`,
`.sh` and `.py` sources. URI records preserve identity when native content
uniqueness prevents an additional tracked row; that outcome is reported.
The scope config and HOME are isolated because bkmr 7.6.7's importer reloads
settings internally. Both reads therefore resolve the same `WORLD` base path.

The supported baseline is real bkmr 7.6.7. Full-text and tags require no embeddings.
`refresh` with `embeddings:true` explicitly prepares embedding-backed records;
`search` with `mode:"hybrid"` uses native `hsearch --json` only for prepared maps.
Pure `sem-search` has no equivalent JSON contract and is not advertised here.
Search/read never dispatch native scripts or openers. Existing unregistered
bookmarks remain usable through native bkmr; they are not silently adopted as
Central-authored source or assigned invented provenance.

Source payloads, native World exclusions and `.no-agent-retrieval` are checked
live. Stale indexed revisions are withheld until refreshed. World declarations
that cannot be read are errors, not permission to assume broader root access.
These are routed owner-operation checks, not OS confinement or caller identity
attestation. Existing AIKit/Actuation/Workcell authority and material boundaries
remain necessary where arbitrary same-user filesystem access must be restricted.

## Moves, replay and rollback

`move-plan` takes a `source_ref` and scope-relative `destination`; it records a
recovery journal and stages owned link replacements, without moving source bytes.
`move-apply` and `move-rollback` take the returned `plan_id` and `quiesced:true`.
The caller must have stopped affected sessions; the operation refuses its own
live cwd but does not claim to discover every external process. Destinations
are never overwritten. Source identity, source-relations bases and managed link
inodes are rechecked. Interrupted publication can be replayed; newer source bytes
or foreign link replacement cause conflicts rather than destructive repair.
The native Flow owner prepares current-location updates in the same journal;
Flow identity and historical source paths/receipts remain intact.

A containing root can relocate with its ordinary files. Reconnect with the new
root path and refresh the maps; SourceRefs and relative links are unchanged.
Whole-Project relocation through the owner also rebinds participating maps and
repairs registered cross-scope links. Git indexes and dirty/untracked source bytes
are not reset. This is the file-map relocation operation, not a replacement for
CAW's Day/NOW lifecycle, migration policy, receiving or document ownership.

## Existing databases and authored records

`adopt-db` takes `database`, current ground `expected_revision` and
`quiesced:true`. SQLite's online backup captures committed WAL as well as the main
file. Central retains `.central/bkmr/adopted-original.db`, records an adoption
receipt and creates a distinct working database inode. Original records remain
unmodified; adoption never overwrites an existing map or retained backup.

To associate an existing native URI bookmark with a registered source, inspect
its exact `record_id`. `result.native_record` returns the record and a canonical
revision (tag-set order is normalized). Pass that `record_revision`, `record_id`,
`source_ref` and current ground `expected_revision` to `record-adopt`. The URL
must match the source location. Authored title, tags and description are retained.
A general JSON import is not an upsert and is not used as one.

## Skills and projections

`skill-tree` takes a registered directory `source_ref` and returns the complete
bounded tree: `files` with relative paths, SourceRefs, exact revisions,
`content_base64` and executable modes; native manifests; and `tree_revision`.
Directories, member bytes, modes, source bindings and current disclosure are
checked before returning. Symlinks, duplicate/special/multiply-linked members and
private subtrees are not copied around. Central owns source standing and native
skill placement; AIKit consumes the returned bytes.

```sh
aikit source bind-central root-skills '<SourceRef>' --root "$CENTRAL_ROOT"
aikit source sync root-skills
aikit source promote root-skills
```

Promotion makes a snapshot available to AIKit's catalogue, not automatically
selected in a generation. AIKit's normal enable/profile/Skill Set selection and
`apply` remain operative. A changed or retired owner source invalidates a stale
snapshot before reuse. `CENTRAL_ROOT` supplies the new connection root after a
move; the retained SourceRef supplies identity.

`projection-record` records an exact immutable file snapshot or a reported AIKit
generation. A generation report names its actual directory, generation ref,
selected capsules and owner tree revision. Available-but-unselected skills are
not reported as projected. `projection-read` returns `records` with live
`source_current` readings; it never claims the harness loaded the projection.
After-commit receipt failures are explicit reconciliation warnings, not fake
rollback of an already committed generation.

## Executable evidence

`tests/bkmr_joined.py` exercises actual source-built ctrl and aikit plus real bkmr
in disposable Worlds. No substitute implements a Central operation. Missing
binaries are failures, not skipped green tests. Run:

```sh
python3 tests/bkmr_joined.py --ctrl /built/ctrl --aikit /built/aikit --bkmr /installed/bkmr
```

The suite covers persistence, two-Project federation, owner database read-only
consumption, native imports, metadata preservation, private/stale source refusal,
managed links, interrupted/replayed/rolled-back moves, whole-Project/root moves,
Flow continuity, live skill companions/retirement, selected generation reporting,
explicit record adoption and WAL-consistent backup. Hosted verification records
both exact source revisions. Personal installation, real embedding inference and
loaded commercial harness behaviour remain distinct observations.
