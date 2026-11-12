---
role: architecture
standing: agent-inference
scope: Central persistent file maps, native Source ownership and AIKit consumption
design_refs: []
updated: 2026-10-02
---
# Persistent file maps and the AIKit consumer

Central #154 / Central #159 / AIKit #287.

Central uses bkmr as a durable searchable file map at the root and at each
ProjectCentral. The root searches across participating maps; each Project can
operate independently. Existing source relations keep the source identity,
current location and managed-link relationships. AIKit queries these native
operations for knowledge and revisioned skill-source trees, then owns its
contextual selection and target materialisation.

## Native contract

Use the existing native Action surface. The operations below return the
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

## The person's own acts

Each act that changes the person's ground, or what the person has adopted, is
theirs: it authenticates on the host's native credential (`CENTRAL_NATIVE_TOKEN`)
for a human grant naming that exact Action, and without one is refused in words,
with nothing written. The grant stands where the act does: at the Work project
an act names, else at the root, and `scope-register` always at the root.

An act also stands where it writes. A place inside a Work project's tree, however
it is spelled, is that project's: an act that writes there names that project and
stands on its grant, and one that names another scope, or none, is refused in
words that say both. A Source the act binds from a Work project needs that
project's grant as well, and so does `adopt-db` of a database inside one. A move
stands at the root, and at each Work project it writes in; see below, where a
move's own rule is stated: it writes in a project only on a plan that project
holds.

One exception stands. A project nested inside a Work project's tree and
registered at the root is a scope of the root's: an act in its own tree stands on
the root's grant. A `link` there makes the link, the relations document and the
lock files inside the Work project's tree on the root's grant alone. Outside such
a nested scope's own tree, a root grant writes into no Work project; for a move
carried out from its journal that rests on
[the plan each project holds](#a-project-writes-only-on-a-plan-it-holds).

A place that holds a Work project's folder, `Work` itself or the ground, reaches
that project's tree too. A write there is refused at the root, and a move of it
needs the grant of every project it holds. Work's projects are found on the disk,
from `Work`'s own entries: a Work project is a folder under `Work`, not a link,
that holds `ProjectCentral/project.json`. A folder under `Work` without one, and a
link under `Work` that leads out of it, stand on the root's grant; so does a Work
folder whose `ProjectCentral` was deleted, which is no longer a project, so a move
into it on the root's grant goes through. Work's projects are never taken from the
scopes registered by name, so a scope registered under a project's name stands in
for none, and `scope-register` refuses that name at another place.

Two shapes are kept apart:

- No declaration: the folder holds no `ProjectCentral/project.json`, or its
  `ProjectCentral` is a file. It is no project, and stands on the root's grant.
- A declaration that is there and does not read: its folder is closed by its
  mode, something on the way is a link to itself, the file has no permissions,
  it is not an ordinary file, or its bytes are no declaration. The folder is
  still a project to the gate, and the map leaves it out as unreachable, in words
  that name it and say what to mend.

While a project is unreachable, an act that does not touch it goes on, and the
readers (`inspect`, `search`, `locate`) answer with it listed. An act on a place
in its tree is refused in the project's words, and so is a `locate` of a place in
it, and so is every move's plan, which has to read each project's links. A project that a move's apply or rollback
reaches or runs from whose declaration does not read is named in the refusal as
well.
A place is judged by the folder it names, its device and inode, whatever its
spelling: `..`, a link, another case, and on macOS the firmlink
`/System/Volumes/Data` spelling of the ground. A folder scope's registration asks
"inside this ground" the same way.

A refusal names the Work project whose grant is missing, by its folder's name, so
the person knows which grant to add. The name is said to whoever holds a human
credential for the act, and a human credential for these acts is only ever the
person's own: this is a stated property of the words, not a leak.

Two limits stand against a writer that runs as the same user, and they need the
operating system's isolation, not a path check:

- A hard link carries a project's file under another name. A root Source
  registered at a hard link is the root's: no act writes through a file path, so
  no file of the project's is written.
- The gate reads the places before the act takes its lock and writes. A writer
  that swaps a folder for a link between the two can defeat that reading. Each
  writer then makes its own check and refuses a link in any folder of the path it
  writes, below its scope's root: `register`, `link`, a move's rename, its
  relations documents and its link steps. A move's scopes are themselves held to
  their folders, by device and inode, before its rename, after it and after each
  link step; a swap between one of those checks and the write after it is the
  same limit.

A move's plan, its apply and its rollback each take the locks of every scope
they run with: `.central/file-map.lock` and `.central/source-mutation.lock`, two
empty files. In a Work project's tree a move makes them, and the `.central`
folder that holds them, only where that project admitted the move: a plan where
the move reaches the project, whose grant the gate then asked; an apply and a
rollback where the project holds the journal's seal. Anywhere else in a Work
project a move takes a lock that already stands and makes none, so it leaves no
entry there. A plan also leaves its seal, `.central/bkmr/moves/<id>.seal.json`,
in each Work project the move writes in.

| Act | What it writes |
| --- | --- |
| `register` | the Source, in the scope's relations document |
| `link` | a link inside the scope's folder, kept in that document |
| `pool` | whether the scope's whole readable tree is Source, in that document; the root's pool stops at each folder holding a project's manifest as an ordinary file; a project's keeps its whole tree; every pool stops at a Work project's folder that is not the scope itself, one whose declaration does not read among them (the map names it as unreachable) |
| `scope-register` | a project scope in Control's relations document, or a folder scope's declaration in the ground |
| `adopt-db` | a copy of the person's bookmark database and its untouched original, under `.central/bkmr` |
| `record-adopt` | a bookmark bound to a Source, its description rewritten with the person's words kept inside |
| `move-plan`, `move-apply`, `move-rollback` | a move's journal, then the move and its links |

`refresh` and `projection-record` write only Central's own index, rebuilt from
what the ground already declares, and read no credential.

## Moves, replay and rollback

`move-plan` takes a `source_ref` and scope-relative `destination`; it records a
recovery journal and stages owned link replacements, without moving source bytes.
The journal, and the plan's answer, name the destination as the disk spells it:
the folders above it in the disk's own case and form, whatever the person typed,
and its own new name as asked.
`move-apply` and `move-rollback` take the returned `plan_id` and `quiesced:true`.
Each move is the person's own act: it authenticates on the host's native
credential (`CENTRAL_NATIVE_TOKEN`) for a human grant naming that exact Action
at the root, and at each Work project it writes in: where its Source stands, where
it goes, and each one whose links it rewrites, with their relations. Without one
it is refused in words that name the project whose grant is missing, with nothing
planned or moved. Apply and rollback read the move from its journal,
and stand on the scopes the journal runs from. So an interrupted move can be
finished or undone while a scope inside the moved folder is still declared where
it stood. One journal is undone and not finished: one that an earlier `ctrl`
planned with its destination's folders typed in another case or form than the
disk keeps. Its relations name that spelling, which the map does not read, so
apply refuses it, from any state, in words that say to roll it back and plan
again, and a rollback goes through. They take nothing else from the journal's list of scopes: the move
stands at the root wherever the list puts it, each Work project its Source or
destination lies in is found on the disk, and a journal whose root scope's path
is not empty, as `move-plan` writes it, is refused. Apply and rollback also stand at each
Work project whose relations or links the journal rewrites.

`Work` itself is not moved, nor a folder that holds it: every project of Work
stands under it, and no act reads a ground whose `Work` is away. The plan of
such a move is refused in words. So is the apply of a journal that names one,
whatever seals it holds, as an earlier `ctrl` could plan and seal it; its
rollback goes on and closes the journal.

On a ground whose `Work` is away, by such a move carried out or by a hand, no
act of the person's is admitted: the authority reads a ground that holds `Work`
before it matches a credential. Each act is refused in the same words for every
caller whose credential is of valid length, granted or not: `Work` does not
stand where the ground keeps it, and a hand puts it back before any press. The
words are made from the ground alone. No journal is read for them, and they
name no place but the ground's own `Work`; where a move took it is in that
move's journal, in its `to`. Once `Work` is back, a move that took it away is
rolled back as any other.

A link the move rewrites is judged by where it stands on the disk, before the
move and after it, and never by what it points at. A link in a Work project's
tree needs that project's grant, whichever scope keeps the link: at the plan, and
at apply and rollback for each link step of the journal.

A link step replaces a link and nothing else. Where a folder stands at a step's
path, the gate judges that folder with all it holds, so a step at a Work
project's own folder, or at `Work`, needs every such project's grant. And apply
refuses, whatever the grants, a step where anything but a symbolic link stands:
at its path before or after the move, at its kept name or at its staged name. It
refuses a step whose path is spelled as a scope's own folder or a folder above
one, in words that say so. A folder or a file is never moved aside as if it were
a link.

Apply and rollback check a journal against what `move-plan` writes, one rule at
a time:

- a relations document only at the move's own scope or at one whose link it
  rewrites;
- each link step at a path contained in its scope, with its staged and kept links
  at their own names in that scope's moves folder;
- each scope spelled as the disk spells the folder it stands at. In the ground
  that is plain names below it, with no link in any of them and each in the
  disk's own case and form, read where the scope stands now (with the moved
  folder, once the Source has moved). Outside the ground it is the folder's own
  absolute path. A scope in the ground that is no project and does not stand is
  refused as well.

A journal that fails one of these is refused at the gate, before anything is
moved or written. A scope inside the moved folder is asked again where it went,
once the Source has moved; the destination is read there as the disk spells it,
so a plan's own journal reads. Apply then makes checks of its own, on its own
read:

- It refuses a link in any folder above a link step's path, its staged link or
  its kept link.
- It holds each scope to the folder it read before the rename, by device and
  inode. If a scope reads as another folder once the Source has moved, or the
  scopes do not read at all, the rename is put back and nothing else is written.
  The journal stays as planned, so the same apply answers the same way and a
  rollback has nothing to undo. A scope spelled through a link is refused by
  the spelling rule before the rename, so "another folder" now stands only
  against a second writer; "do not read" is still reached by a journal that
  moves a scope's own declaration away.
- It holds each scope again after every link step, before the next step and
  before any relations document is written. If one has strayed, the link steps
  this call carried are undone, newest first, the rename this call made is put
  back, and no document is written; the refusal says which step could not be
  undone, if any, and then the Source is left where it stands. With the spelling
  rule above, no journal reaches this check on its own: it stands against a
  second writer that swaps a folder while the move runs.

A put-back can itself fail, when another writer takes the Source's old name in
that moment or the system refuses the rename back. The refusal then says where
the Source stands and that it must be moved back by hand, and the journal's state
says "renamed". While the scopes do not read, an apply of that journal is
refused. A rollback of it makes the rename back and nothing else, and says so in
its answer (`state: "rolling-back"`, with a `note`); a second rollback, now that
the scopes read, undoes whatever else the move had carried. Where the rollback
cannot make the rename back either, its refusal says the same as apply's. This
holds only for a journal whose state says "renamed": one still "prepared" whose
Source another hand moved is refused as before, naming the project that does not
read. A rollback whose own put-back fails leaves the Source where the move took
it from, says so, and the journal reads again once its scopes do.

### A project writes only on a plan it holds

A move writes in a Work project only when that project itself holds a seal of
the very journal being carried out.

- **What is sealed.** Every field of the journal as `move-plan` wrote it but
  `state`, which apply and rollback write as the move goes: the scopes and their
  spellings, the owner, where the Source stands and goes, its identity and
  revision, every link step, every relations document's scope, place and bytes,
  the scopes to refresh, the id and the schema. The seal is a SHA-256 digest of
  that. A sealed journal says `central.file-map-move/v2`.
- **Where it is kept.** `move-plan` writes one small file in each Work project
  the move's places reach: `Work/<project>/.central/bkmr/moves/<id>.seal.json`,
  mode 0600 in a folder 0700, beside the lock files a plan makes there. The
  projects are the ones the gate asks a grant of for that move: one function
  reads which Work projects a place reaches, for the gate over the plan's own
  places and for the sealing over the places the journal then names. So a
  project that needs a grant for a move holds a seal of it, and a plan the
  project's grant did not admit leaves no seal there. A seal already standing
  for the same id is never written over.
- **What apply and rollback ask.** Before every write (the rename, each link
  step's link, kept name and staged name, each relations document) they read
  the folder actually written in, as the disk resolves it at that moment, find
  each Work project it lies in or holds from `Work`'s own entries, and ask that
  project's seal to be the digest of the journal in hand. The seal is read as
  the project's own ordinary file, with no link in the folders above it. Every
  place is also asked once before the rename, so a journal that cannot be
  carried out whole is refused before anything moves.
- **What else stands behind the seal.** The lock folder: an apply or a rollback
  makes `.central` and its two lock files in a Work project's tree only where
  that project holds the journal's seal. And the index: when a move is done it
  refreshes the index of each scope its journal lists to refresh and of each
  scope whose relations it wrote, where one is kept; an index kept in a Work
  project's tree is refreshed only where that project holds the seal. A scope
  passed over is named in the answer (`indexes_not_refreshed`); `refresh` is a
  press of its own and needs no grant.
- **So an edited journal writes in no project.** A journal changed in the
  root's `.central`, in any field but `state`, is no longer the plan a project
  holds:
  "This move's journal is not the plan the Work project alpha holds
  (Work/alpha/.central/bkmr/moves/<id>.seal.json) … plan the move again". This
  holds whatever grants stand. The rule-by-rule checks above remain, and are no
  longer what the boundary rests on.
- **How long a seal lives.** As long as its journal, at every state: a move
  that was rolled back can be applied again from the same journal, and that
  needs the seal. Nothing removes a journal today, so a project keeps one small
  seal file for each move journal that wrote in it, in its
  `.central/bkmr/moves`. Whatever later removes old journals removes their
  seals with them, and nothing else removes one.
- **A journal from before seals** (`central.file-map-move/v1`) is carried out
  where it writes in no Work project, and never leaves the person without a
  press. Its apply is refused where it would write in a project; the refusal
  says where the Source stands and what goes on from there: plan the move again
  where nothing of it stands, roll it back where something does. Its rollback is
  carried out from every state so far as it writes in no project: the Source is
  moved back, and each link and relations document outside Work's projects is
  put back. A step that stands carried in a project is left as it is, and the
  answer names each one for a hand, in order (`left_for_a_hand`, and a `note`):
  which link to remove and which kept link to move back to its place, which
  relations document to write back from the journal's own `before` text. The
  journal then says "rolling-back"; a rollback after the hand's steps finds
  nothing left and closes it. Where the rename back would itself write in a
  project (a Source that stands in one), the rollback is refused before anything
  moves, in words that give the one step a hand makes: move the Source back;
  the rollback then goes on as above.

A journal's scopes are the ones its plan wrote, which are the ones the ground
holds: the root, each project of Work at `Work/<its folder>`, each project
registered at the root at the place the root's relations keep for its name, and
each folder scope declared in `.central/scopes`. A journal that names any other
scope is refused where the gate reads it, before a lock is taken: a folder
scope made up at a folder of a Work project's, say, or a project that stands in
a Work project's tree and was never registered.

A move's journal that names a folder scope or a project outside the ground as
its owner is not carried out, as `move-plan` would not plan it.

What stays outside this rule is what stays outside every path check here: a
second writer running as the same user that swaps a folder between a check and
the write after it, and a hand that can write in the project's own tree, which
needs no journal.

A link whose Source no longer reads stops every plan. The refusal names the link
and the project it stands in, and says to remove the link or restore its Source:
no grant mends it.
`move-plan` is no read: its journal names where the file goes, and apply and
rollback carry that out by the plan's id alone.
The caller must have stopped affected sessions; the operation refuses its own
live cwd but does not claim to discover every external process. Destinations
are never overwritten. Source identity, source-relations bases and managed link
inodes are rechecked. Interrupted publication can be replayed; newer source bytes
or foreign link replacement cause conflicts rather than destructive repair.
The retired Flow registry has no relocation planning. Any leftover registry
stays in place and is refused as a move target. Dated Flow source files use
[ordinary file CAS and native history](../SOURCE-RETURN.md#flow-disclosure-seam-retired);
the [historical Flow contract](../PROJECTCENTRAL-FLOW.md#flow) remains retained.

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

Verification must cover persistence, two-Project federation, owner database
read-only consumption, native imports, metadata preservation, private/stale
source refusal, managed links, interrupted/replayed/rolled-back moves,
whole-Project/root moves, ordinary file CAS/history and retired-registry
retention, live skill companions/retirement, selected generation reporting,
explicit record adoption and WAL-consistent backup. A test of the retired
FlowRef registry does not verify the current ordinary file route. Hosted results must name
both exact source revisions and actual cases executed. Personal installation,
real embedding inference and loaded commercial harness behaviour remain
separate observations.

## Binding ownership metadata — source candidate

This metadata mode is implemented on the qualified Source cuts below. It adds optional
Boolean `binding_only:true` to existing `central.file-map.resolve` and
`central.file-map.locate`, retaining their `central.file-map/v1` envelope and
default Entry/payload shapes. True is invalid with `content:true` or any
`expected_revision` key. Default resolution still computes an admitted payload
revision with `content:false`; suppressing content is not a body-free probe.

Ownership comes from native declarations across participating owners, before
material filtering. Accepted Source roles, provenance and standing take
precedence over registered acceleration metadata. Complete owner collisions
refuse rather than selecting the first scope. A generated address only nominates
a normal member after round trip; a prefix does not establish ownership. Only
matching Skill metadata is read through its existing native parser. A malformed
unselected fallback remains a genuine bulk-horizon failure, while an explicit
selected Source does not depend on that unrelated fallback.

| Native disposition | Native result and consumer consequence |
| --- | --- |
| Eligible owned source | `ownership:"owned"`, `binding_only:true`, exact existing `source`, `world_ref`, `project`, admitted `path`/`kind`, existing title/tags/native-import fields, `relation_revision` and `material_metadata_basis`. Select the intended source family and target privacy before a body read through the same owner. |
| Healthy complete metadata establishes no owner | Success containing only `ownership:"unregistered"`, `binding_only:true` and the exact requested `source_ref` (resolve) or `requested_path` (locate). A consumer may decline. |
| Known missing or withdrawn source | Native coded failure with `ownership:"known"`, actual `failure_stage` and `material_state`; retain the owner failure rather than substituting a copied source. |
| Unsupported, invalid, incomplete, unavailable or changed metadata | Native failure; it cannot certify healthy nonownership or broaden retrieval. |

`relation_revision` is the full declaration-byte FNV revision or literal
`absent` for genuine optional declaration absence. `material_metadata_basis`
contains integer `device`, `inode`, `byte_len`, `mtime_seconds` and
`mtime_nanoseconds`. Owned metadata results omit top-level payload `revision`,
`content`, `content_encoding`, projection and Skill-manifest fields. These
bases prove neither payload freshness nor body permission. A managed-link
locate retains `encountered_link:{path,world_ref}` while keeping the target's
actual SourceRef and owning World. An ordinary physical FileRef read with a
null Source binding does not establish `ownership:"unregistered"`.

A known missing final source is `central.file_map_not_found` with
`failure_stage:"source_metadata"` and `material_state:"missing"`; missing owner
or parent is unavailable. Native marker, World, Project-link or context
exclusion is `central.file_map_denied` with the actual admission stage and
`material_state:"withheld"`. Semantic denial has `io_error:null` and discloses
no private descriptor, member path, title, standing, body or hash. Real IO
preserves its original kind, errno and message; finite native Store capacity
remains separately typed. `effects:"none"` applies to these read-only
resolve/locate failures; mutation failures retain their existing contract.

Project/link/effective-context admission precedes body/hash/projection in
default resolution too. The selected native metadata basis is reobserved after
operation checkpoints. Current World exclusions use the
[native Store's material and complete-list qualification](../NATIVE-FILESYSTEM-READING.md#native-relation-records--source-candidate).
This is observation evidence, not a new registry, read registration, cache
authority, permission lease or atomic snapshot. Optional bkmr absence and
supported reduced composition retain their existing explicit routes.

Central's producer repair does not establish AIKit's current-owner routing or
external-provider egress acceptance. Ordinary NOW Return registration and root
lifecycle also remain separate joins; no broad root Return/T/archive corpus is
admitted by this mode.

Source basis: Central `7a0e2c21505ad7cf6590db8701fc473102fd399b` preimages,
native owner v4 packet SHA-256
`390656e902c866959304094a9b92c46f52434ba04140e17fe10a683bb8f7ca79` and
unchanged response handover
`15ebcf6037f3e0f5cf43b896ff97cdc0a5460d0363d13aa35dec96f35a0568c8`.
Hosted qualification is now recorded by [joined run 37061204754](https://github.com/EpiLogos/Central/actions/runs/37061204754):
all 58 unique selected cases executed and passed, with no skips, in 9.267 seconds
on Ubuntu 24.04.5. The cut was Central
`452525d45fce20c2c667b87ee9590e7f5a3883c6`, AIKit
`338232e5fca8d1df81ac81a37fd9d610278e2e43`, bkmr 7.6.7 and Rust 1.98.1.
Both real owner and consumer binaries were built with `--locked`. Their SHA-256
values were ctrl
`7627827bcbf35dd8c03f241aff9b2a83b01fadd1b39b389fc9176c5b993a7bc8`
and aikit
`82edd6bb5c912dad0e402c70913decfa8155c644233c974dfba7201e50702583`.
The complete joined log has SHA-256
`842e7281ccff711c72b307a94862de52e1cf40c7ff69116e5af8f021e40d86d6`.

[Normal run 37061163439](https://github.com/EpiLogos/Central/actions/runs/37061163439)
also passed Linux workspace and native macOS checks at merge
`cadc4942a5aa64dd1e00216b5203dea2ff7052d7`. Its complete source tree
`4ffb02d0f04684c1b974ebd50a5d014d45043cfe` equals the joined Central tree;
normal checks used Rust 1.99.0. Their ephemeral Linux source-install smoke
omitted `--locked`, resolved 121 highest-compatible dependencies and published
no installed binary SHA-256. That reduced-install proof has a separate dependency
cut from the locked joined binaries.

The original joined case 28 only required a nonzero disconnect exit and parseable
JSON. Its stronger successor now checks a successful current read, the actual
failure envelope, absence of the unique retained body, unchanged source/database
and a current read after reconnection. Those stronger assertions executed in
[joined run 37066338855](https://github.com/EpiLogos/Central/actions/runs/37066338855)
at Central `5eb41c635f2978f5adb34f0c947f774eda498827` with the same
AIKit338 composition: all 58 unique cases passed with zero failures/skips in
9.380 seconds. The full raw log SHA-256 is
`7050ee8dc5c3085555f5c0d5d790d7168ff45716aa539e3fd58c751ed3993a2d`;
the locked ctrl binary is
`b30f6240ca7e047f8d8587323b2d7a6c3646bca88d73e86ffbe9b8579a107aeb`. AIKit338 turns failed known-owner attachment into
`knowledge.source_missing`; this existing loss of the native unavailable-owner
cause is unresolved on that consumer cut. The stronger case does not certify
truthful owner-error propagation.

These receipts qualify the stated hosted source and joined boundaries, not the
personal installed/running World, later Source revisions, human acceptance or
newer prepared AIKit R4 consumers. Preserve the original failed and unqualified
predecessor receipts. See [architecture navigation](../ARCHITECTURE-NAVIGATION.md#native-ownership-and-world-disclosure--pending-source-cut).

See [the scoped qualification account](../ARCHITECTURE-NAVIGATION.md#scoped-native-qualification--2-october-2026)
for the exact earlier four failures, the 16-path source union, retired Flow
fixture classification and separate receiving-recovery result. A no-body
disconnect pass does not repair the lost native cause or qualify newer R4.
