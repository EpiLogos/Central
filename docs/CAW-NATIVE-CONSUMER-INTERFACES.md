# Native continuous-work consumer interfaces

Implementation: Central #150/#153/#151/#152. The initial native operations landed through PR #157 (superseding #155/#156); PR #161 continues document recovery and receiving on that main. These are actual handlers in the existing `ctrl action run` registry, not a proposed schema. Root agency omits `project`; a participating Project supplies its existing immediate `Work` member, e.g. `"project":"one"`. Central remains the root meta-project: root work does not fabricate a child Project. Ordinary external repositories can be authorised by root policy without stamping ProjectCentral into them.

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

Do **not** blindly serialize all Central protection into `workcell.write-boundary/v1`. The Workcell contract at the initial recorded consumer cut `f3a5be9fc751ee94b78aff11411e0cde65a46e4c` requires existing absolute **directories**, at most 64 writable and 64 protected entries, and refuses a writable ancestor of a protected directory. Central can describe protected files and repository/source subtrees that cannot be represented by that narrower contract. Such a join must report unsupported or use an explicitly reviewed representable arrangement; dropping protection is not a valid conversion. Later Workcell implementations must be inspected against their own current contract rather than assumed to have the limits of this historical cut.

A representable boundary must carry the exact policy ref/revision, authority ref, writable/protected directory paths, actual required coverage, and expiry converted from seconds to milliseconds. Do not label path validation as Landlock enforcement. The recorded Workcell Linux implementation advertises only its actual `file-content`, `file-creation`, `file-removal`, `rename-link`, `truncate`, `descendant-processes` coverage, not all metadata, network, prior hard links or outside processes. Broader required coverage or unavailable OS support must refuse material dispatch. This repository does not expand Workcell's provider contract.

## Civil Day / independent NOW operations

`central.time.policy` reads the recognised **root** `civil-time-policy` SourceRef, IANA timezone and local boundary, including DST. `central.day.ensure` requires its exact `expected_time_policy_revision`; `central.day.read` reads a DayRef or today pointer. Blank native text is distinguished from unavailable exact HTML-template fidelity. Day creation/rollover does not replace open writing, close the previous Day, carry/tick tasks, clear/archive NOW, or invoke a model.

`central.day.lifecycle` needs exact source and relation revisions and a human-scoped native credential. `central.now.lifecycle` needs exact NOW and placement-policy revisions; archive refuses recorded pending receiving/obligations, retains source history and artifacts, and does not claim to have stopped external processes. `central.now.obligations` adds, never silently removes, native obligation references. `central.temporal.source-history` reads the existing native file-history store by SourceRef. Native Flow documents retain revisions through the existing Flow owner and `projectcentral.flow.history`, not a second copy in the Day history store.

Mutations requiring authenticated authorship use a recognized root `native-action-authority` source with exact scope/action grants and SHA-256 bearer credential digests. The host passes `CENTRAL_NATIVE_TOKEN` through its protected process channel, **not document JSON**. An `H` label, `actor_kind:human`, or a claimed acceptance string is not a credential. A bearer authenticates its declared principal, not physical human presence; same-UID malicious processes require credential isolation at the host/material boundary. No personal authority source is installed by these handlers or this PR.

## Document continuation: exact source, not imported authority

The existing `central.document.read` returns `source.ref`, `document_id`, `revision.revision`, `last_native_revision` and `unreviewed_external_revision`. The existing `central.document.mutate` requires the first three identity/basis values plus a caller-unique `request_id` and `operation`. Add `project` only at Project scope. Replaying the identical request returns its original applied revision without rewriting newer source. Reusing its identity with changed input is a conflict.

After an external edit, ordinary native mutations remain refused. An authenticated human can explicitly reconcile that retained revision:

```json
{
  "source_ref": "<source.ref>",
  "document_id": "<document_id>",
  "expected_revision": "<current external revision.revision>",
  "expected_native_revision": "<last_native_revision>",
  "request_id": "review-external-edit-1",
  "operation": "external.reconcile"
}
```

The existing Day/Flow history retains the exact pre-reconciliation bytes. The operation records the human review, protects externally edited contributions from later Agent overwrite and repairs native source metadata through the existing recovery journal. It does not infer who made the external edit or certify the content. A source/metadata change after either reviewed basis is a conflict. An interrupted publication can be recovered by repeating the identical request.

`entry.insert` takes a new `entry_id` and existing `before_entry_id`. Omit `html` for a valid blank entry; include `contribution_id` with supplied rich text otherwise. `note.add` takes `note_id`, `timing: During | After`, optional `parent_note_id`, optional sanitised `html`, and optionally:

```json
{
  "kind": "contribution",
  "target_id": "<existing local contribution id>",
  "original_text": "<the explicitly selected passage>"
}
```

That object is the `anchor` input. `kind` also accepts `entry` or `field`; an empty original text is allowed for whole-entry notes. The native operation records the current target basis. Later changes retain the quote and local target with `status: needs-review`, never silently retargeting. During/After notes and replies belong to the shared document source, not a Flow/Dialogue layout. Complete linked-page selection/context-packet operations remain separate work; adding a note does not send anything to an Agent.

`central.document.export` returns both a `snapshot` (`central.document-retained-snapshot/v1`) and inert `html`. The readable snapshot includes human fields, ordered entries including blanks, contribution attribution, notes and stale-anchor warnings. The JSON preserves the supplied template payload and all retained local identities. This is an explicit retained copy, not autosave or original-template fidelity.

Reopen into the **same existing native document** using `central.document.mutate` with `operation: portable.restore`, fresh identity/revision/request fields and either `html` containing that retained copy or `value` containing the returned snapshot. Do not supply both. Authenticated human review is required. Native identity/date/field definitions/lifecycle must still match; existing owner journals and fidelity standing cannot be granted by imported JSON. Changed imported contributions retain declared attribution separately and become protected reviewed material. Arbitrary original HTML, new-World adoption, non-empty plain-source conversion and an interactive standalone browser editor are not implemented by this operation.

## Root receiving by reference

The ordinary scoped `central.receiving.list` remains available. At root, explicitly select participating Projects:

```sh
ctrl --json --root "$CENTRAL_ROOT" action run central.receiving.list \
  '{"projects":["one","two"],"limit":20}'
```

The host must supply a native credential whose `central.receiving.list` grant covers **root and every selected Project**. A root grant is not ambient Project authority. `projects: []` explicitly requests only the authenticated root aperture. Existing repositories are not stamped or adopted by this read.

The `central.receiving-aperture/v1` result carries summary rows with their original `return_ref`, current receiving revision, `scope_ref`, optional `project`, and an `open` Action/input addressing the owning `central.receiving.read`. It does not copy Day prose or proposal bodies. Native source and retained-artifact disclosure are checked at read time; a revoked source is withheld, not served from cached receiving material. A missing scope grant or changed authority basis refuses the composition.

For the next page, retain the exact `projects` order and pass the returned **cursor object** as `cursor`, not a scoped `after` integer. The cursor binds the selected scopes and per-owner sequence positions. Round-robin progression prevents a busy root from starving Project results and does not depend on receipt clocks being monotonic. This is a per-scope locked reading with live disclosure revalidation, not a global atomic snapshot.

## Draft Return → review → inclusion

`central.receiving.submit` retains the existing required producer key, target identity/revision and native document-operation `proposal`. Optional `summary` is bounded plain text; `evidence_refs` are retained opaque owner refs. Optional `artifacts` contain up to sixteen exact participating source selections:

```json
{
  "source_ref": "<actual draft SourceRef>",
  "expected_revision": "<actual draft revision>",
  "producer_ref": "<declared original producer, when known>",
  "proposed_target_ref": "<proposed README or other target ref>"
}
```

Central reads and retains the exact source bytes, binding, revision and SHA-256, plus authenticated `submitted_by`. A declared original producer is not silently promoted to authenticated identity. This first bounded evidence route accepts at most 512 KiB of retained UTF-8 draft content per Return; it is not a binary/media attachment route. Arrival remains pending and does not insert the draft into a human Day or adopt it as human-authored source. Source revocation continues to protect retained evidence on later reads.

`central.receiving.review` takes `return_ref`, current `expected_return_revision` and a disposition. `accepted` additionally pins `expected_source_revision`; it does not include the contribution. `rejected` preserves the received record. `pending` clears an unapplied acceptance/rejection without resetting included or uncertain effects. `acknowledged` records the authenticated human acknowledgement independently without changing review/inclusion status. None is Factory Recognition.

`central.receiving.include` applies the accepting human's exact reviewed proposal through document ownership. The contribution retains its original occurrence and receipt times rather than receiving the later review time. Missing occurrence remains missing. The existing `central.receiving.recover` handles interrupted inclusion. Accepted-but-unincluded work remains an archive obligation; successful inclusion and later NOW archive/re-entry do not delete the exact retained draft evidence.

## Reproduce without a personal installation

```sh
python3 scripts/prove-continuous-work.py --ctrl /path/to/built/ctrl
cargo test -p ctrl --locked --test continuous_work_native
cargo test -p ctrl --locked --test continuous_work_continuation -- --nocapture
cargo test --workspace --locked --no-fail-fast -- --nocapture
```

The Python proof creates and deletes its own controlled World and prints complete actual requests/results. Its initial execution used build run `34524306879`, test merge `c9b74b56226d4f933bd7a1b8fc9a278346a95998` (branch cut `c930ba9976a62117422335e714e328e07dd8a3e6`): eight concurrent CLI allocation processes, exactly one source creation, stable NOW identity/revision, valid repository and T destinations, rejected loose Work-root scratch. That historical evidence covers the first policy/NOW boundary, not later implementations. Follow the exact-head PR checks and retained logs for current results.

The continuation suite exercises root plus two Project Worlds through actual `ctrl` subprocesses and controlled-clock native operations: external Day/Flow reconciliation and exact history, interrupted metadata recovery, native retained HTML restoration, notes/anchors/entry order, forged/stale payload refusal, scoped root receiving and credential revocation, concurrent replay, and draft receiving through archive/re-entry. The test-only payload keys are not claims to be the original seventeen CT4b keys.

### Remaining evidence and implementation boundaries

The original HTML/CT4b payload is still required for its exact seventeen-key/layout fidelity test. Its absence does not block the native tests above. Embedded image/audio/video with the 12 MB per-file rule, an interactive standalone document/browser round-trip, arbitrary template import and new-World adoption, complete linked-page context and non-empty source conversion remain repository work. Broader legacy migration, active-process/material joins and the actual model/Factory headless Return circuit retain their native owners and are not proved by these disposable tests.

This is a repository implementation boundary, not installed-world or independent full-feature acceptance. Personal governance/credential adoption, real-machine enforcement, actual original-template rendering and human assessment remain separate local proof. No private Control is read or mutated by the fixture campaign.
