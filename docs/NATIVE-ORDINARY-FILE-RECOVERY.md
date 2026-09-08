# Native ordinary-file mutation and recovery

Standing: implementation contract for `central.files.*`. This extends filesystem
reading without adopting directories, creating ProjectRefs, or demoting existing
SourceRefs. The configured canonical Central root and exact `central.path-ref/v1`
remain required on every operation.

`central.files.read` retains its existing reading and adds `operations.write`,
`operations.history`, and `operations.restore`, each `{available, reason}`.
Availability is revalidated when an Action runs. It is not an authority grant.
Protected Control, ProjectCentral, `.central`, `.git`, and participating authored
sources cannot be changed through the ordinary-file route, even with declared
human attribution. Malformed adopted project identity fails closed. The existing
source operation remains responsible for participating SourceRefs.

## Actions

| Action | Required input beyond `location` | Result |
|---|---|---|
| `central.files.write` | `expected_revision`, `content`, `actor`, `actor_kind` | `central.file-mutation/v1` |
| `central.files.history` | none; optional `limit` 1–200, `before` cursor | `central.file-history/v1` |
| `central.files.recovery_preview` | `expected_revision`, `revision` | `central.file-recovery-preview/v1` |
| `central.files.restore` | `expected_revision`, historical `revision`, `actor`, `actor_kind` | `central.file-mutation/v1` |

Mutation attribution optionally includes `agent_session_ref`. Actor kinds are
`human`, `agent`, `system`; a human declaration combined with an agent session is
invalid. These are caller declarations, not an authentication mechanism. They
cannot override the protected-ground refusal.

A mutation success has `outcome: written | unchanged`, `location`,
`previous_revision`, `revision`, `changed`, and, when written, `change`. A change
contains `cursor`, `previous_revision`, `revision`, `actor`, `actor_kind`,
`agent_session_ref`, and optional `restored_from`. No agent/model is invoked.

An initially stale basis returns successful Action delivery with
`outcome: conflict`, `expected_revision`, `current` (the complete current native
FileReading), and `changed: false`. A race detected during commit returns
`verification_failure` with `error.details.outcome: conflict`; reread the file
while preserving the draft. Protected-route refusal returns
`unavailable_capability` and `error.details.outcome: refused`. Other filesystem
and resource failures remain errors; they are not successful writes.

History has `current_revision`, newest-first `entries`, `more`, and
`next_before` (exclusive cursor, null at end). Both sides of every recorded
change are retained as exact UTF-8 snapshots. A preview returns `content`,
`current_content`, target `revision`, basis `expected_revision`, and `changed`;
it does not change source bytes. Restore passes the same CAS path as writing.
No history is inferred from git or from unobserved external edits.

## Filesystem and recovery bounds

Text remains bounded to 4 MiB and must be UTF-8 without NUL. Symlink components,
nonregular files, retrieval exclusions, hard-linked mutation targets and
read-only file permissions are refused. Descriptor-relative `openat` uses
`O_NOFOLLOW` for each ancestor and leaf; source publication uses a held parent
file descriptor and `renameat`. The parent and file identity and current
revision are checked again before publication. macOS atomic replacement preserves
ACLs, stat metadata and extended attributes; file permissions remain unchanged.

Native ordinary writers serialize through the owner's cross-process advisory
lock. External editors do not necessarily honor that lock. Changes observed
before the final publication check produce conflicts. POSIX filesystems do not
provide a universal compare-and-rename operation against noncooperating writers;
this contract does not promise exclusion of an external write in the final
check/rename interval. The atomic replacement never exposes partial new content.

History lives under `.central/file-history`, not in the desktop or authored
source. Snapshots, identity and pending records publish only after file fsync
using atomic rename. A pending receipt records the exact basis and target before
the source rename. On the next owner history or mutation call, an interrupted
operation is reconciled against current bytes: target means committed; basis
means not committed. If neither matches, the owner reports unresolved recovery
rather than inventing an outcome. A resource error after source publication says
the commit may already have occurred and must not be automatically resent.

Page memory is bounded; retained history uses durable disk. Disk exhaustion is
reported explicitly. Process termination before journal publication can leave a
same-directory `.central-write-*` temporary file; it is not a source revision
and is not silently deleted by unrelated file operations.

## Native acceptance

Run the actual candidate CLI against independent temporary Central ground:

```sh
CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 cargo test -p ctrl --locked
CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 cargo build -p ctrl --bin ctrl --locked
CTRL_BIN="$PWD/target/debug/ctrl" python3 ctrl/tests/native/ordinary_files.py
```

The executable acceptance exercises real read/write/history/restore operations,
eight simultaneous CLI writers, source identity and protected-route refusal,
external parent symlink churn, Unicode paths, native macOS metadata, and SIGKILL
of an actual writer after durable prepare followed by owner recovery. This
establishes the owner candidate; consumer/native-desktop acceptance is separate.
