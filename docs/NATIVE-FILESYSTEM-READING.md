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
