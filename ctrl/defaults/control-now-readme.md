# Control/agents/now — the root NOW/DAY field

**Status:** distributed default — becomes human source on adoption

## One law, two registers

This field is the root register of the same temporal contract the projects run.
The law is the ProjectCentral NOW/DAY contract (`docs/PROJECTCENTRAL-NOW.md` in
the Central product repository); the shape and the policy are identical:

```text
Control/agents/now/
├── user/            free human scratch / current source (human-owned; rollover copies, never cleans)
├── agents/          attributed bounded agent returns (central.project-now.handoff/v1)
├── day/
│   ├── YYYY-MM-DD.md          derived dated closure reading
│   └── YYYY-MM-DD.sources/    byte-preserving source state at close (user/** + agents/**)
├── policy.json      central.project-now.policy/v1 — identical defaults to the project policy
└── promotions.json  central.project-now.promotions/v1 — current promotion receipts
```

`NOW` is the moving current working horizon of the **world** (cross-project,
suite-level, personal), not of any one project. Project horizons live in their
projects. A session works one register at a time; material that belongs to a
project returns through that project's field even when the session started at
the root.

## Execution standing (honest)

- `projectcentral.now.*` ctrl Actions remain **canonical and project-scoped**;
  they resolve `Work/<project>` only. Nothing here pretends otherwise.
- This stamp lays down the field's shape and policy only; it ships no executor.
  Until Central ships a native root NOW action, the root field's lifecycle
  (return / update / promote / rollover) is executed by the mirroring
  procedure — the `central-session-strap` skill — which arrives through the
  O:I guardian projection, not through this stamp. A fresh tree without that
  projection has the field but no operator for it; nothing here hides that.
- The mirroring procedure follows the ctrl semantics exactly: same envelope,
  same policy classification, same rollover order (inspect → classify →
  snapshot → derive → carry/clean → clear promotions ledger), same
  preserve-ref protection, same partial-failure honesty.
- When Central ships a native root NOW action, this README's execution section
  retires and the Action becomes the only door. The field's bytes will already
  conform.

## Boundaries

- Durable Control content is still human-authored or explicitly adopted
  (propose-not-write). NOW is temporal presence; it grants no authorship.
- Promotion targets from here: `Control/user/**` (human acceptance required)
  and `Control/agents/wiki/returns/**` (the wiki owner path). Never
  `Control/agents/wiki/wiki.json` directly.
- The root wiki does not aggregate project wikis; cross-project knowledge
  returns here with its own provenance (see
  [field-and-now governance](../governance/field-and-now/)).
