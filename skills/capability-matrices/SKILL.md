---
name: capability-matrices
description: Author, reconcile and validate capability matrices alongside QL product accounts, including arbitrary named-axis views and the suite product seed-by-field profile. Use for capability CSV/Markdown changes and their governing HTML links.
---

# Capability matrices

Read the [matrix protocol](../../docs/CAPABILITY-MATRIX-PROTOCOL.md) before changing the carrier or defining a view. It is the single contract for product and QL-MEF matrices.

Recover the need, native operation, useful result and governing account passage from current sources. Retain the capability's identity as it contributes to several cells. Attribute inferred mappings. Empty cells are unassessed; do not fabricate powers to fill a grid. Read implementation and test definitions separately from actual execution evidence.

For native products, start at the HTML's six-answer 0/1 overview. The default view crosses those six seeds with S and the five other products. General views declare their own ordered axes; sixfold cardinality alone does not establish a QL shape. Use [QL foundations](../../../Quaternal-Logic/skills/ql-foundations/SKILL.md) when declaring shape, lens, conjugation or return semantics.

Maintain one CSV contract, preserve unknown fields and source annotations, and reconcile readers when converting existing carriers. Keep standing, implementation, relational coverage and lifecycle separate. A capability belongs in its native product account; a composed S experience links to it.

## Maintained templates

- [CSV header](assets/matrix.csv): use the same core columns in every view.
- [Manifest schema](assets/matrix.schema.json): explicit whole, view and axis declarations.
- [HTML collection template](../../../ai-kit/registry/capsules/skill/aikit/html-account/payload/full-account-template.html): use the seed-led product mode in the adjacent HTML skill; preserve the actual human seed.
- [Seed-led HTML reference](../../ProjectCentral/user/central.html): the working 0/1 overview plus six expansions. Reuse its structure with the target product’s own sourced content; the generic HTML template alone does not contain this opt-in seed layout.
- [Central manifest](../../ProjectCentral/user/capability-matrix.json), [CSV](../../ProjectCentral/user/capability-matrix.csv) and [readable account](../../ProjectCentral/user/capability-matrix.md): concrete source-backed use of the product profile, not reusable claims for another product.

Use [docs-methodology](../docs-methodology/SKILL.md) for the governing prose. Update affected HTML passages and CSV records coherently from exact source revisions, then refresh the Markdown CSV appendix. Run the product checker with the appropriate repository, HTML filename, product index and namespace. Inspect the readable grid and follow the changed capability back to its source. Structural acceptance and human adoption are separate.

For population, contemplation or retirement, route to AIKit's [project reflection protocol](../../../ai-kit/docs/v2/21-PROJECT-REFLECTION-AND-LOCAL-ARTICULATION.md). Central owns retirement's filesystem structure; AIKit owns the operations. Keep unresolved source meaning available through consolidation and preserve exact retrieval when successors are established.

## Continuous product maintenance

Recover the current native CLI inventory before mapping commands. Declare its read-only discovery route in the manifest and exact command identities in each capability’s `cli_commands` extension. Record the actual exposure boundary, update date, execution ticket/session/wayfinder refs and inspected code hashes. Read the protocol’s CLI-maintenance section for these fields and the CI contract.

When a seed or capability changes, use `tools/reconcile_product_ground.py plan` with an explicit HTML-to-CSV or CSV-to-HTML direction. Read and refine the affected expanded sections, regenerate the final plan, then apply with the reviewed sections and actual change reference. Hashes record that review; do not refresh them to silence a check without reading the changed source. The transaction receipt supports exact recovery while protecting later edits.

Run `tools/product_maintenance.py` against the built native CLI and actual source. Use the PR comparison base to find newly changed code outside the matrix. Update the capability, command exposure, evidence and linked account together. Maintain the same deterministic CI bundle across the six product repositories through `tools/package_product_ground.py`; never hand-edit generated bundles.


## Jev-assisted matrix readings

AIKit may ask its general Jev capability typed questions over an explicitly selected matrix view. Build that question from the view's own question, ordered axis meanings, governing account passages and the selected capabilities' need/operation/outcome/source standing. For full-scope work, account for every declared member before selection; search hits are not a substitute for the inventory.

Do not force one winning capability when several contributions are jointly required. Retain a legitimate “catalogue insufficient for this need” determination. Keep the returned model/version, question basis, source revisions, usage and evidence standing with the invocation receipt. A repeated warm NOW read does not justify another Jev call unless a relevant semantic source, dependency, Return or selected practice changed.

If the reading warrants a matrix update, apply it through the normal CSV/companion reconciliation path above. The classifier can expose pressure on a relation; it does not change the matrix, source standing, implementation status or human-authored product position by itself.

### Situate the concern before asking

A Jev reading is only as good as the selection you give it. Gather first, then ask a few precise questions; never send whole matrices or unrelated documents.

Run this step at two moments:

- **Planning (forward)**, before a change or while charting a wayfinder map: which cells and capabilities does the intended work touch, what must be tested, and is the catalogue sufficient for the need?
- **After CI or a merge (returning)**, alongside `tools/product_maintenance.py --base <PR base>`: what did the change do to which cells, capabilities and evidence, and what now needs updating?

1. **Name the concern.** State the feature, product domain and UX concern in a sentence. Name the telos goal/track it serves: `Control/user/telos/<goal>/` at the root, `ProjectCentral/user/telos/` per product.
2. **Code.** Take the changed paths from `git diff --name-only <base>..<head>`. Read derived structure through the GitNexus lens: `aikit knowledge code changes --repo <repo> --scope compare --base-ref <base>`, then `aikit knowledge code impact --repo <repo> --symbol <S> --file <F>` (or `aikit knowledge code context` with the same flags) for the symbols that matter. `aikit now-context field --repo <repo> --base <base> --projectcentral <ProjectCentral>` gives the explicit changed-path → capability (`code_refs`) → `test_refs` joins with revisions in one reading. GitNexus output is derived; it never outranks authored source.
3. **Documents.** Read the governing account passages (`account_ref`), the product's docs and the wiki (`aikit wiki query search --file <wiki.json> "<terms>"`). Pull related concepts from the source pool through `aikit knowledge search`.
4. **Select from the matrices.** For each product the concern touches, open its matrix (telos folder first) and pick the view whose question fits:
   - the product-field view crosses the product's six seed questions with S and the other products;
   - `suite-relations` carries the product↔product readings.

   Select the affected cells and the capability rows they reference, with all fields. Check the same capabilities in O:I's suite matrix (`suite/capability-matrix.json`, the `capability-matrix` Method) for their cross-product standing.
5. **Ask Jev a few questions** over exactly that selection. The state holds the view's question and axis meanings, the selected cells and rows, and the account passages. The questions are two to five precise determinations, for example:
   - which selected cells does this change;
   - is the catalogue sufficient for this need;
   - which implicated capability's evidence is stale after this merge.

   Write a request file and run `aikit --json jev invoke --request-file <request.json> --limits-file <limits.json> --credential-ref <ref>`. Read the provider outcome in `data.answer` and `data.attempts`, not the envelope's `ok`. Keep the request under the provider's ~32k-token input ceiling; over it the provider refuses with an opaque HTTP 400.
6. **Read the answers yourself** against what you gathered. Act through this skill's normal path: a reconciled record, a test to run, a wayfinder ticket, or a "catalogue insufficient" note on the relevant cell. Keep the invocation receipt with the change.
