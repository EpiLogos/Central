# Native Central filesystem reading

`central.files.list` and `central.files.read` expose actual filesystem material
under the configured Central root. An ordinary Work directory needs no
ProjectCentral manifest, adoption, role assignment or inferred Project identity.
These read-only actions neither reconcile a source horizon nor invoke an agent.
A file reading also carries its existing authored Source binding when Central
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
their existing provenance, authority, CAS and history contracts. An ordinary
file's edit/write/history operation remains a separate required owner seam;
clients must not implement those mutations by writing directly to disk.

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
