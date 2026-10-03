---
role: architecture
standing: agent-inference
scope: Central native filesystem material, relation-store qualification and World projection
design_refs: []
updated: 2026-10-02
---
# Native Central filesystem reading

`central.files.list` and `central.files.read` expose actual filesystem material
under the configured Central root. An ordinary Work directory needs no
ProjectCentral manifest, adoption, role assignment or inferred Project identity.
These read-only actions neither reconcile a source horizon nor invoke an agent.
A file reading also carries its existing native Source binding when Central
already recognizes one, allowing a client to open that same editable source
without manufacturing an adoption or inferring authority from the path.

List a directory with `{"path":"Work/example/src"}`; omitting path lists the
Central root. Each entry carries its exact name, kind, byte length, retrieval
availability and an owner `central.path-ref/v1` location. Read a regular text
file by passing that returned location as `{"location": ...}`. The reading
contains its content, byte length and Central's existing versioned content
revision. Clients must use the returned location and revision without presenting
them as a semantic World/Project/Source identity or authorship claim.

Locations carry an opaque owner ref plus the canonical root and relative path.
File readings disclose actual Work project membership, with a canonical ProjectRef
only when a valid native ProjectCentral manifest supplies one. On every read the owner
revalidates the configured root, path components and actual file. Root changes,
parent traversal, symlink traversal and `.no-agent-retrieval` exclusions refuse
access. Symlink entries are disclosed as such without following their targets.
A missing/replaced location is not silently redirected to another source.

This is a filesystem reading contract, not a general write or authority grant.
Participating authored sources retain `projectcentral.source.read/write` and
their existing provenance, authority, CAS and history contracts. Ordinary
[creation and history](#creation-and-history-are-separate-owner-operations) use
their separate native owner operations; clients must not implement those
mutations by writing directly to disk.

Text reads are bounded at 4 MiB and refuse non-UTF-8 or NUL-containing binary
content. Directory reading refuses more than 20,000 entries or a non-UTF-8 name
rather than silently presenting a truncated/ambiguous tree. Paging and binary
material presentation remain explicit additional capabilities.

Native tests exercise the public Action registry over real temporary Central
ground, ordinary unadopted source files, exact content revisions, unchanged
source bytes, retrieval exclusions, root mismatches, restored symlink changes,
and binary/oversized content. There is no simulated filesystem backend.

`control.search` and `control.index` use the existing source-horizon retrieval
relation fallibly. They check the supplied native root, selected directory and
ancestors before traversal and before/after body and standing-source IO. A marked
descendant contributes no child filename, title or body; an excluded selected
root is a refusal rather than a successful empty reading. Actual IO failures
retain their kind, OS error and message in the two Actions. Missing standing
relations may be undeclared; unreadable or malformed relations are errors.

These eager Control reads admit at most 4 MiB per file. An oversized source fails
the whole command without a truncated result. Search retains an explicit
non-text skip for non-UTF-8 or NUL-containing material. Index requires text.
Body reads reuse held no-follow/nonblocking native descriptors, compare their
current named/root/parent affiliation and bytes, and close the descriptors at
exec. An unchanged root alias remains useful; retargeting requires a fresh
reading. Affiliation is physical evidence, never semantic World/Source identity.
Current checkpoints do not claim atomic exclusion of arbitrary external writers.
Existing boolean binding projections conservatively deny failed observations;
they do not supply the fallible current-body admission or prove other consumers
have completed their own read-path migration.

Aperture admission is not proof that previously read material still exists.
After all collection and emission checkpoints, before acknowledging Control
hits, titles or non-text skip metadata, these consumers reopen every returned
source through the same bounded native reader and qualify its
captured native content revision and physical observation basis. Removed or
changed material fails that reading; a fresh operation can observe the current
source. The operation retains small basis records, not descriptors for the tree.
Governance standing likewise qualifies its captured relation source before
output. A genuinely absent relation source must still be observed as absent;
an appearance, disappearance, change or unavailable observation cannot silently
emit the previous standing. Neither this check nor the physical basis creates
a semantic identity, permission lease or atomic snapshot of external writers.


Control search additionally returns each delivered hit's source_binding
(the existing native SourceBinding, or null) and source_revision (actual
content revision and byte length). A non-text skip has the same material
metadata; a withheld directory has neither a fabricated binding nor body
revision. The authored class means explicit native human-authored or
human-adopted provenance. The unresolved class leaves human authorship
unclaimed; an existing binding still discloses its exact Agent/derived/observed
provenance and standing. Durable standing alone does not establish human
adoption. Root aperture classes describe the root independently of file evidence.

Exact accepted source relations take precedence over the selected native Skill
manifest, then the existing native tree binding. Other material remains useful
without minting a SourceRef. Only selected Skill manifests are read; search
does not scan the World to classify a hit. Their typed parser and native
horizon conversion are shared with the Skill owner. The shared relation
validator checks actual schema, World id, subject, normal member paths, and
duplicate refs or paths. Ambiguity is refused without rewriting declarations.
Every metadata source used for disclosure is qualified after all checkpoints,
including a prior true absence, alongside the delivered body basis.

The public locate_control_root now returns io::Result<ControlSourceRoot>
rather than a String error. The control.open Action observes current owner
affiliation, ordinary directory form, ancestor retrieval treatment and genuine
final absence; it opens no application or source body. Real IO kind, errno and
message remain in the error with effects none. Additive search fields and
SourceClass::Unresolved require Rust/JSON consumers to preserve the distinction;
unknown external Rust consumers are not assumed absent. An aperture or
observation creates no lease, semantic identity or human recognition.


The Source transfer creation route uses `retrieval_creation_admission` through
that same native recursive treatment and form predicate. A genuinely absent
suffix below inspected existing ancestors admits only a creation aperture; it
is not proof of existing material, Source identity, payload freshness or write
authority. Delivery admission keeps its existing final-absence distinction and
qualifies actual material separately. Real IO failures retain their cause; only
an observed marker establishes masking. The creator rechecks this aperture
under its existing source-mutation lock before creating parents or source
bytes. This supplies a current checkpoint, not exclusion of arbitrary external
filesystem writers.

## Creation and history are separate owner operations

[`central.files.create`](../ctrl/src/file_creation.rs) first-saves an ordinary
file into an existing eligible native directory. It takes a returned `parent`
location, one `name`, `content`, declared `actor`/`actor_kind`,
`expected_absent:true` and `operation_ref`; `agent_session_ref` is optional.
`content_encoding` defaults to UTF-8 or accepts base64, bounded to 4 MiB decoded.
Creation shares the ordinary owner's protected/source policy, lock, snapshots
and history. Held-parent `linkat` publication atomically refuses an existing
destination; it does not overwrite another writer's file.

The native result is `central.file-mutation/v1`, with `outcome:created` or an
exact idempotent `outcome:unchanged`, location, revision, operation identity and
independent current readback. Existing destinations, changed requests or later
edits refuse first-save replay. A committed creation whose readback or recovery
finalization fails retains that effect; retry the same operation identity
rather than automatically minting another or overwriting.

Existing-file write, history, recovery preview and restore use the
[native ordinary-file mutation and recovery](NATIVE-ORDINARY-FILE-RECOVERY.md#actions)
operations. Availability is revalidated, and declared attribution is not an
authentication or protected-ground override. A returned physical location or
created file does not adopt a directory, mint a World/Project/Source identity,
invoke an agent, or transfer the Source owner's authority.

The creation source SHA-256 at the inspected preimage is
`038011d82b5587a2e6971e3cd47ec45ecbaef004da29a281ab64c29f73c3ddba`;
its native tests are in [`file_creation.rs`](../ctrl/tests/file_creation.rs).
These source relations do not establish personal installation, composed desktop
acceptance or human Recognition.

## Native relation records — source candidate

This section retains the original native-owner source candidate and its
subsequent scoped hosted qualification. It does not establish a personal
installed result. Central's `RelationRecordStore` owns the four root/Project
AgentSet and World containers. It distinguishes optional absence from unsafe
forms, unavailable IO and changed affiliation; a failed native collection
cannot become an empty exclusion graph. See [binding ownership metadata](integrations/BKMR-FILE-MAP.md#binding-ownership-metadata--source-candidate).

Reads reuse [`NativeFileRead`](../ctrl/src/file_mutation.rs) with the same held
no-follow, nonblocking and close-on-exec descriptors. The Store qualifies its
owner/container and each captured record, then reopens returned material
sequentially after collection checkpoints. It retains small basis records,
not one open descriptor per record.

A complete list observes exact admitted JSON names initially and again after
material qualification, with owner/container affiliation checked around both
observations. A changed membership is unavailable under the earlier basis,
including an initially empty existing container. Both observations preserve
the same entry rules: symbolic entries refuse before extension filtering;
only ordinary `.json` files participate; other ordinary non-JSON material is
ignored. A newly observed name is checked without reading its body. A fresh
native operation can observe the new collection. These checkpoints are not an
atomic snapshot or exclusion of external writers; late or net changes between
observations remain outside the guarantee. Complete-list memory remains
proportional to total record bytes/count.

### Capacity and public error contract

The eager relation metadata profile admits at most 8 MiB serialized bytes per
record. Larger retained material stays untouched and unavailable; it is never
truncated or silently omitted. Serialized save candidates, including their
newline, are checked before directory or staging effects. This is an
engineering capacity, not a World/AgentSet domain limit or human Recognition.
No explicit larger finite profile is exposed by this cut.

[`RelationRecordStoreError`](../ctrl/src/agent_set_store.rs) retains original IO
through `Io(RelationRecordIoError)`; clones share an `Arc<io::Error>`, exposed
through `io_error()` and `Error::source`. Rust callers constructing the former
`Io(String)` must migrate to the real error carrier, for example
`RelationRecordStoreError::Io(error.into())` for an actual `io::Error`. The
additive `RecordBudget { byte_len: u64, limit: u64 }` also requires exhaustive
match updates. Successful record JSON, schema, refs, revisions and unknown
fields retain their existing meanings.

The six read-only native AgentSet/World Action boundaries report
`effects:"none"` and original IO kind/errno/message. Capacity reports
`central.relation_record_budget`, status `unavailable_capability`, capacity
`byte_len`/`limit`/`profile` and `io_error:null`. A membership consistency
refusal has no invented OS errno. This read contract does not certify the
inherited save CAS, staging or after-effect uncertainty, nor redefine mutation
errors.

### World disclosure

The qualified Source cut's `central.world.here` consumer uses
[`RelationRecordStore::list`](../ctrl/src/agent_set_store.rs) in
[`world_record`](../ctrl/src/world_here.rs), rather than independently accepting
raw JSON. It projects native failures and qualifies absence through that owner.
A reading's `source_path` is relative to the Store's owning root; the consumer
joins that root before displaying a location relative to Central, including
for Project records. The displayed World-record `revision` is its declared
field, distinct from the physical file's content revision. The boundary keeps
presentation from acknowledging material its native owner refuses.

Source basis: native owner v4 packet SHA-256
`390656e902c866959304094a9b92c46f52434ba04140e17fe10a683bb8f7ca79`,
Store after-image
`52b45d4437a50b0e8ed42accf06572c5d410847a9d4f34230aacf4d6ab3831b1`;
World consumer v1.3 packet
`c5652f528091f7952c0b77076c7084f8c2033f16417dc9fae74417f8c3d868e5`.
The 18 Store and five World-consumer definitions were UNRUN at the original
source capture. They subsequently executed and passed on both Linux and macOS
in [normal run 37061163439](https://github.com/EpiLogos/Central/actions/runs/37061163439),
at merge `cadc4942a5aa64dd1e00216b5203dea2ff7052d7`, whose full tree
equals Central `452525d45fce20c2c667b87ee9590e7f5a3883c6`. These
results qualify the stated held-reader, error, capacity, membership and owning-
root projection relations; they do not certify every inherited mutation path.
[The scoped qualification account](ARCHITECTURE-NAVIGATION.md#scoped-native-qualification--2-october-2026)
keeps the joined composition, source-install dependency cut, prior failures and
remaining consumer/installed boundaries distinct. See [architecture navigation](ARCHITECTURE-NAVIGATION.md#native-ownership-and-world-disclosure--pending-source-cut).

## Native material publication boundary

The existing `file_mutation::atomic_record(root, relative, bytes, disposition)`
is the physical publication seam for its current native callers, including
owner records and explicitly admitted Self material. Its root/parent/device/inode
observations preserve material affiliation, not World/Project/Source identity.
The semantic owner supplies `CreateNew` or `ReplaceOrCreate`, permission,
serialization, lifecycle and recovery; successful bytes are not adoption.

The publisher uses held no-follow/nonblocking/close-on-exec descriptors, unique
exclusive staging and descriptor-relative atomic publication. Immutable admitted
privacy precedes candidate bytes; retained mode/ownership/ACL/xattrs must be
confirmed before and after publication. Native exclusive creation never replaces
a concurrently appearing target. Existing-target replacement retains the admitted
basis and acknowledges only exact published inode/bytes/metadata plus held
owner-route readback. Unsuccessful stages and unselected legacy remnants remain
in place; no unowned cleanup or new background sweeper is implied.

Actual failure after rename is typed uncertainty with the original cause and
`published:true`; lack of acknowledgement cannot be restated as absence of effect.
No universal exclusion of noncooperating filesystem writers or power-loss proof
is claimed. Source-class, retrieval policy, writes into protected human ground,
Source CAS and other owners' capacities remain separate and unchanged.
