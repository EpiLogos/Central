# Native continuous-work consumer interfaces

Implementation: Central #150/#153, draft PR #155. These are actual handlers in the existing `ctrl action run` registry, not a proposed schema. Root agency omits `project`; a participating Project supplies its existing immediate `Work` member, e.g. `"project":"one"`. Ordinary external repositories can be authorised by root policy without stamping ProjectCentral into them.

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

Do **not** blindly serialize all Central protection into `workcell.write-boundary/v1`. The accepted Workcell contract at `f3a5be9fc751ee94b78aff11411e0cde65a46e4c` requires existing absolute **directories**, at most 64 writable and 64 protected entries, and refuses a writable ancestor of a protected directory. Central can describe protected files and repository/source subtrees that cannot be represented by that narrower contract. Such a join must report unsupported or use an explicitly reviewed representable arrangement; dropping protection is not a valid conversion.

A representable boundary must carry the exact policy ref/revision, authority ref, writable/protected directory paths, actual required coverage, and expiry converted from seconds to milliseconds. Do not label path validation as Landlock enforcement. Workcell's Linux implementation advertises only its actual `file-content`, `file-creation`, `file-removal`, `rename-link`, `truncate`, `descendant-processes` coverage, not all metadata, network, prior hard links or outside processes. Broader required coverage or unavailable OS support must refuse material dispatch. This repository does not expand Workcell's provider contract.

## Civil Day / independent NOW operations

`central.time.policy` reads the recognised **root** `civil-time-policy` SourceRef, IANA timezone and local boundary, including DST. `central.day.ensure` requires its exact `expected_time_policy_revision`; `central.day.read` reads a DayRef or today pointer. Blank native text is distinguished from unavailable exact HTML-template fidelity. Day creation/rollover does not replace open writing, close the previous Day, carry/tick tasks, clear/archive NOW, or invoke a model.

`central.day.lifecycle` needs exact source and relation revisions and a human-scoped native credential. `central.now.lifecycle` needs exact NOW and placement-policy revisions; archive refuses recorded pending receiving/obligations, retains source history and artifacts, and does not claim to have stopped external processes. `central.now.obligations` adds, never silently removes, native obligation references. `central.temporal.source-history` reads the existing native file-history store by SourceRef.

Mutations requiring authenticated authorship use a recognized root `native-action-authority` source with exact scope/action grants and SHA-256 bearer credential digests. The host passes `CENTRAL_NATIVE_TOKEN` through its protected process channel, **not document JSON**. An `H` label, `actor_kind:human`, or a claimed acceptance string is not a credential. A bearer authenticates its declared principal, not physical human presence; same-UID malicious processes require credential isolation at the host/material boundary. No personal authority source is installed by these handlers or this PR.

## Reproduce without a personal installation

```sh
python3 scripts/prove-continuous-work.py --ctrl /path/to/built/ctrl
cargo test -p ctrl --test continuous_work_native
cargo test -p ctrl continuous_work::tests
```

The Python proof creates and deletes its own controlled World and prints complete actual requests/results. It was executed against the CI-built native binary from build run `34524306879`, test merge `c9b74b56226d4f933bd7a1b8fc9a278346a95998` (branch cut `c930ba9976a62117422335e714e328e07dd8a3e6`): eight concurrent CLI allocation processes, exactly one source creation, stable NOW identity/revision, valid repository and T destinations, rejected loose Work-root scratch. That evidence covers the first policy/NOW boundary, **not** later lifecycle/document/migration implementations.

The ten initial Rust policy/NOW tests also passed in that run. The workspace subsequently stopped at an existing fixed Action-count assertion; that assertion was corrected to verify the retained baseline plus exact new descriptors. Follow PR checks for the latest full workspace result. Tests added later are not covered by the earlier binary proof.

This is a repository implementation boundary, not installed-world or independent full-feature acceptance. Personal governance adoption, real-machine enforcement, actual original-template fidelity and the independent full-feature journey retain their separate acceptance duties.
