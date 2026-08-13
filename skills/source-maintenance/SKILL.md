---
name: source-maintenance
description: Review durable Central source and prepare human-reviewed maintenance proposals.
---

# Source maintenance

Use this procedure to maintain `Control/user`, `Control/agents`, or `Control/machines`. Control is durable human-owned source. Observed state and generated advice can support a proposal but do not become authored truth automatically.

## Audit procedure

1. Read the relevant Control root and identify the scope of each item before judging it.
2. Apply the durability test: would removing the item materially reduce future understanding, interaction quality, decision quality, or reproducibility where it applies?
3. Identify stale, duplicate, conflicting, low-value, and misplaced content. Show conflicting authored sources rather than silently merging them.
4. Distinguish authored source from current observations and generated projections. Current tool, package, host, or service state remains observation unless the human deliberately adopts a durable statement about it.
5. Check scope. Keep project-specific facts, CI workflows, test commands, gates, active plans, and temporary requirements with the project or task instead of promoting them into global Control.
6. Identify reusable procedure. Long-lived instructions describing how an agent should perform a task belong in a Skill or Action; Control may record only the durable preference or relationship that makes that procedure relevant.
7. When the current dialogue reveals an important durable preference area that is missing or unclear, surface it as an optional question rather than imposing a schema.
8. Prepare a proposed change with target source, reason, supporting context, and final diff. Do not mutate durable source before explicit human acceptance.

## Verification and confidence dialogue

When engineering practice is relevant, offer verification and confidence as one optional durable-preference topic. Ask what normally gives the user confidence that agent-produced software work is complete, including preferences about executed tests, CI, independent review, evidence, and when human review is required.

Retain only the cross-project preference that emerges. Keep concrete repository commands, CI providers, workflow triggers, merge gates, coverage thresholds, project test seams, and release checks at project scope.

## Proposal format

Every durable proposal must include:

- target Control source;
- direct proposed content;
- reason it has durable value at this scope;
- supporting authored context or observations, labelled by source class;
- any conflicting or superseded source;
- relocation target when content belongs in a project, Skill, Action, or temporary scope;
- final diff;
- explicit acceptance request before mutation.
