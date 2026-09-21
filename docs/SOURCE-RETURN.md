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
| `projectcentral.source.return` | `source_ref`, `expected_revision`, `proposed_content`, `reason`, `agent_session_ref`, optional `evidence_refs` | `central.source-return-reading/v1` |
| `projectcentral.source.returns` | optional `limit` (1–100), exclusive `before` cursor | bounded `central.source-returns/v1` metadata |
| `projectcentral.source.return_read` | `return_ref` | proposal, current source, basis comparison, acceptance availability |
| `projectcentral.source.return_accept` | `return_ref`, `expected_revision`, `acceptance: human-accepted`, `accepted_by_ref` | accepted native owner receipt or conflict/refusal |
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

## Human authority remains an explicit integration obligation

`ActionExecutionContext` currently has roots, connectors and platform context,
but no attested human principal. `human-accepted` and `accepted_by_ref` are explicit
caller declarations, not grants of human authority. Native source binding rules
therefore decide applicability. Collaborative working sources can use their
existing agent mutation authority. A returned change to authored human ground is
retained and disclosed but acceptance returns `unavailable_capability`; it cannot
be applied by relabeling the returning agent as human. Its reading names that
missing native authority route.

The optional legacy personal extension's Control proposals now capture a source
basis. Missing-basis legacy proposals require a fresh proposal; concurrent source
edits cause a conflict. Control application refuses until a native human authority
route exists. The extension is not registered in the default `ctrl` CLI. This is
an explicit safety correction, not completion of human-ground Return acceptance.

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
