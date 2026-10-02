# Native continuous-work consumer interfaces

Initial implementation provenance: Central #150/#153 and PR #155. For current native World/Workcell-root NOW/Day and ordinary Flow operations, use [architecture navigation](ARCHITECTURE-NAVIGATION.md) and the current handler revision; the original draft label is not current implementation standing. These are actual handlers in the existing `ctrl action run` registry, not a proposed schema. Root agency omits `project`; a participating Project supplies its existing immediate `Work` member, e.g. `"project":"one"`. Ordinary external repositories can be authorised by root policy without stamping ProjectCentral into them.

## Read → allocate → validate

The CLI takes one JSON object as its final argument:

```sh
ctrl --json --root "$CENTRAL_ROOT" action run central.work.policy '{}'
```

Success is the ordinary ActionResult envelope, `{"ok":true,"status":"success","action":"central.work.policy","data":{...}}`. `data` contains:

- `schema: central.effective-placement-policy/v1`, `scope_ref`, and `revision` for the entire effective reading;
- `sources[]`: real SourceBinding (`ref`, path, provenance, standing, treatment, retrieval state), its exact content revision and authority bases;
- absolute `writable_destinations[]` with class and material path anchor; absolute `protected_paths[]`;
- `enforcement`, `required_coverage`, `issued_at_unix_seconds`, `expires_at_unix_seconds`;
- `outside_writes_prevented:false`. Central validates routed native operations; this is not an OS enforcement claim.

Recognised source law is selected through the existing root/Project source relations and the `work-placement-policy` role. No recognized root policy is an explicit refusal, not a permissive default. A Project-local policy pins its root policy SourceRef/content revision and can narrow, not widen, root grants. Do not manufacture Recognition or adopt private governance while integrating a consumer.

A root policy can also explicitly grant an existing linked checkout under
`worktrees/<slot>/<checkout>` with class `worktree`. Its primary repository must
already have a `repository` grant under `Work`. Central checks Git's checkout
marker, administrative directory, common directory and backlink together; a
directory name or copied marker does not establish registration. Symlinked
registration components are refused, including a symlink followed by `..`.
Ordinary Git `commondir` parent traversal remains supported. The grant covers
only the selected checkout, not its siblings. Its `.git`, `.central` and
`ProjectCentral` paths and the primary repository's common Git directory remain
protected. This allows an assigned development checkout without granting shared
Git administration or authored Project ground to the executing agent. Policy
changes still require a fresh effective revision at dispatch.

Pass the returned effective revision unchanged:

```json
{
  "task_ref": "task:consumer-proof",
  "purpose": "Native consumer request/result proof",
  "expected_policy_revision": "central.content-fnv1a64/v1:1164:78279682d3d3c8e3"
}
```

Call `central.now.allocate`. That revision above is a **recorded disposable-test result**, not a usable revision in another World. The real test returned:

```json
{
  "now_ref": "central:now:control:root:56863853957ec1f5fe0cf71c5f4344d09a39602adc768fea89c9b589f18ecc97",
  "source": {
    "ref": "central:source:control:root:Control/agents/now/clearings/56863853957ec1f5fe0cf71c5f4344d09a39602adc768fea89c9b589f18ecc97/now.json",
    "path": "Control/agents/now/clearings/56863853957ec1f5fe0cf71c5f4344d09a39602adc768fea89c9b589f18ecc97/now.json"
  },
  "revision": {
    "revision": "central.content-fnv1a64/v1:690:7d9fadfe4ef9fdb6",
    "byte_len": 690
  },
  "writable_destination": "/tmp/central-consumer-proof-3n5cuzj0/Control/agents/now/clearings/56863853957ec1f5fe0cf71c5f4344d09a39602adc768fea89c9b589f18ecc97/T",
  "artifact_namespace": "T"
}
```

This is a subset of the actual response, not a fabricated full ActionResult. The temporary World has been deleted. The complete live response also contains `created`, the NOW source record, current effective policy, permitted artifact kinds and explicit no-model-invocation disclosure. Use the response's real values; never reconstruct a path from an opaque hash or reuse sample revisions. Same task/scope/purpose/relationships are idempotent, including concurrent processes. Reusing that task key with a different purpose or relationship basis is an identity conflict, not shared scratch.

Call `central.now.read` with `{"now_ref":"<returned NOW ref>"}` for fresh source state. Call `central.work.validate` with:

```json
{
  "now_ref": "<returned NOW ref>",
  "expected_now_revision": "<returned revision.revision>",
  "expected_policy_revision": "<current policy.revision>",
  "destination": "Work/demo/src/main.rs"
}
```

`destination` is absolute or relative to the Central root. A permitted native response includes the exact destination anchor and policy expiry. Supply its `expected_destination_anchor` when revalidating the same preview. The consumer still has to validate at its own execution/commit boundary; this read is not a reservation against arbitrary outside writers.

For `Work/scratch.diff`, the real native response is `ok:false`, code `placement_rejected`, with details containing `allowed:false`, the exact `valid_now_destination`, current bases and retry Action. Retarget intentionally, then revalidate; never silently rewrite the command. `Work/demo/src/main.rs` and a file under the allocated T directory were permitted. `ctrl` did not perform those ordinary engineering writes.

## AIKit join

Before commencing situated task work, obtain effective policy, allocate or resume the task's actual NOW, retain both SourceRef and exact revision, and disclose the returned engineering/NOW destinations. At dispatch and interruption/re-entry, reread changed bases and enforce the declared interception coverage. A policy/source conflict or expired material arrangement requires re-resolution; it does not authorize broader paths. A closed/archived NOW requires explicit lifecycle re-entry, not a replacement task allocation.

The root/Project NOW source persists participant/source references, task purpose, original allocation policy, obligations, lifecycle and archive/continuation references. Native allocation itself neither commissions nor executes an agent.

## Workcell join: preserve exact limits

The actual native NOW destination is an existing directory. It can supply the existing `workcell.directory-storage/v1` contract:

```json
{
  "schema": "workcell.directory-storage/v1",
  "directories": [{"logical_ref": "<returned NOW ref>", "path": "<returned writable_destination>"}]
}
```

The corresponding required storage demand remains `access:writable`, `sharing:shared`, `persistence:external`, `retention:preserve`; use Workcell's documented demand envelope. Directory attachment is not allocation and release is not deletion. Retain Workcell's real device/inode correlation and re-entry result.

Do **not** blindly serialize all Central protection into `workcell.write-boundary/v1`. The accepted Workcell contract at `f3a5be9fc751ee94b78aff11411e0cde65a46e4c` requires existing absolute **directories**, at most 64 writable and 64 protected entries, and refuses a writable ancestor of a protected directory. Central can describe protected files and repository/source subtrees that cannot be represented by that narrower contract. Such a join must report unsupported or use an explicitly reviewed representable arrangement; dropping protection is not a valid conversion.

A representable boundary must carry the exact policy ref/revision, authority ref, writable/protected directory paths, actual required coverage, and expiry converted from seconds to milliseconds. Do not label path validation as Landlock enforcement. Workcell's Linux implementation advertises only its actual `file-content`, `file-creation`, `file-removal`, `rename-link`, `truncate`, `descendant-processes` coverage, not all metadata, network, prior hard links or outside processes. Broader required coverage or unavailable OS support must refuse material dispatch. This repository does not expand Workcell's provider contract.

## Civil Day / independent NOW operations

`central.time.policy` reads the recognised **root** `civil-time-policy` SourceRef, IANA timezone and local boundary, including DST. `central.day.ensure` requires its exact `expected_time_policy_revision`; `central.day.read` reads a DayRef or today pointer. Blank native text is distinguished from unavailable exact HTML-template fidelity. Day creation/rollover does not replace open writing, close the previous Day, carry/tick tasks, clear/archive NOW, or invoke a model.

Clearings may carry a material horizon (O-I `docs/contracts/WORLD-INHABITATION-V1.md` §1). `central.now.workcell-root {workcell_ref}` idempotently ensures the one root NOW of a Workcell in the root register (task `central:task:control:root:workcell-root:<workcell_ref>`, `horizon: "workcell-root"`). `central.now.allocate` accepts `parent_now_ref` — an allocated NOW of the same scope or of the root scope, never another Project's — which makes the clearing a `horizon: "child"`, and a declared `workcell_ref`. `central.now.children {now_ref}` lists every child, uncapped, across the root register and all Projects for a root parent. The three fields are optional and skipped when absent, so standalone clearings keep their exact bytes and schema. `central.day.ensure` and `projectcentral.now.rollover` report `now_horizon` (active horizon clearings `carried`, quiescent ones `released` and retained) and never close, complete or archive a clearing.

`central.day.lifecycle` needs exact source and relation revisions and a human-scoped native credential. `central.now.lifecycle` needs exact NOW and placement-policy revisions; archive refuses recorded pending receiving/obligations, retains source history and artifacts, and does not claim to have stopped external processes. `central.now.obligations` adds, never silently removes, native obligation references. `central.temporal.source-history` reads the existing native file-history store by SourceRef.

Mutations requiring authenticated authorship use a recognized root `native-action-authority` source with exact scope/action grants and SHA-256 bearer credential digests. The host passes `CENTRAL_NATIVE_TOKEN` through its protected process channel, **not document JSON**. An `H` label, `actor_kind:human`, or a claimed acceptance string is not a credential. A bearer authenticates its declared principal, not physical human presence; same-UID malicious processes require credential isolation at the host/material boundary. No personal authority source is installed by these handlers or this PR.

## Document continuation: exact source, not imported authority

`central.document.read` returns `source.ref`, `document_id`, `revision.revision`, `last_native_revision` and `unreviewed_external_revision`. `central.document.mutate` takes those identity/basis values plus a caller-unique `request_id` and an `operation` (`project` only at Project scope). Replaying the identical request returns its original applied revision; reusing its identity with changed input is a conflict. `occurred_at_unix_seconds` and `received_at_unix_seconds`, when present, must be integers.

After an external edit, ordinary native mutations stay refused. An authenticated human reconciles the exact retained revision:

```json
{"source_ref":"<source.ref>","document_id":"<document_id>","expected_revision":"<current external revision>",
 "expected_native_revision":"<last_native_revision>","request_id":"review-external-edit-1","operation":"external.reconcile"}
```

The reconciling write goes through source history for Day **and** Flow/Dialogue documents, so `central.temporal.source-history` retains the external revision's exact bytes (an ordinary Flow write keeps its operations log as its history; an external edit is the one state that log cannot reconstruct). The operation records the human review, locks externally edited contributions against later Agent overwrite, and repairs the native metadata through the recovery journal. It does not infer who made the external edit. A change to either basis is a conflict; an interrupted reconciliation recovers by repeating the identical request.

- `entry.insert` — new `entry_id` before an existing `before_entry_id`; omit `html` for a blank entry.
- `note.add` — `note_id`, `timing: During | After`, optional `parent_note_id`, optional sanitised `html`, optional `anchor {kind: contribution|entry|field, target_id, original_text}` (empty text allowed for a whole target). The anchor records the target's current basis; when the target later changes the note keeps its quote and target and is marked `status: needs-review`, never silently retargeted. Adding a note sends nothing to an Agent.

`central.document.export` returns a `snapshot` (`central.document-retained-snapshot/v1`) and inert `html`: human fields, ordered entries including blanks, contribution attribution (`display_role` as recorded), notes with stale-anchor warnings, and the escaped retained JSON. It is an explicit retained copy — not autosave, not original-template fidelity.

`portable.restore` reopens that copy into the **same** native document: fresh identity/revision/request fields and either `html` (the retained copy) or `value` (the snapshot), not both. Human only. Native identity, date, kind, field definitions and lifecycle must match; operations, creation digest, template fidelity and import history come from the owner, never the imported JSON. Changed imported contributions keep their declared attribution as `imported_attribution` and become locked, reviewed material. Arbitrary original HTML, new-World adoption and a standalone browser editor are not implemented.

## Receiving: contributions and owner requests

`central.receiving.*` is the one ledger through which work reaches the person, per register (root or Project), at `.central/source-returns/contributions/`. A Return is one of two kinds:

- **contribution** — proposes one operation on a native Day/Flow/Dialogue document (`source_ref`, `document_id`, `expected_source_revision`, `proposal`). Submit refuses an operation the document owner cannot apply, and refuses human-only operations (`field.set`, `title.set`, `summary.set`, `lifecycle.set`) from a non-human credential. Review accepts at the exact current source basis; include applies it with the contributor's attribution.
- **request** — asks the person to decide and targets no document: `request {kind: proposal|question, subject, body?, proposed_owner_ref?, proposal_ref?, options?}` (a proposal carries no options; a question has no proposed owner). A request from a non-human credential must name the NOW it belongs to (`now_ref`); that NOW cannot archive while the request is unsettled.

Either kind may carry `summary`, `evidence_refs` (≤128), `reply_to` (an opaque message ref, e.g. a Gateway Communique) and `declared_producer {ref, actor_kind: agent|native-service, attribution: verified|claimed}`. The credential stays the authenticated `author`; an Agent credential may not declare a different producer.

Either kind may also carry `artifacts` — up to 16 exact source selections `{source_ref, expected_revision, producer_ref?, proposed_target_ref?}` (e.g. README drafts proposed for adoption). Central reads each at its exact revision and retains the bytes, binding, revision, `content_sha256`, the authenticated `submitted_by` and the declared producer (never promoted to identity), with standing `retained-source-evidence-not-human-adoption`; at most 512 KiB of text per Return. Arrival adopts nothing. A Return is disclosed only while its target document and every artifact origin are still readable: revoking retrieval on any of them withholds it from `list` (counted in `withheld_unavailable_sources`) and refuses `read`, review and inclusion. List rows carry `artifact_count`.


`central.receiving.read` accepts exactly one non-null selector: the known
`return_ref`, or the original `producer_key`. Both are nonempty strings of at
most 4096 bytes. Optional `original_request` is an object; optional
`expected_authority_revision` is bounded nonempty text. Null means absent;
malformed types are refused, never silently dropped. The latter two fields
require the producer-key selector. Existing by-ref responses remain unchanged.

Producer-key lookup requires the protected credential's current exact-scope
`central.receiving.submit` grant. It uses the SAME native receipt identity and
asserts the retained authenticated carrier, not a `declared_producer` label.
Current target/artifact disclosure remains a separate requirement. With
`original_request`, Central compares the immutable exact original submit JSON
(including occurrence time, project spelling and old authority expectation)
against its retained request digest. The lookup's current authority expectation
is independent; a changed original request conflicts rather than resubmitting.
The existing `central.receiving-reading/v1` response adds transient
`lookup {selector: authenticated_producer_key, original_request_verified: bool}`
only on this selector. No persistent schema, receipt ref or hash changes.

A key-only match confirms publication under that principal/key/scope, not the
acceptance of changed queued input. A guarded match confirms the exact original
input and returns CURRENT review/inclusion state; it does not perform human
review, Recognition or inclusion recovery. Failed, denied, absent or unavailable
lookup keeps the sender's original intent unresolved and never triggers an
automatic resend. A receipt must not be reconstructed by consumer-side hashing.
Existing original author, authority, occurrence/receipt times and all stored
proposal/evidence remain retained. Lookup uses the existing owner lock order and
bounded record read; it adds no read registration, cursor increment or store.

Decisions are human-only `central.receiving.review` dispositions: a proposal is `accepted` or `rejected`, a question is `answered` (with `answer`) or `rejected`; any Return may be left `pending` or `acknowledged` (seen, not decided — status unchanged). An optional `note` travels with the decision. A proposal naming an owner (e.g. `factory`) stays open after acceptance until the accepting human records that owner's realisation with `central.receiving.include {realisation_ref, realisation_owner_ref}`; Central never calls the owner, and replaying the same ref is idempotent while a different ref conflicts. Including a contribution keeps its original occurrence and receipt times rather than the review time; a missing occurrence stays missing.

After a contribution's `including` update has been acknowledged, failure to
complete its document/Receiving acknowledgement returns existing
`partial_completion` with code `central.receiving.inclusion_incomplete`. The
original document IO remains the native Rust error cause; actual follow-up
Receiving and recovery-observation IO are retained separately. Added details
carry actual IO kinds/raw OS codes (null when a native preflight has no errno),
the prior acknowledged `including` revision, attempted final status and any
acknowledged final update. A failed write has `persistence: unconfirmed`: it may
have renamed before an error. These are invocation observations, not a CURRENT
receipt state or a proof that the source remained unchanged.

Document `committed`, actual returned operation/replay receipt facts, and an
actual recovery `not_committed` observation are separate from Receiving's
persisted `included`/`needs-review` state. No receipt or applied revision is
invented. Current state remains `central.receiving.read` under its existing
principal/scope and disclosure rules; neither a failure nor lookup triggers an
automatic include, resend or recovery. Ordinary successful include/replay and
successful return-to-review responses, schema, digest, identity and attribution
are unchanged; unrelated generic IO status/code mappings are unchanged.

Added diagnostic details copy only typed body-free native receipt scalars,
never the document result, request, token, nested error Display or Debug.
Strings exceeding 4096 bytes are explicitly omitted with their byte length;
identities are never truncated/reminted. Actual cause-chain observations stop
at eight source nodes, with explicit limit disclosure. Added details are bounded
to 64 KiB, including JSON escaping, by explicit optional-observation omission.
This bounds new facts, not the existing displayed error text or a universal
JSON heap. Test-only permission checkpoints exercise actual dual/late ledger
faults through the native owner and same formatter; the public executable tests
exercise real single-fault JSON and current readback, not a dual-fault CLI or
installed/human acceptance claim.

Settled means `included | rejected | answered | cancelled`, or `accepted` for a proposal with no proposed owner. `central.receiving.list` rows carry `kind`, the request subject/owner, `declared_producer`, `summary`, `acknowledged` and `settled`; `open: true` pages only unsettled Returns, and `open_total` is the exact unsettled count for the scope. `central.now.read` composes each Return keyed to the NOW with its `decision` (disposition, answer, note) and `realisation`, so the asking Agent reads the person's decision where it works.

## Reproduce without a personal installation

```sh
python3 scripts/prove-continuous-work.py --ctrl /path/to/built/ctrl
cargo test -p ctrl --test continuous_work_native
cargo test -p ctrl --test continuous_work_registered_worktree
cargo test -p ctrl --test continuous_work_continuation
cargo test -p ctrl continuous_work::tests
```

The Python proof creates and deletes its own controlled World and prints complete actual requests/results. It was executed against the CI-built native binary from build run `34524306879`, test merge `c9b74b56226d4f933bd7a1b8fc9a278346a95998` (branch cut `c930ba9976a62117422335e714e328e07dd8a3e6`): eight concurrent CLI allocation processes, exactly one source creation, stable NOW identity/revision, valid repository and T destinations, rejected loose Work-root scratch. That evidence covers the first policy/NOW boundary, **not** later lifecycle/document/migration implementations.

The ten initial Rust policy/NOW tests also passed in that run. The workspace subsequently stopped at an existing fixed Action-count assertion; that assertion was corrected to verify the retained baseline plus exact new descriptors. Follow PR checks for the latest full workspace result. Tests added later are not covered by the earlier binary proof.

This is a repository implementation boundary, not installed-world or independent full-feature acceptance. Personal governance adoption, real-machine enforcement, actual original-template fidelity and the independent full-feature journey retain their separate acceptance duties.
