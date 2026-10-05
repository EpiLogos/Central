# Native Source Return

Implementation standing: Central owns durable proposals and exact source bases.
A desktop or provider does not keep its own return store. Returned bytes remain
proposal material until a native operation applies them. None of these Actions
invokes an agent or model.

## Flow disclosure seam (retired)

The Flow disclosure Actions were retired with the legacy FlowRef registry
(Central #177): `projectcentral.flow.inspect`, `projectcentral.flow.read` and
`projectcentral.flow.write` no longer exist, and `.central/flows.json` /
`.central/flow-revisions` are no longer read or written. The owner's dated
Flow documents remain ordinary files under `Control/user/flows/`, carried by
the ordinary file CAS and its native history. A ground with leftover registry
files stays visible: the world map discloses the residue instead of silently
looking clean.

## Return operations

Every operation requires the existing native `project` selector. The operation
resolves its current Project identity and exact SourceRef before acting.

| Action | Other inputs | Result |
|---|---|---|
| `projectcentral.source.return` | `source_ref`, `expected_revision`, `proposed_content`, `reason`, `agent_session_ref`, optional `evidence_refs`, optional `acceptance: commissioned-maintenance` | default `central.source-return-reading/v1`; selected mode `/v2` |
| `projectcentral.source.returns` | optional `limit` (1–100), exclusive `before` cursor | bounded `central.source-returns/v1` metadata |
| `projectcentral.source.return_read` | `return_ref`, optional `acceptance: commissioned-maintenance` | proposal, current source, basis comparison, default acceptance availability; selected authority ref/revision metadata |
| `projectcentral.source.return_accept` | `return_ref`, `expected_revision`, `acceptance: human-accepted` or `commissioned-maintenance`, `accepted_by_ref`; selected mode also requires `expected_authority_revision` | accepted native owner receipt or conflict/refusal |
| `projectcentral.source.return_reject` | `return_ref` | rejected proposal, unchanged source |

A `central.source-return/v1` proposal preserves `return_ref`, `source_ref`,
`basis_revision`, exact `basis_content`, `proposed_content`, `reason`, evidence
refs, agent session provenance, `status`, optional acceptance attribution and
result revision. The proposal body is immutable. Storage is owner operational
state under `.central/source-returns`; it is not authored ground or wiki.

Creation with a stale source basis returns `outcome: conflict` and current source
without storing a proposal. Acceptance also compares the immutable proposal
basis against both supplied expectation and current source. Conflict leaves the
proposal pending and the concurrent source intact. Rejection never writes source.
Eligible source returns use the native source writer. Proposal/list memory and
body sizes are bounded.

## Authenticated commissioned maintenance and declared collaborative acceptance

The default `human-accepted` variant remains a caller declaration. It applies
only where the existing Agent source authority permits it; protected human
source remains refused. `ActionExecutionContext` itself has no Principal field.
Neither a repository merge nor `accepted_by_ref` is authentication.

The selected `acceptance: commissioned-maintenance` variant on the SAME
`projectcentral.source.return_accept` uses the existing native authority owner.
It requires the host's `CENTRAL_NATIVE_TOKEN`, the exact Action grant, current
Project World, human-scoped native Principal and mandatory
`expected_authority_revision`. The Agent executor/session remains Agent in the
Source writer and horizon. The separately recorded authorizing principal is
credential-derived, not a caller-supplied actor. It proves neither physical
human presence, H, Recognition nor adoption of a product position.

For this mode, `accepted_by_ref` must match the authenticated principal. Source
binding, exact basis bytes and revision must match the immutable proposal. Only
ordinary Project human-source aperture companions qualify; dedicated native
Day, NOW, contribution, policy and authority owners remain protected. Root
Control sources are not added to this mode. Source standing, provenance, roles,
treatment, IDs and content are not normalized or adopted by the operation.

Use `return_read` with `acceptance: commissioned-maintenance` to obtain the
recognized authority Source ref/revision. This explicit read exposes no grant or
credential and does not assert acceptance availability. Old reads remain
unchanged. Explicit `return` selection with `acceptance: commissioned-maintenance` creates
`central.source-return/v2` and `central.source-return-reading/v2`, capturing
`basis_source`. New readers preserve v1 and v2; the old Source Return owner
rejects v2 before proposal/status mutation, so it cannot discard the new facts. Default collaborative proposals
remain v1 with their original wire shape. Legacy v1 proposals stay readable but
require a fresh explicitly selected v2 proposal for this mode. Optional historical
`maintenance_authorization` facts on the existing record cannot reauthorize a
write. A failed maintenance intent cannot switch to declared collaborative
acceptance on the same Return; that requires a fresh exact proposal. Current
authentication is repeated under existing root/Project mutation
locks before publication. No new authority store or public Source writer bypass
is introduced.

The current personal authority Source has no exact Source Return acceptance
grant. Source capability and its native credential adoption are distinct: this
implementation installs no grant and does not apply the pending six accepted
Main companions. The existing grant schema bounds actions/Projects/expiry,
not individual SourceRefs. No six-file restriction may be inferred from a
Project-wide credential. The owner must settle the exact native authorization
before operative protected-source delivery.

The optional legacy personal extension's Control proposals still capture a
source basis. Its missing native authority route is not closed by this
Project-only variant; it is not registered in the default `ctrl` CLI.

## Interruption and errors

Proposal records publish through fsync and atomic rename. Acceptance stores an
applying record before native publication. A later owner call reconciles the
current revision against basis and proposed target: unchanged basis restores
pending state; target records acceptance; a third revision reports unresolved
interruption and requires recovery rather than a blind resend. Resource errors
after possible publication do not falsely claim source was unchanged.

Run real acceptance with the explicit candidate `CTRL_BIN`:

```sh
CTRL_BIN="$PWD/target/debug/ctrl" python3 ctrl/tests/native/source_returns.py
```

Native tests cover retrieval refusal, returned-work
persistence, conflict, explicit collaborative application and native provenance,
rejection, paging and protected human-source refusal. Existing source authority
and Control tests remain required. Desktop/native-human acceptance remains
separate from this owner candidate.

## Durable proposal publication failures

The existing Return owner uses rooted native physical publication for its
proposal records: a new native Return identity is exclusive, existing status
updates preserve admitted operational metadata, and the legacy `record-staging`
name is untouched. ReturnRef, basis/proposed content, SourceRef, attribution,
schema and the target/basis/third-revision recovery rules remain unchanged.
No receipt lookup or inclusion recovery is overloaded onto this legacy vehicle.
The v2 maintenance record adds captured binding and historical authorization
facts as described above; the old v1 record layout remains unchanged.

A record failure after actual rename preserves typed `published:true` and the
original IO cause. A successful native Source write followed by accepted-record
failure preserves its actual SourceRef/revision and existing ReturnRef as scalar
invocation observations, never a copied source result/body or fabricated current
receipt. Recovery that observes the intended target likewise retains that actual
observation if recording it fails. A failed Source write followed by failed
current read/status recording retains both actual original and supplemental
errors rather than replacing the original failure.

The native Action reports `partial_completion` / `central.publication_uncertain`
for these observed incomplete owner phases. Read the same native Return/current
Source after restoring the physical route; do not automatically issue another
proposal or acceptance. Existing human-source authority refusal remains intact.
JSON-escaped proposals above 8 MiB remain readable within the owner's existing
reader allowance; no Wiki body budget is applied to this record format. Added
real owner/CLI/OS definitions need composed Source qualification and do not
constitute installed or human acceptance.

## Qualification scope for commissioned maintenance

The existing `ctrl/tests/native/source_returns.py` retains every prior native
check and adds actual controlled recognized authority and two native Projects
with six CSV/MD/HTML carriers. It checks missing/wrong/expired/Agent/stale native
credentials, exact Project/binding/revision/bytes, Agent execution versus native
principal, dedicated-owner refusal, actual readonly and nonroot parent IO
failure, fresh-process reads and no repeat application. These are test fixtures,
not personal authorization or the six Main bodies. Deliberately seeded retained
applying states characterize target/third-revision recovery; they are not claims
of an observed crash or post-rename IO. Existing genuine publication uncertainty
and Source/Receiving suites remain required. No added gate has executed at this
Source-candidate stage.

The previous native Central Rust owner rejects a non-v1 record in `load`, before
applying recovery, rejection or status save. The tracked Python file is a CLI
adapter to that owner; it has no separate schema guard or record authority.
The current O:I typed Source Return consumer checks v1 schemas and intentionally
refuses selected v2 readings/mutations. Its default v1 calls remain useful; this
candidate does not silently migrate that public consumer or permit it to erase
maintenance facts. An explicit consumer migration is a separate source scope.

For executed downgrade characterization, qualify the previous native image and
its Source/lock basis, then select the existing native test with
`SOURCE_RETURN_REQUIRE_CUTOVER=1` and `SOURCE_RETURN_PREVIOUS_CTRL_BIN` alongside
current `CTRL_BIN`. The real previous owner's read, reject and accept must each
refuse the native-created v2 record without changing any record/source bytes.
Without this explicit prerequisite, the test reports `executed_total: 0` and
`pass_credit: 0` for that comparison. Paths alone do not qualify either image.
Malformed operational fixture checks exercise refusal before rejection/recovery
status writes; they are not crash or publication-failure observations.
