---
name: control-maintenance
description: Audit human-authored Central Control source, identify durable-content problems or missing preference areas, and propose reviewable changes without turning generated advice into authored truth.
---

# Control maintenance

Use this Skill when the user wants to audit, tidy, configure, review, or extend durable material under `Control/user/`, `Control/agents/`, or `Control/machines/`.

Control is **human-owned authored source**. The Skill is a reusable procedure around that source; it is not a hidden memory database and does not become Control content itself.

Read before acting:

1. `docs/CONTROL-CONTENT-PROTOCOL.md` in full where relevant, especially source classes, scope, durable change proposals, conflict/supersession, Control-maintenance Skills, and quality criteria.
2. `docs/CENTRAL-SYSTEM-SPEC.md` for the Control/derived-state boundary and canonical Action semantics.
3. The relevant live Control root through `control.open` / `control.search` or ordinary filesystem reads. Direct filesystem source is authoritative; do not require an import or personal-profile database.
4. Other Skills only when a finding genuinely belongs in reusable procedure rather than durable Control.

The governing flow is:

```text
current authored Control
        +
relevant dialogue / supporting context
        │
        ▼
classify findings at the correct scope
        │
        ▼
reviewable proposal + reasons + final diff
        │
        ▼
explicit human acceptance
        │
        ▼
optional durable Control mutation
```

## Procedure

### 1. Establish scope and source authority

Identify which Control roots are relevant to the current request:

```text
Control/user
    durable human self-description, interests, cross-context working preferences, decision criteria

Control/agents
    durable human-agent relationship, collaboration style, initiative, evidence/verification expectations

Control/machines
    portable machine roles and intended computing-environment state
```

Do not assume all three roots need a full audit every time. Use the narrowest scope that can answer the request.

Record the source paths actually inspected. Treat those files as authored source. Keep `.central/`, indexes, observations, caches, summaries, and other generated/local material separate even when they help locate or interpret source.

### 2. Inventory without inventing a schema

Read the relevant source directly. Natural prose is first-class. Structured formats are valid when their objects genuinely require structure.

Build a lightweight audit inventory containing, for each relevant source item:

- source path;
- Control root;
- apparent scope;
- durable subject or purpose;
- relevant neighbouring/overlapping sources;
- source class (`authored` for live Control source);
- whether the item needs deeper review.

Do not convert the tree into a mandatory universal profile schema merely to make the audit easier.

### 3. Apply the deletion and scope tests

For each reviewed item ask:

> If this item is removed, what useful future behavior, understanding, decision quality, or reproducibility changes?

Then ask:

> Is this information kept at the narrowest scope where it stays correct?

Use those questions to identify low-value or misplaced content without treating brevity as the goal.

### 4. Classify findings before proposing edits

Classify concrete findings. Do not collapse distinct problems into a generic “clean up” recommendation.

The audit classification field is:

```text
clean
    durable, correctly scoped, current authored material; no change required

stale
    once-useful authored material that no longer describes the current durable relation

duplicate
    materially repeated authored content whose repetition adds no useful scope or distinction

conflicting
    authored statements whose live meanings cannot both be followed as written

low-value
    retained content whose deletion test does not reveal a durable future benefit

misplaced
    useful content kept at the wrong scope or in the wrong architectural layer

procedure-candidate
    reusable steps/method that should move to a Skill or Action instead of persistent Control context

missing-durable-area
    a durable preference area is materially relevant to the current dialogue but absent or too unclear to guide future work
```

A finding must include evidence and a reason. Absence is not automatically a problem: only surface a `missing-durable-area` when the current dialogue makes the area relevant.

### 5. Handle conflicts and supersession explicitly

Never silently merge contradictory authored statements.

For a conflict, show:

- every conflicting source path;
- the relevant statement or compact paraphrase;
- each statement's apparent scope;
- why the statements conflict in practice;
- what decision the human needs to make.

The human resolves authored conflict. Git history can preserve superseded source; the live tree should make the current statement clear after acceptance.

### 6. Move reusable procedure out of persistent context

When source contains a repeatable method rather than a durable relation or preference, classify it as `procedure-candidate`.

Use this boundary:

```text
Control
    what the human durably wants, values, prefers, intends, or requires

Skill
    reusable agent procedure for accomplishing a class of work

Action
    canonical semantic operation exposed by Central or another application
```

Do not leave long operational recipes in Control merely because agents need them repeatedly. Propose the destination Skill/Action and preserve only any durable preference that motivates the procedure.

### 7. Surface relevant missing durable preference areas

During configuration or audit, the current conversation may reveal an area where future agents would materially benefit from a durable cross-context preference.

Examples can include communication style, initiative, decision criteria, preferred human-review boundaries, or verification/confidence expectations.

Do not turn a generic checklist of possible preferences into compulsory Control fields. Surface only areas that are relevant now, explain why they matter, and let the human decide whether to author anything.

### 8. Offer verification and confidence as an optional engineering-governance topic

When engineering practice is relevant to the dialogue, verification and confidence are an **optional topic**, not a required schema section.

Ask in substance:

> When an agent changes software for you, what normally gives you confidence that the work is complete? Do you have durable preferences about tests, CI, review, evidence, or when human review is required?

Listen for cross-project expectations such as:

- completion claims should be supported by executed evidence appropriate to the change;
- normal implementation work should preserve or improve existing assurance;
- deterministic checks should run during work rather than only after prose claims;
- independent review is valuable for particular classes of change;
- human review is required at particular authorial or risk boundaries.

Retain only the durable cross-project preference that the human actually endorses.

Keep project mechanics out of global Control, including:

- exact test commands;
- concrete test suites or fixtures;
- GitHub Actions workflow names/triggers;
- CI providers;
- repository merge gates;
- project-specific coverage thresholds;
- release procedures;
- exact review bots/tools;
- provider configuration.

Those remain project-local or capability-local even when they instantiate a durable preference.

### 9. Produce a structured audit result before mutation

The review output must make every proposed durable change inspectable.

Use this conceptual shape:

```text
Audit target
  roots and source paths reviewed

Findings
  id
  classification
  target source
  evidence
  reason
  recommended disposition
  destination when relocation is proposed

Missing durable areas
  topic
  why relevant now
  question to human

Proposed durable changes
  target
  reason
  supporting context
  proposed content

Project/local exclusions
  concrete material deliberately kept out of Control and why

Final diff
  exact live authored source → proposed authored source

Acceptance
  pending / accepted / revised / rejected
```

A proposed durable change must always include **target, reason, supporting context, and final diff**.

### 10. Require explicit acceptance before durable source mutation

Generated audit advice is not authored truth.

Show the proposed content and final diff before writing. Require explicit acceptance before durable source mutation.

Do not treat any of the following as acceptance:

- the agent believes the proposal is correct;
- a test passes;
- an observation is accurate;
- the source is stale;
- the user asked for an audit but not an edit;
- silence or failure to object.

If the user requested only an audit, stop with the review packet.

### 11. Apply accepted changes narrowly and re-read source

After explicit acceptance, mutate only the accepted Control paths. Preserve unrelated authored material.

Re-read the changed source directly from the filesystem and report the final source paths. Do not require a re-import or generated-index update before the source is considered live.

When relocating procedure, complete or explicitly hand off the Skill/Action change rather than deleting useful procedure and losing it.

### 12. Record evidence without promoting derived material

Completion evidence can include:

- source paths inspected;
- findings and their classifications;
- conflict sources;
- dialogue context supporting a missing-durable-area;
- proposed and accepted diffs;
- relocation destinations;
- project-specific mechanics deliberately excluded from Control;
- post-write filesystem re-read.

Evidence explains the change. Evidence is not itself authored Control unless the human explicitly adopts a durable statement from it.

## Verification/confidence scope example

A healthy separation is:

```text
Control/agents:
For engineering work, I want completion claims backed by appropriate executed evidence, and routine implementation should preserve the project's existing assurance.

Project-local repository:
cargo test --workspace
.github/workflows/verify.yml
required PR checks
coverage target for this project
```

The durable preference can survive changes in language, repository, CI provider, or test command. The mechanics stay where they can evolve with the project.

## Decision rules

### Authored source is not generated advice

A strong agent recommendation remains a proposal until the human accepts it.

### Observation is supporting context, not automatic Control

Current machine state, current project state, or a one-session preference can support discussion without becoming durable source.

### Conflict is not synthesis permission

Two incompatible authored statements must be surfaced for resolution. Do not invent a compromise and write it silently.

### Missing content is not a schema error

Control deliberately has no universal fixed schema below its three roots. Surface an absent preference area only when it matters to the current interaction.

### Useful procedure deserves a procedural home

Do not delete reusable steps merely because they are misplaced. Preserve their value by proposing a Skill or Action destination.

### Project mechanics stay project-local

A durable preference about evidence can belong in `Control/agents`; the commands, CI workflow, merge gate, provider, and project-specific thresholds that satisfy it do not.

## Completion checklist

A complete Control-maintenance pass can answer:

- Which authored Control source was actually inspected?
- Which relevant items are clean?
- Which findings are stale, duplicate, conflicting, low-value, misplaced, or procedure-candidates?
- What evidence supports each classification?
- Did any current dialogue reveal a genuinely relevant missing durable preference area?
- If engineering governance was relevant, was verification/confidence offered as an optional topic rather than imposed as schema?
- If verification preferences were discussed, what durable cross-project preference was retained and which project mechanics were explicitly excluded?
- Does every proposed durable change include target, reason, supporting context, and final diff?
- Was explicit human acceptance obtained before any durable mutation?
- Were accepted edits re-read directly from ordinary filesystem Control source?

If any answer needed for the requested scope is missing, the maintenance pass is not complete.
