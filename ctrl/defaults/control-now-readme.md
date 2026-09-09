# Control/agents/now — the root NOW/DAY field

**Status:** distributed default — becomes human source on adoption

## One law, two registers

This field is the root register of the same temporal contract the projects run.
The law is the NOW/DAY architecture contract (`docs/PROJECTCENTRAL-NOW.md` in
the Central product repository); the shape and the policy are identical:

```text
Control/agents/now/
├── user/            free human scratch / current source (human-owned; rollover copies, never cleans)
├── agents/          attributed bounded agent returns (central.project-now.handoff/v1)
├── flows/           ordinary-file Flows of the root register
├── day/
│   ├── YYYY-MM-DD.md          derived dated closure reading
│   └── YYYY-MM-DD.sources/    byte-preserving source state at close (user/** + agents/** + flows/)
├── policy.json      central.project-now.policy/v1 — identical defaults to the project policy
└── promotions.json  central.project-now.promotions/v1 — current promotion receipts
```

`NOW` is the moving current working horizon of the **world** (cross-project,
suite-level, personal), not of any one project. Project horizons live in their
projects. A session works one register at a time; material that belongs to a
project returns through that project's field even when the session started at
the root.

## Execution standing (honest)

- `projectcentral.flow.*` ctrl Actions serve this register when `project` is
  omitted. Root Flows live here; their refs stamp `control:root`. That door is
  native.
- `projectcentral.now.*` ctrl Actions remain **canonical and project-scoped**;
  they resolve `Work/<project>` only. This stamp lays down the field's shape
  and policy; it ships no root NOW lifecycle executor. Until Central ships a
  native root NOW action, the root field's lifecycle (inspect / return / update /
  promote / rollover) is executed by the mirroring procedure — the
  `central-session-strap` skill — which arrives through the O:I guardian projection,
  not through this stamp. A fresh tree without that projection has the field
  but no operator for those lifecycle steps; nothing here hides that.
- The mirroring procedure follows the ctrl semantics exactly: same envelope,
  same policy classification, same rollover order (inspect → classify →
  snapshot → derive → carry/clean → clear promotions ledger), same
  preserve-ref protection, same partial-failure honesty.
- When Central ships a native root NOW action, this README's lifecycle gap
  retires and that Action becomes the only door for those steps. The field's
  bytes will already conform. Flow already uses the native door.

## Boundaries

- Durable Control content is still human-authored or explicitly adopted
  (propose-not-write). NOW is temporal presence; it grants no authorship.
- Promotion targets from here: `Control/user/**` (human acceptance required)
  and `Control/agents/wiki/returns/**` (the wiki owner path). Never
  `Control/agents/wiki/wiki.json` directly.
- The root wiki does not aggregate project wikis; cross-project knowledge
  returns here with its own provenance (see
  [field-and-now governance](../governance/field-and-now/)).
