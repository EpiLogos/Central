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
cargo test --locked -p ctrl --test native_ordinary_interruption_child --features native-ordinary-interruption-child --no-run --message-format=json
# Export CTRL_ORDINARY_INTERRUPTION_BIN from the exact returned compiler-artifact executable.
mkdir -p ProjectCentral/now/tmp/ordinary-file-native-evidence
export CTRL_NATIVE_EVIDENCE_DIR="$PWD/ProjectCentral/now/tmp/ordinary-file-native-evidence"
CTRL_BIN="$PWD/target/debug/ctrl" python3 ctrl/tests/native/ordinary_files.py
```

The executable acceptance exercises real read/write/history/restore operations,
eight simultaneous CLI writers, source identity and protected-route refusal,
external parent symlink churn, Unicode paths, native macOS metadata, and SIGKILL
of an actual writer at acknowledged durable prepare and after source rename,
followed by owner recovery. The required `CTRL_ORDINARY_INTERRUPTION_BIN` names
the exact compiler-reported opt-in test executable, built with
`native-ordinary-interruption-child`. It includes the same crate Source and
actual CLI, with a `cfg(test)` checkpoint bridge absent from default library and
binary builds. A bounded root-local test admission binds the actual request,
root and source identities. A held inherited pipe reports the actual pending
checkpoint; the direct writer holds it for at most five seconds, refuses parent
loss/late observation, and cannot return a successful write. The parent retains
its original five-second checkpoint deadline, kills and reaps the actual writer,
then asks the production CLI to reconcile. Missing/changed admission and source
bytes matching neither recorded revision must remain failures. No fixture writes
a pending record or invents a receipt. Actual compiler/binary identities and all
Linux/Mac results are retained by the normal verification workflow. Raw phase
notifications/capture, admission refusals and production recovery replies remain
in the required caller evidence directory after disposable fixture cleanup.
This
establishes the owner candidate; consumer/native-desktop acceptance is separate.

## Supporting material publication and acknowledgement

The existing native `file_mutation::atomic_record` publishes owner-supplied bytes
under an explicit native root and normal relative member. Each existing owner
retains its own lock, identity, revision/CAS, lifecycle and recovery. First
identity/snapshot/proposal publication is exclusive; mutable pending/cursor/intent
updates replace an admitted current record, or exclusively create an absent one.
A shared physical routine does not make its bytes human Source or acceptance.

Publication holds the admitted root and parent descriptors, requalifies the
original supplied-root route, refuses symlink/nonregular/multiple-link targets,
and allocates a unique create-new same-directory stage. `record-staging` is no
longer opened, truncated, renamed, consumed or swept. Existing remnants remain
unselected evidence. Before candidate bytes, an immutable UID/GID/mode/ACL/xattr
expectation is captured from the admitted source or actual new stage defaults.
Supported retained metadata is copied and checked; unsupported retention refuses
before publication. Operational read-only records remain owner-replaceable with
their mode retained; this does not change Source read-only or write authority.

Stage bytes/fsync, held-directory rename, directory fsync and actual named/held
readback establish distinct observations. Failures after rename retain typed
`published:true`, actual target and original IO cause: publication may have
committed although durability/current acknowledgement is unconfirmed. Native
Action errors use `partial_completion` / `central.publication_uncertain` when
this physical uncertainty or actual prior owner progress exists, with body-free
`record_publication` and `prior_owner_observation`. Source publication and
semantic acceptance are never inferred from a supporting material publication.
Use the existing native read/recovery and original input, without automatic
resend. Failure before rename retains its exact unique stage; no failed-stage
check-then-unlink can remove another actor's replacement.

Readback streams at most the admitted candidate length plus one byte. It does
not import a Wiki body limit or truncate legitimate serialized records above
8 MiB; each caller's existing admission/read capacity remains its own contract.
Metadata observation retains the existing finite Wiki mechanical metadata
profile, separately from body capacity. Original IO objects survive typed error
sources; new diagnostic observations are body-free, at most 8 KiB each, with
explicit optional text omission rather than identity truncation. These are
mechanical profiles, not new domain meaning or human acceptance.

Owner-cooperating serialization does not exclude arbitrary external writers in
a final check/rename interval. `source_safety` source-stage cleanup, SourceTransfer
conflict serialization, hardware power-loss guarantees and installed acceptance
remain separate obligations. The added real filesystem/native Action and CLI
regressions require qualification on the published composed Source cut; their
Source definitions alone do not establish execution.

An interrupted first save may already have acknowledged its native creation
request while the Source is still absent. The existing owner compares the full
retained request before resuming; only that exact request may update its own
creation record. A different operation/request remains refused. This does not
create a new identity, acknowledge a Source that is still absent, or turn a
failed pending-record write into an instruction to resend another request.
