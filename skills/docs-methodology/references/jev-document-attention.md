# Jev document attention

How to ask Jev which documents an act needs, using the compact inventory from
`tools/documentation_inventory.py` as shared state. The shapes follow AIKit's
typed question contract (`crates/aikit-core/src/jev.rs`, `Question`):
`noul` (a probability that a statement is true, optional `criteria` for the
`true`/`false` outcomes only), `choice` (1–255 named options) and `score`
(2–10 ordered rubric levels). Requests hold 1–256 uniquely named questions,
stay under 1 MiB, and name an explicit `jev-` model. Question ids are
attribution only.

## State

Send the inventory records, not document bodies, plus the act:

```json
{
  "act": "Add a refusal message when a ledger record is rejected.",
  "method": "skill/central/ui-development",
  "inventory": { "schema": "central.documentation-inventory/v1", "records": ["…"] }
}
```

Records carry `source_ref`, `revision`, `role`, `standing`, `determination`,
`relations`, `capability_refs` and `unit_ids`. Keep the whole declared
inventory when the act is a full account or matrix scope; otherwise a
pre-filtered slice (by role, capability or path) is fine, provided dependencies
named in `relations` stay in.

## Per-source relevance — `choice`

One question per candidate source, id `relevance:<source_ref>`:

```json
{
  "type": "choice",
  "instructions": "For the act in state, how does the source <source_ref> (see its inventory record) participate?",
  "criteria": {
    "required": "The act cannot be done correctly without reading this source.",
    "supporting": "Useful context; the act can proceed without it.",
    "jointly-required": "Needed only together with another named source; alone it misleads.",
    "irrelevant": "Does not bear on this act."
  }
}
```

Read `choice`, `probabilities` and `confidence` together. A low-confidence
`required` is still read; a confident `irrelevant` is left unloaded.

## Joint requirement — `noul`

For each pair the choice answers mark `jointly-required`, confirm the pairing,
id `joint:<a>+<b>`:

```json
{
  "type": "noul",
  "instructions": "Must <a> and <b> be read together for this act (for example a Design state and the Architecture contract that owns it)?",
  "criteria": {
    "true": "Reading either alone gives a wrong or partial determination.",
    "false": "Each can be read independently."
  }
}
```

## Catalogue insufficiency — `noul`

Always ask once, id `catalogue:insufficient`:

```json
{
  "type": "noul",
  "instructions": "Is the inventory insufficient for this act — does the act need a determination no listed source makes?",
  "criteria": {
    "true": "A needed document, unit or capability is missing from the inventory.",
    "false": "The listed sources are enough, whatever their quality."
  }
}
```

A `true` answer is a finding: disclose the missing layer (do not manufacture
it), and route to reverse-recovery or a proposal. It never licenses inventing
a capability to fill the gap.

## Unit narrowing — `choice` (optional)

For a large required source, narrow to units, id `units:<source_ref>`:
`criteria` keyed by the record's `unit_ids` (up to 255), each describing that
unit by its heading. Retrieve only the chosen units.

## After the answer

1. Retrieve exactly the required and jointly-required grains at the recorded
   `revision`; retrieve supporting sources only if the act stalls.
2. Prepare the context in the participant NOW with `aikit now-context prepare`.
3. Reuse it while every selected revision is unchanged; re-ask when a source,
   dependency, capability record or Return changes.
4. The Jev result is evidence for what was read, never authorship of what was
   written.
