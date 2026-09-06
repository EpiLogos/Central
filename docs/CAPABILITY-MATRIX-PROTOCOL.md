# Capability matrices and product accounts

Protocol: `ql-capability-matrix/1`. Standing: design commitment for this documentation consolidation; generated mappings retain their own attribution. Central maintains the documentation convention and its filesystem homes. QL owns the shapes and lenses it uses. AIKit owns interpretation, contemplation, staging, wiki population and document lifecycle operations.

A capability states what becomes possible for someone: the need, the operation and the useful result. A matrix asks how that capability contributes within a disclosed field. The HTML account supplies the purpose, intended experience and governing decisions. Its compact 0/1 overview remains readable on its own.

## One form, explicitly named views

Each collection has a UTF-8 CSV and a JSON manifest. Product collections also have a readable Markdown account with stable capability anchors and an exact CSV appendix. All matrices use this protocol, including the existing QL-MEF matrices. A different axis set is a view declaration within the same form. Migration reconciles the actual records and their consumers; a second format is not an accepted end state.

The manifest declares `protocol`, `matrix_id`, `anchor_ref`, `default_view` and `views`. Each view has an `id`, `title`, `semantics`, `row_axis` and `column_axis`. An axis has an `id`, `label` and ordered `members`; each member has an `id`, `label` and, where available, `source_ref`. Identifiers are stable; labels and ordering are editable. Member identifiers are local to their axis. Every view states the question its intersections ask and what an empty intersection means.

`1` accounts for an ordinary item. `0/1` identifies an item opening a whole and its relations. Matrix members may refer to either. A matrix does not force an ordinary note to acquire internal positions. Arbitrary named axes are supported; QL positions, where declared, remain local 0–5 coordinates with explicit face and provenance. Optional `shape_ref`, `lens_ref`, `derivation_ref`, pair-family and return metadata must name an actual QL construction. Six rows and six columns alone do not establish the native direct × conjugate shape. A declaration of `ql:shape:1.0.0:6x6:direct-conjugate` requires a `derivation_ref` and, for every member, `source_ref` plus `ql_coordinate` containing integer `position` 0–5 and `face` (`direct` for rows, `conjugate` for columns). Each face contains every position once. Structural checks verify this declaration; the referenced derivation and semantic reading still require QL/source verification. See [QL foundations](../../Quaternal-Logic/skills/ql-foundations/SKILL.md) and [the native shape contract](../../Quaternal-Logic/fixtures/kernel/ql-shape-contract-v1.json).

## CSV records

The maintained header template is [matrix.csv](../skills/capability-matrices/assets/matrix.csv). The core columns below occur in order; additional named columns are permitted and must survive editing and round trips. The template also appends the product `question` column.

| Field | Meaning |
| --- | --- |
| `id` | Stable identity of a capability or relational determination. |
| `record_type` | `capability` or `relation`. |
| `view_id`, `row_id`, `column_id` | Address of a relation in one declared view. Empty for a capability. |
| `capability_refs` | JSON array of capabilities contributing at this address; `[]` when no capability is asserted. |
| `need`, `operation`, `outcome` | Concrete functional account. Required for capabilities. |
| `implementation_status` | Bounded implementation reading, including explicit intent-only or unresolved status. |
| `standing` | Standing of this determination, independently of implementation and lifecycle. |
| `source_refs`, `code_refs`, `test_refs` | Semicolon-separated source and evidence references. Product capability paths are relative to the owning repository. A test definition alone is not an executed observation. |
| `account_ref` | Governing account unit; product paths are relative to the matrix directory. |
| `relation` | Plain statement of what this placement asserts. Required for relations. |
| `coverage` | The declared relational reading, distinct from readiness. Existing H/S/L/W/I readings retain their meanings in their view. |
| `extensions` | JSON object retaining source annotations and additional typed information, including provenance, source revisions, readiness, lens readings and migration mappings. |

A capability has one identity independent of its placements. A cell can contain no assertion, one determination or several. Several cells can reference the same capability. A relation can pose a sourced question without asserting an implemented capability. Empty means unassessed unless the view declares a narrower meaning; it never means complete or impossible. Contradictory readings remain individually attributable and explicitly related until resolved.

Every capability retains its need, operation, outcome, actual standing, status, source and account links. Implemented product claims also carry code and test references. Missing implementation for an intended capability is recorded explicitly, with empty evidence fields. Evidence must never be generated to satisfy a column. Preserve unknown extension fields and original source annotations during conversion; retain their namespaces and meaning rather than coercing them into a QL coordinate or readiness score.

The product profile appends a readable `question` column. Its six seed-carrying S records hold the editable question text alongside the answer in `relation`; the other records leave this field empty. Reconciliation maintains the manifest’s row labels from these questions.

## Native product view

For each S0–S5 product, `product-field` is the default view:

- Rows `q0`–`q5` reference that product's six seed answers in the HTML 0/1 overview. Their expansions are the corresponding #0–#5 account sections.
- Columns are `S`, then the other five native products in numeric order. The product itself is represented by the row axis and is excluded from the columns.
- An intersection asks: how does this aspect of the product contribute to, depend on, or receive a return from this member of the field? The relation names the direction and native responsibility explicitly.

This is a seed × field-contribution view. It is not automatically QL's direct × conjugate shape. Product and technical-stratum namespaces stay distinct, even when both use labels such as S0. Existing suite direct/conjugate readings remain inspectable as named views in this same contract and are reconciled against their own sources. They do not substitute for the product's default view.

The S whole will use the same protocol when its account is reviewed: CLI routing/composition refers to native capabilities, while the desktop's versioned experience relates its own feature claims to those native powers. This protocol does not invent a second copy of each native capability at S.

## Human editing and coherent changes

Open on the product's 6×6 grid, with readable seed questions and product names. Each cell lists linked capability names and its actual relation; an empty cell says unassessed. Selecting a capability exposes its functional account, implementation reading, source and governing HTML passage. A record view exposes all CSV fields and additional views. IDs remain available for linking without displacing human labels.

CSV is the editable interchange. Use normal quoted CSV fields for commas, newlines and Unicode; use JSON arrays/objects in their named columns. A CSV editor must preserve unknown columns, JSON values and stable IDs, show changes before saving, and reject stale source revisions rather than overwrite them. A readable Markdown grid and index provide the first human inspection surface; a graphical editor must meet the same round-trip contract before it becomes an authoring surface.

For a change, recover the exact seed, account unit, capability and source revisions. Determine which claims are pressured. Edit the existing units and affected placements together, maintaining IDs and links. Validate the complete candidate before replacement. A multi-file operation must check all source bases before writing and support recovery from a partial write. A matching seed hash records the reviewed basis; it does not establish semantic agreement or adoption.

## Templates and validation

The [capability-matrices skill](../skills/capability-matrices/SKILL.md) collects the CSV header, manifest schema, existing HTML template and concrete conforming accounts. Use the actual product source to supply content; templates do not supply capability claims.

Run `python3 tools/check_product_ground.py` in Central for its account, or pass `--root`, `--account`, `--product-index` and `--namespace` for another product. Generic matrix validation is independent of that product profile. Validation checks declared addresses, identities, referenced capabilities and carrier integrity. Product validation additionally checks seed alignment, native field members, evidence links, HTML anchors and MD/CSV parity. Neither validator ratifies meaning or proves runtime behaviour.

## CLI discovery and continuous maintenance

The CLI supplies an independently executable inventory of public operations. Each native product declares `maintenance.cli` in its matrix manifest: an explicit argument array and either a JSON array selector (`format: json`, `items_path`, optional `id_field`), recursive Clap help (`clap-help`), or a flat help command list (`help`). CI builds the current executable before reading this inventory. Context-dependent resources, such as AIKit’s brokered capability list, remain separate from its static CLI command tree.

Every capability carries these extensions:

- `cli_commands`: an array of exact discovered command identities. One capability may use several commands; several capabilities may compose the same command.
- `cli_exposure`: a `kind` (`direct`, `composed`, `library`, `intent` or `gap`) and a concrete `reason`. Direct access needs a mapped command. Library facilities and intentions retain their actual access boundary. Existing exposure gaps stay visible in the check report.
- `maintenance`: `updated_at` in ISO date form, `change_refs` linking the execution ticket/session/wayfinder, and `code_basis` mapping each native code reference to its reviewed SHA-256 revision.

The manifest also declares `runtime_paths` and any narrow `runtime_excludes`. Against a pull-request base, every changed runtime source must belong to a capability’s code references. Adding a command without a capability mapping fails. Naming a removed command fails. Changing referenced code without reconciling the capability’s evidence basis fails. Code hashes record the revision reviewed; they do not certify that the prose is true. New functionality is reviewed for its actual CLI access, intended result and evidence alongside the implementation.

The maintained check is `python3 tools/product_maintenance.py --root . --account central.html --namespace central --product-index 0 --base HEAD`. CI uses the PR base rather than `HEAD`. It also checks seed/CSV agreement, reviewed matrix records, HTML source relationships, the derived CLI catalogue and the Markdown appendix. Reconciliation refreshes the CLI table between its `cli-catalog` markers from the same capability records. Other products supply their own account, namespace and index.

Every repository carries `.github/workflows/product-ground.yml` and a reproducible `.github/product-ground.pyz` tool bundle. Central owns the source tools. `tools/package_product_ground.py` rebuilds the bundle; `--check` detects drift from its maintained source. Export the same bundle to the other products when the shared protocol/tool changes. This keeps CI runnable from one product checkout without granting cross-repository credentials or downloading mutable tool code during a build. The bundle records the hash of every included module.

CI uses `--reference-scope repository`: local source and unit links are checked, while external repository/ticket sources retain their references for the full workspace audit. A repository-only result does not claim those external sources were fetched or verified. Run the default workspace scope during suite harmonisation. These workflow files must be published before hosted CI can execute them; repository branch rules determine whether their status is required for merging.

## Reviewing and propagating question changes

Both the HTML question/answer and the CSV S-column `question`/`relation` fields are editable. The visible HTML question has a `data-seed-question` span keyed to its seed; the answer has `data-seed-content`. Descriptive navigation labels remain separate from these questions. Choose the direction explicitly for a change. The tools do not select authority by modification time. Expanded prose is reviewed by the human/Agent against the changed meaning; automatic propagation updates the agreed source representation, links, readable grid and basis records.

From Central, create a bounded plan:

```sh
python3 tools/reconcile_product_ground.py plan --root . --account central.html --namespace central --direction html-to-csv --out /tmp/central-ground-plan.json
```

The plan names changed records, exact companion file bases and affected expanded sections. Changes to a capability also trace back through its governing account and explicit capability links. Review and refine those sections first, then regenerate the plan against the final source. For a reviewed #1 change:

```sh
python3 tools/reconcile_product_ground.py apply /tmp/central-ground-plan.json --reviewed q1 --change-ref codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5
```

Use `csv-to-html` when the CSV/manifest supplied the intended edit. Supply the actual execution reference for subsequent work. An initial harmonisation may require all six sections; later plans narrow the review to actual affected sections. Applying a plan records the review, preserves unknown CSV fields and refreshes the Markdown grid/appendix and HTML matrix basis. It preserves capability identities and source standing.

Stale plans fail before writing. Transactions retain exact before/after companions and a receipt under `.central/documentation-transactions/`. An interrupted multi-file write stays visibly incomplete. `restore` accepts that receipt and restores the exact prior files only while no later edits would be overwritten. This is a documentation maintenance helper, not an implementation of AIKit’s wiki population or document-retirement operations.

Acceptance exercises real copied source files: HTML-to-CSV and CSV-to-HTML changes, changed question labels, scoped review, stale edits, unknown-field round trips and guarded restoration. Native CLI discovery and code-drift tests use actual executables and source files. The maintenance checks establish a reviewable loop; interpreting intent and assessing semantic agreement remain part of the harmonisation work.

## Wiki population and document retirement

AIKit's [project reflection protocol](../../ai-kit/docs/v2/21-PROJECT-REFLECTION-AND-LOCAL-ARTICULATION.md) owns the operational population and acceptance procedure. It must disclose ordinary notes, HTML units, CSV capabilities and Run sources under the same source policy, while retaining the extra structure each actually carries. Capability-specific contemplation produces attributable proposals against exact source revisions; acceptance follows the authority of the affected source.

Central owns the filesystem homes, source identity and structural relationships needed for retirement. AIKit owns assessment, consolidation, retirement, archival and restoration operations using those contracts. Retirement is a lifecycle relation, separate from standing. A retired source retains its identity, bytes, successor mapping and revision evidence; current retrieval can prefer its successor while exact and historical reads still resolve.

Before retirement, account for each selected source unit as retained, transferred, superseded, conflicting or unresolved against a real successor. Partial consolidation leaves remaining meaning available. Physical archival additionally requires identity-preserving redirects and inbound-link continuity. Existing `projectcentral.migrate` handles selected wiki collections; it is not the document retirement operation. These operational requirements remain acceptance work until AIKit implements and verifies them. This consolidation changes the selected matrix carriers in place and leaves broader document retirement for the subsequent source review.
