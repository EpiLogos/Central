---
name: vision-authoring
description: "Write or revise a product Vision as a standalone HTML account — why it exists, what it is, the intended experience, deliberate distinctions and direction. Use when product meaning itself is being set or changed. Not for experience detail (design-authoring), structure (architecture-authoring) or proving what is implemented."
---

# Vision authoring

A Vision owns **why** the product exists, **what** it is, the **intended
experience**, the **distinctions** that are deliberate, and the **direction**
of development. It never proves implementation: a Vision may say what should
be true; only code and evidence say what is.

## Form

Start from [assets/vision-template.html](assets/vision-template.html). It is
derived from AIKit's maintained account template
(`skill/aikit/html-account`, `full-account-template.html`; the source sha256
is recorded in the template's provenance JSON) and keeps that template's CSS
tokens, shell and script unchanged. Do not restyle it into a separate design
system; when the AIKit template changes, regenerate from it.

The template opens with a **0/1 overview**: six short answers (Why, What, How,
Who/Whereby, Where/When, Why-For), each opening an expanded surface — the
pattern of Central's seed-led account `ProjectCentral/user/central.html`.
Write the overview first; it must read on its own.

For how to compose a deep account, compose `skill/aikit/html-account` and
`skill/aikit/structured-account-authoring`; for recovering existing meaning
before writing, `skill/aikit/product-understanding`.

## Rules

1. **Recover before writing.** Read the authored ground (founding positions,
   seed answers, the human's own words) before drafting. Preserve supplied
   meaning; mark editorial changes.
2. **Standing is explicit.** Set `standing` in the provenance JSON and
   `data-standing` on each section. Agent drafts are `agent-inference` until a
   human recognises them; human-supplied answers keep
   `authored-human-position` with the clarification attributed.
3. **Stable unit ids.** Every section `id` is a stable address that Design,
   Mockup states and capability records (`extensions.documentation.vision_refs`)
   may cite. Do not rename ids casually; supersede instead.
4. **No implementation claims.** No "the system does X" without an evidence
   link; say "should" or link the capability record.
5. **Leave open what is open.** Unknown answers stay visibly unresolved.
6. **Hand off to Design.** The Direction surface names which determinations
   Design must take up.

## Verification

- The file opens standalone (no external script `src`), the 0/1 overview reads
  without the expanded surfaces, and every cited unit id exists.
- Standing is stated on every section.
- Run `python3 tools/documentation_inventory.py <path>` and check the reported
  role is `vision` and the unit ids are the ones you intended.

## Return

A changed Vision is authored meaning: return it as a proposal for human
Recognition, naming the Design units and capabilities it pressures.
