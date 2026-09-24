# Central — Agent governance root

This is Central's ProjectCentral Agent governance root (`agents/governance`),
the place the repo covenant expects `repo-structure.md` (where things go) and
`repo-content.md` (what artefacts should be like). Those two sources have not
been written for Central yet; until they are, the repository's standing
guidance lives where it already is:

- structure and product ground: [`README.md`](../../../README.md),
  [`docs/PRODUCT-GROUND-CONVENTION.md`](../../../docs/PRODUCT-GROUND-CONVENTION.md),
  [`docs/PROJECTCENTRAL-CONTRACT.md`](../../../docs/PROJECTCENTRAL-CONTRACT.md)
- content and gates: [`docs/CLI-REFERENCE.md`](../../../docs/CLI-REFERENCE.md),
  [`.github/workflows/verify.yml`](../../../.github/workflows/verify.yml)

The directory must exist for `projectcentral.doctor` to validate Central's own
ProjectCentral; without it every `projectcentral.*` Action — including the
Project Day close — refused with `project_central_incomplete`.
