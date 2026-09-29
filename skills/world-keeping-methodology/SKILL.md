---
name: world-keeping-methodology
description: "METHODOLOGY: World-keeping — where session work lands and how a field stays clean: one register per session with its NOW field, returns bounded and attributed pointing at canonical plans, durable knowledge travelling to wikis through the promotion door, and the day closing in order so tomorrow opens clean. Use when starting or ending a session, placing work or records, promoting a learning, or closing a day. A task-local session whose threads all closed in place selects none of this and loads no register."
---

# World-Keeping Field

This Methodology orients an Agent inside the world-keeping field: where
session work lands, how it returns, and how a field closes so it can open
clean. Two registers, one law. It orients and points; the durable statements
named in [Source basis](#source-basis) carry the law.

## Session work has a place

Work that has nowhere to land lands everywhere.

- Every session stands in one of two registers, and each keeps a NOW field:

```text
project session        Work/<Name>/ProjectCentral/now/     via projectcentral.now.*
root / cross-project   Control/agents/now/                 via central.now.* (root scope = absent project)
```

- Decide by concern, not by where the session happened to start: work inside
  a project's concern belongs to that project's field even when the session
  began elsewhere; cross-project, suite-level and world-keeping work belongs
  to the root field.
- The Work root is an index of projects, not a scratchpad. A dated folder at
  the Work root, a draft parked beside the projects, an outcome living only
  in a finished chat — each is the same fault: a horizon with no field. When
  you find one, account for it in the NOW field of its register and propose
  its placement; do not move it silently, and do not add to it.

## Returns are bounded and attributed

- A session return states: what was done, what it means, what remains open,
  where the durable evidence lives. It is not a transcript and not a report
  that pretends to be source.
- It points at the canonical plan it serves — ticket, map, plan file or PR —
  through source refs, and instructs the follow-up agent to read that plan
  and execute from it. The record is a pointer, never a replacement. The
  record contract and its writing budgets live in the ledger skill, not in
  law.
- Durable material — a learning, a decision, a convention — does not stay in
  NOW; it returns to its owner through the promotion path, and NOW keeps only
  the ref.
- A return or flow given no explicit name still gets a readable one, derived
  from content, never an opaque timestamp: `<slug>-<local civil date>` for
  returns, kebab-cased from the subject; `<slug>-YYYY-MM-DD-HHMM.md` for
  flows, kebab-cased from the title. Local civil time only.
- One implementation serves both registers — the ctrl native Actions, where
  root scope is the absent `project` argument. No third variant.

## One wiki per register; the return is the only door

- The root wiki (`Control/agents/wiki/wiki.json`) holds cross-project and
  personal cognition; each project keeps one wiki at
  `ProjectCentral/agents/wiki/wiki.json`. Two registers, no third field.
- A wiki is agent-maintained knowledge, never source, and is never edited
  directly at either register. What a session learned that deserves wiki
  standing travels as a return: NOW promotion writes it under
  `agents/wiki/returns/**` with its lineage already stamped, and the wiki
  owner's procedure (`aikit wiki ...`) does the incorporation and cleanup.
  The return is the only door.
- A project's documents — its README, its docs tree, its AGENTS file — are
  that project's ground inside its repository. Work on them happens in that
  repository, to that repository's standard. A wiki holds knowledge about the
  work; it does not hold the work.
- The root wiki does not aggregate the project wikis. Cross-project resonance
  lives at the root with its own provenance; a project's self-knowledge is
  not lifted out of its project. Each wiki discloses its register.
- Cleaning the field is the wiki owner's maintenance, run from evidence, with
  provenance — not a session's side errand, and never silent deletion.

## The day closes, in this order

A moving horizon that never closes becomes a junk drawer. NOW is useful
because it ends: what was live is carried, what finished is removed, and what
the day held stays readable in a dated record that later edits cannot
rewrite.

- DAY is a closure reading at a local civil date the human supplies or you
  read from the field — never a timezone guess, never a scheduler's opinion.
- The order is the law:

```text
read the field            human scratch, agent returns, open promotions
classify                  carry what is live; release what resolved, expired or promoted
snapshot first            the day keeps byte-exact copies of what it closed over
derive the reading        the dated record is written from the snapshot, before anything is cleaned
then clean                carried records stay at their stable paths and gain the day in their lineage;
                          released records leave the moving field unless a preserve ref protects them
```

- Human scratch is the human's: the close copies it into the day's sources
  and leaves the live copy untouched — protection from cleanup and rewrite,
  never an assignment of writing. Returns are attributed and bounded: moved
  to their day's keeping or released, never quietly deleted.
- The dated reading is an index: it points at retained material rather than
  retelling it.
- A carried record needs a live reason and a next review condition — age
  triggers review, never silent self-renewal, never deletion.
- A day that cannot close honestly says so and stops before it cleans: a
  partial close that names its failure beats a clean claim that lied.
- Commands: at the project register the close is `projectcentral.now.rollover`;
  at the root register it is `central.day.lifecycle`, and the day opens with
  `central.day.ensure` under the recognised civil-time policy. Same law at
  both registers.

## Source basis

All sources are authored governance ground in the personal world root
(`~/Central`); paths are relative to it.

- `Control/agents/governance/field-and-now/session-work-placement.md` — two registers, NOW fields, bounded returns, placement faults.
- `Control/agents/governance/field-and-now/wiki-field-law.md` — one wiki per register, the return as the only door.
- `Control/agents/governance/field-and-now/day-close.md` — the close order: read, classify, snapshot, derive, clean.

These files are the law. This Methodology makes them loadable; it does not
replace them.
