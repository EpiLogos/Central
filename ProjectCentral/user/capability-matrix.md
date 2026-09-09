# Central capabilities and relations

Central is a CLI that safely and minimally structures a filesystem into a shared, recursive home for Human–Agent work. These capabilities develop that foundation: establishing common ground, relating projects, preserving source and carrying ongoing work. The human-authored [0/1 overview](central.html#whole) anchors the intended product; its [What](central.html#whole/what) defines the scope this inventory serves. Each capability links to its expanded account, native operation and evidence.

Status: additive review draft, 2026-09-06. The inventory contains 25 capability families recovered from current source and development tickets, alongside the source-defined suite relation readings. Implemented capability entries use **source-inspected** status; the guided-authoring target is **intent-only**. For implemented entries, test definitions have been inspected. A selected set of 67 native tests was also executed successfully; [the account’s verification](central.html#q5/verification) records the exact command and scope. Each entry states material limits. Ticket state alone is not acceptance.




## Seed × field contribution

[View declarations](capability-matrix.json) · [Editable CSV](capability-matrix.csv). Select a populated cell for its source and capability links. Unassessed cells carry no assertion.

| Seed | O:I whole | Actuation | AIKit | Software Factory | Workcell | Quaternal Logic |
| --- | --- | --- | --- | --- | --- | --- |
| [Why?](central.html#whole/why) | [relation](#field-q0-S) | Unassessed | Unassessed | Unassessed | Unassessed | Unassessed |
| [What?](central.html#whole/what) | [12 capabilities](#field-q1-S) | Unassessed | Unassessed | Unassessed | Unassessed | Unassessed |
| [How?](central.html#whole/how) | [9 capabilities](#field-q2-S) | Unassessed | Unassessed | Unassessed | Unassessed | Unassessed |
| [Who / Whereby?](central.html#whole/whereby) | [5 capabilities](#field-q3-S) | Unassessed | Unassessed | Unassessed | Unassessed | Unassessed |
| [Where / When?](central.html#whole/context) | [relation](#field-q4-S) | Unassessed | Unassessed | Unassessed | [relation](#field-q4-S4) | Unassessed |
| [Why-For?](central.html#whole/purpose) | [relation](#field-q5-S) | Unassessed | [2 capabilities](#field-q5-S2) | Unassessed | Unassessed | Unassessed |

<details>
<summary>Read the field contributions and their capability links</summary>

<a id="field-q0-S"></a>

### Why? → O:I whole

Central aims to fill a gap in agentic work: bringing together the agentic World born in the Human–Agent relationship. An Agent encounters a World through the context it receives. The context of that context is a filesystem. Central gives that filesystem a structure, relating each project’s own Central to common Central ground. Instances of agency on a machine can then be understood as nodes in a field of activity, whose context files and code carry the inputs and outputs of agentic work.



[Source account passage](central.html#whole/why) · placement: agent-inference.

<a id="field-q1-S"></a>

### What? → O:I whole

Central is a CLI that safely and minimally structures a filesystem. It anchors relationships between projects, creates spaces for human- and Agent-developed artifacts, and provides a recursive home for AI work across the system.

[root](#cap-central-root) · [work entry](#cap-central-work-entry) · [control reading](#cap-central-control-reading) · [machine declaration](#cap-central-machine-declaration) · [machine reconciliation](#cap-central-machine-reconciliation) · [authored ground](#cap-central-authored-ground) · [world map](#cap-central-world-map) · [source reading](#cap-central-source-reading) · [governance](#cap-central-governance) · [skills](#cap-central-skills) · [personal surface](#cap-central-personal-surface) · [files](#cap-central-files)

[Source account passage](central.html#whole/what) · placement: agent-inference.

<a id="field-q2-S"></a>

### How? → O:I whole

Central projects a filesystem structure, laying the infrastructure for Agent navigation and human clarity around work. The conditions that determine an Agent’s World live outside any particular harness session and remain available between its activities. Tending those conditions becomes a quiet starting point for developing one’s own relationship with AI Agents and their work.

[machine recovery](#cap-central-machine-recovery) · [projectcentral lifecycle](#cap-central-projectcentral-lifecycle) · [source editing](#cap-central-source-editing) · [now](#cap-central-now) · [day rollover](#cap-central-day-rollover) · [flow](#cap-central-flow) · [agent profiles](#cap-central-agent-profiles) · [control proposals](#cap-central-control-proposals) · [guided ground authoring](#cap-central-guided-ground-authoring)

[Source account passage](central.html#whole/how) · placement: agent-inference.

<a id="field-q3-S"></a>

### Who / Whereby? → O:I whole

For human users, Central is the means by which a simple foundation for organised work and a deliberate User–Agent relationship can be established. It also makes an O:I World—an Objective Internality constituting both Agent and user—available for inspection, sharing and research.

[action discovery](#cap-central-action-discovery) · [reproject](#cap-central-reproject) · [source change](#cap-central-source-change) · [source history](#cap-central-source-history) · [notification](#cap-central-notification)

[Source account passage](central.html#whole/whereby) · placement: agent-inference.

<a id="field-q4-S"></a>

### Where / When? → O:I whole

Central is consistent and quiet: the relatively immutable structured ground that gives space and form to the relationships involved in agentic work, between Humans, Agents, Machines and Work.



[Source account passage](central.html#whole/context) · placement: agent-inference.

<a id="field-q4-S4"></a>

### Where / When? → Workcell

Central preserves the declared source and native operations; Workcell and the other execution owners determine how work becomes operative on a machine.



[Source account passage](central.html#q4/contexts) · placement: agent-inference.

<a id="field-q5-S"></a>

### Why-For? → O:I whole

Central provides the foundational bootstrap for the O:I paradigm. It is intended as the first install and the only necessary package for beginning to work in that paradigm. A person can keep their existing setup while Central gives it a more integrated, relationally aware structure. This foundation supports the other O:I packages: it gives AIKit’s wiki functions a source home and receives projections from Workcell machine setups. Its first concern is the initial disclosure of a person’s World to an Agent: making the context being offered clear and worth tending. Central gives durable principles, governance and user context a common home from which they can inform projects at the depth and scope the user chooses. The user determines how that material propagates and lives. Central gives the ratified basis a home.



[Source account passage](central.html#whole/purpose) · placement: agent-inference.

<a id="field-q5-S2"></a>

### Why-For? → AIKit

AIKit’s wiki functions can develop and disclose knowledge relative to its sources.

[authored ground](#cap-central-authored-ground) · [source reading](#cap-central-source-reading)

[Source account passage](central.html#q5/coherence) · placement: agent-inference.

</details>


## Native CLI catalogue

Observed 6 September 2026 through `ctrl action list --json`. These 65 native Action identities are available through `ctrl action run <id>`. Discovery establishes command exposure; mutation and physical provider behaviour retain their own evidence.

<!-- cli-catalog:start -->
| CLI identity | Capability |
| --- | --- |
| `action.list` | [cap.central.action-discovery](#cap-central-action-discovery) |
| `agent-profile.list` | [cap.central.agent-profiles](#cap-central-agent-profiles) |
| `agent-profile.propose` | [cap.central.agent-profiles](#cap-central-agent-profiles) |
| `agent-profile.read` | [cap.central.agent-profiles](#cap-central-agent-profiles) |
| `agent-profile.remove` | [cap.central.agent-profiles](#cap-central-agent-profiles) |
| `agent-profile.save` | [cap.central.agent-profiles](#cap-central-agent-profiles) |
| `central.agent-set.list` | [cap.central.bounded-relations](#cap-central-bounded-relations) |
| `central.agent-set.read` | [cap.central.bounded-relations](#cap-central-bounded-relations) |
| `central.agent-set.remove` | [cap.central.bounded-relations](#cap-central-bounded-relations) |
| `central.agent-set.resolve` | [cap.central.bounded-relations](#cap-central-bounded-relations) |
| `central.agent-set.save` | [cap.central.bounded-relations](#cap-central-bounded-relations) |
| `central.doctor` | [cap.central.root](#cap-central-root) |
| `central.files.history` | [cap.central.files](#cap-central-files) |
| `central.files.list` | [cap.central.files](#cap-central-files) |
| `central.files.read` | [cap.central.files](#cap-central-files) |
| `central.files.recovery_preview` | [cap.central.files](#cap-central-files) |
| `central.files.restore` | [cap.central.files](#cap-central-files) |
| `central.files.write` | [cap.central.files](#cap-central-files) |
| `central.init` | [cap.central.root](#cap-central-root) |
| `central.recognize` | [cap.central.projectcentral-lifecycle](#cap-central-projectcentral-lifecycle) |
| `central.recover` | [cap.central.machine-recovery](#cap-central-machine-recovery) |
| `central.recovery.plan` | [cap.central.machine-recovery](#cap-central-machine-recovery) |
| `central.remember` | [cap.central.remembered-note](#cap-central-remembered-note) |
| `central.root` | [cap.central.root](#cap-central-root) |
| `central.template.preview` | [cap.central.governance](#cap-central-governance) |
| `central.template.stamp` | [cap.central.governance](#cap-central-governance) |
| `central.wiki.read` | [cap.central.wiki-reading](#cap-central-wiki-reading) |
| `central.world` | [cap.central.world-map](#cap-central-world-map) |
| `central.world-relations.list` | [cap.central.bounded-relations](#cap-central-bounded-relations) |
| `central.world-relations.read` | [cap.central.bounded-relations](#cap-central-bounded-relations) |
| `central.world-relations.remove` | [cap.central.bounded-relations](#cap-central-bounded-relations) |
| `central.world-relations.save` | [cap.central.bounded-relations](#cap-central-bounded-relations) |
| `central.world.effective-sources` | [cap.central.bounded-relations](#cap-central-bounded-relations) |
| `central.world.project` | [cap.central.world-map](#cap-central-world-map) |
| `central.world.reproject.apply` | [cap.central.reproject](#cap-central-reproject) |
| `central.world.reproject.plan` | [cap.central.reproject](#cap-central-reproject) |
| `control.engineering-ground.plan` | [cap.central.governance](#cap-central-governance) |
| `control.engineering-ground.render` | [cap.central.governance](#cap-central-governance) |
| `control.open` | [cap.central.control-reading](#cap-central-control-reading) |
| `control.search` | [cap.central.control-reading](#cap-central-control-reading) |
| `control.skills.inspect` | [cap.central.skills](#cap-central-skills) |
| `control.skills.restore` | [cap.central.skills](#cap-central-skills) |
| `control.skills.retire` | [cap.central.skills](#cap-central-skills) |
| `machine.account` | [cap.central.machine-reconciliation](#cap-central-machine-reconciliation) |
| `machine.adopt-current` | [cap.central.machine-reconciliation](#cap-central-machine-reconciliation) |
| `machine.apply` | [cap.central.machine-reconciliation](#cap-central-machine-reconciliation) |
| `machine.declaration` | [cap.central.machine-declaration](#cap-central-machine-declaration) |
| `machine.inspect` | [cap.central.machine-reconciliation](#cap-central-machine-reconciliation) |
| `machine.plan` | [cap.central.machine-reconciliation](#cap-central-machine-reconciliation) |
| `machine.verify` | [cap.central.machine-reconciliation](#cap-central-machine-reconciliation) |
| `projectcentral.adopt` | [cap.central.projectcentral-lifecycle](#cap-central-projectcentral-lifecycle) |
| `projectcentral.adopt.preview` | [cap.central.projectcentral-lifecycle](#cap-central-projectcentral-lifecycle) |
| `projectcentral.change.ack` | [cap.central.source-change](#cap-central-source-change) |
| `projectcentral.change.horizon` | [cap.central.source-change](#cap-central-source-change) |
| `projectcentral.change.reconcile` | [cap.central.source-change](#cap-central-source-change) |
| `projectcentral.doctor` | [cap.central.projectcentral-lifecycle](#cap-central-projectcentral-lifecycle) |
| `projectcentral.flow.adopt` | [cap.central.flow](#cap-central-flow) |
| `projectcentral.flow.create` | [cap.central.flow](#cap-central-flow) |
| `projectcentral.flow.history` | [cap.central.flow](#cap-central-flow) |
| `projectcentral.flow.inspect` | [cap.central.flow](#cap-central-flow) |
| `projectcentral.flow.lifecycle` | [cap.central.flow](#cap-central-flow) |
| `projectcentral.flow.list` | [cap.central.flow](#cap-central-flow) |
| `projectcentral.flow.now` | [cap.central.flow](#cap-central-flow) |
| `projectcentral.flow.read` | [cap.central.flow](#cap-central-flow) |
| `projectcentral.flow.rename` | [cap.central.flow](#cap-central-flow) |
| `projectcentral.flow.write` | [cap.central.flow](#cap-central-flow) |
| `projectcentral.ground.apply` | [cap.central.authored-ground](#cap-central-authored-ground) |
| `projectcentral.ground.inspect` | [cap.central.authored-ground](#cap-central-authored-ground) |
| `projectcentral.ground.plan` | [cap.central.authored-ground](#cap-central-authored-ground) |
| `projectcentral.init` | [cap.central.projectcentral-lifecycle](#cap-central-projectcentral-lifecycle) |
| `projectcentral.inspect` | [cap.central.projectcentral-lifecycle](#cap-central-projectcentral-lifecycle) |
| `projectcentral.migrate` | [cap.central.projectcentral-lifecycle](#cap-central-projectcentral-lifecycle) |
| `projectcentral.migrate.preview` | [cap.central.projectcentral-lifecycle](#cap-central-projectcentral-lifecycle) |
| `projectcentral.now.init` | [cap.central.now](#cap-central-now) |
| `projectcentral.now.inspect` | [cap.central.now](#cap-central-now) |
| `projectcentral.now.promote` | [cap.central.day-rollover](#cap-central-day-rollover) |
| `projectcentral.now.return` | [cap.central.now](#cap-central-now) |
| `projectcentral.now.rollover` | [cap.central.day-rollover](#cap-central-day-rollover) |
| `projectcentral.now.update` | [cap.central.now](#cap-central-now) |
| `projectcentral.remember` | [cap.central.remembered-note](#cap-central-remembered-note) |
| `projectcentral.source.compare` | [cap.central.source-history](#cap-central-source-history) |
| `projectcentral.source.history` | [cap.central.source-history](#cap-central-source-history) |
| `projectcentral.source.read` | [cap.central.source-reading](#cap-central-source-reading) |
| `projectcentral.source.recovery.preview` | [cap.central.source-history](#cap-central-source-history) |
| `projectcentral.source.return` | [cap.central.source-editing](#cap-central-source-editing) |
| `projectcentral.source.return_accept` | [cap.central.source-editing](#cap-central-source-editing) |
| `projectcentral.source.return_read` | [cap.central.source-editing](#cap-central-source-editing) |
| `projectcentral.source.return_reject` | [cap.central.source-editing](#cap-central-source-editing) |
| `projectcentral.source.returns` | [cap.central.source-editing](#cap-central-source-editing) |
| `projectcentral.source.write` | [cap.central.source-editing](#cap-central-source-editing) |
| `projectcentral.wiki.read` | [cap.central.wiki-reading](#cap-central-wiki-reading) |
| `work.list` | [cap.central.work-entry](#cap-central-work-entry) |
| `work.open` | [cap.central.work-entry](#cap-central-work-entry) |
| `work.reveal` | [cap.central.work-entry](#cap-central-work-entry) |
| `work.search` | [cap.central.work-entry](#cap-central-work-entry) |
<!-- cli-catalog:end -->

## Capability index

| Capability | What becomes possible |
|---|---|
| [Establish a usable Central root](#cap-central-root) | A person needs a durable place for personal source and ordinary projects that tools can find consistently. |
| [Discover available operations](#cap-central-action-discovery) | A person or agent needs to discover what this installed Central can actually do before choosing a command. |
| [Find and enter a project](#cap-central-work-entry) | A person wants to move from a project name to the actual working directory without maintaining a duplicate project catalogue. |
| [Inspect ordinary Central files](#cap-central-files) | A person or agent needs to inspect ordinary Central-root material without adopting it or inventing a semantic source identity. |
| [Find personal source](#cap-central-control-reading) | People and agents need to consult durable preferences, collaboration guidance and machine declarations where they are authored. |
| [Describe the machine needed for work](#cap-central-machine-declaration) | A person needs to state the environment they rely on so it can be inspected and recovered later. |
| [Bring an environment toward its declared state](#cap-central-machine-reconciliation) | A person needs to see what is missing and apply supported changes with a clear account of the result. |
| [Recover a working environment](#cap-central-machine-recovery) | After a machine change or loss, a person needs a repeatable route back to their declared tools and configuration. |
| [Establish project continuity](#cap-central-projectcentral-lifecycle) | A project needs stable source relationships and places for human ground and collaborative material as it develops. |
| [Recognise the sources that guide development](#cap-central-authored-ground) | A person wants existing vision, UX and design decisions available to collaborators without rewriting every source. |
| [See the current world and project state](#cap-central-world-map) | A person needs to locate work, current threads and source areas, including places whose structure is incomplete. |
| [Add missing project structure](#cap-central-reproject) | An existing project should gain current facilities while preserving its accumulated files. |
| [Read the exact source behind a view](#cap-central-source-reading) | A reader needs to inspect the material underlying an account or decision and know which revision they are seeing. |
| [Edit shared material without overwriting newer work](#cap-central-source-editing) | A person and an agent may both change a file between reads, so each edit needs a known basis. |
| [Recover what changed since the last reading](#cap-central-source-change) | People need to keep using ordinary editors while later sessions discover relevant source changes. |
| [Compare earlier material and prepare recovery](#cap-central-source-history) | A person needs to understand how a source changed or retrieve a prior version before deciding what to restore. |
| [Resume current collaboration across sessions](#cap-central-now) | Current scratch, questions and handoffs need to remain available after a chat ends. |
| [Close the day and carry useful work forward](#cap-central-day-rollover) | A person needs a dated account of work while keeping unfinished material available and promoting useful findings deliberately. |
| [Develop one thread over time](#cap-central-flow) | A developing idea needs continuity across sessions, days and changes of filename. |
| [Carry collaboration guidance into each project](#cap-central-governance) | A person needs durable collaboration preferences and local engineering rules that can be found and revised in their owning scope. |
| [Maintain reusable ways of working](#cap-central-skills) | People need reusable skills to remain discoverable while outdated methods can be withdrawn and later restored. |
| [Save how an agent should work in a scope](#cap-central-agent-profiles) | A person needs durable agent configuration that survives sessions and can vary between personal and project work. |
| [Reach personal source and work from one place](#cap-central-personal-surface) | A person needs an understandable entry into their own preferences, agent guidance, machines and projects. |
| [Adopt a durable preference after review](#cap-central-control-proposals) | An agent suggestion needs a visible review step before it becomes lasting personal or collaboration source. |
| [Request the person’s attention](#cap-central-notification) | A task needs to alert the person through the installed machine while retaining which work caused the request. |
| [Recover and review a coherent project account](#cap-central-guided-ground-authoring) | Intended integrated journey from fragmented sources to reviewable product ground. |

## Capability accounts

<a id="cap-central-root"></a>
### Establish a usable Central root

`cap.central.root` · #capability #central

A person needs a durable place for personal source and ordinary projects that tools can find consistently. central.root, central.init and central.doctor resolve, initialise and inspect the selected root.

The root contains Control and Work with structured health diagnostics. Repeated initialisation preserves existing authored material.

**Source:** [../github-recovery-mirror/mirror/Central/issues/2.json](../../../github-recovery-mirror/mirror/Central/issues/2.json); [../github-recovery-mirror/mirror/Central/issues/19.json](../../../github-recovery-mirror/mirror/Central/issues/19.json).

**Implementation:** [ctrl/src/root.rs](../../ctrl/src/root.rs); [ctrl/src/action.rs](../../ctrl/src/action.rs). **Test definitions:** [ctrl/tests/cli_contract.rs](../../ctrl/tests/cli_contract.rs); [ctrl/tests/product_acceptance.rs](../../ctrl/tests/product_acceptance.rs).

**Verification limit:** Source and test definitions inspected; this capability was not exercised in the selected run.

**Account relation:** [[central]] · [capability reading](central.html#q1/entry).

<a id="cap-central-action-discovery"></a>
### Discover available operations

`cap.central.action-discovery` · #capability #central

A person or agent needs to discover what this installed Central can actually do before choosing a command. ctrl capabilities and action.list expose the native Action registry; the guided picker helps select an operation and its inputs.

Callers receive named operations, input definitions, mutation classes and capability requirements. The same canonical operation can be selected through different surfaces.

**Source:** [../github-recovery-mirror/mirror/Central/issues/3.json](../../../github-recovery-mirror/mirror/Central/issues/3.json); [../github-recovery-mirror/mirror/Central/issues/7.json](../../../github-recovery-mirror/mirror/Central/issues/7.json); [../github-recovery-mirror/mirror/Central/issues/114.json](../../../github-recovery-mirror/mirror/Central/issues/114.json).

**Implementation:** [ctrl/src/action.rs](../../ctrl/src/action.rs); [ctrl/src/picker.rs](../../ctrl/src/picker.rs); [ctrl/src/cli.rs](../../ctrl/src/cli.rs). **Test definitions:** [ctrl/tests/cli_doorway.rs](../../ctrl/tests/cli_doorway.rs); [ctrl/tests/picker.rs](../../ctrl/tests/picker.rs).

**Verification limit:** Source and test definitions inspected; this capability was not exercised in the selected run.

**Account relation:** [[central]] · [capability reading](central.html#q3/interaction).

<a id="cap-central-work-entry"></a>
### Find and enter a project

`cap.central.work-entry` · #capability #central

A person wants to move from a project name to the actual working directory without maintaining a duplicate project catalogue. work.list and work.search discover ordinary Work entries; work.open and work.reveal invoke the native opening or revealing Port.

The result identifies the selected filesystem project and native provider outcome. Opening and revealing depend on an available native Connector.

**Source:** [../github-recovery-mirror/mirror/Central/issues/6.json](../../../github-recovery-mirror/mirror/Central/issues/6.json); [../github-recovery-mirror/mirror/Central/issues/14.json](../../../github-recovery-mirror/mirror/Central/issues/14.json); [../github-recovery-mirror/mirror/Central/issues/15.json](../../../github-recovery-mirror/mirror/Central/issues/15.json).

**Implementation:** [ctrl/src/action.rs](../../ctrl/src/action.rs). **Test definitions:** [ctrl/tests/work_entry.rs](../../ctrl/tests/work_entry.rs).

**Verification limit:** source-inspected; Selected native suites passed 2026-09-06: work_entry. Other referenced tests were not rerun.

**Account relation:** [[central]] · [capability reading](central.html#q1/entry).

<a id="cap-central-files"></a>
### Inspect ordinary Central files

`cap.central.files` · #capability #central

A person or agent needs to inspect ordinary Central-root material without adopting it or inventing a semantic source identity. `central.files.list` returns bounded directory entries and owner locations; `central.files.read` returns a validated returned location as bounded UTF-8 text.

Eligible regular files and directories can be inspected with exact locations and content revisions while retrieval exclusions and redirected paths refuse access.

**Source:** [docs/NATIVE-FILESYSTEM-READING.md](../../docs/NATIVE-FILESYSTEM-READING.md).

**Implementation:** [ctrl/src/files.rs](../../ctrl/src/files.rs). **Test definitions:** [ctrl/src/files.rs](../../ctrl/src/files.rs); [ctrl/tests/foundation.rs](../../ctrl/tests/foundation.rs).

**Verification limit:** The gated release executable listed the real configured Central root in this documentation pass. Linked source tests were not freshly executed.

**Account relation:** [[central]] · [capability reading](central.html#q1/entry).

<a id="cap-central-control-reading"></a>
### Find personal source

`cap.central.control-reading` · #capability #central

People and agents need to consult durable preferences, collaboration guidance and machine declarations where they are authored. control.open selects a Control area; control.search retrieves matching eligible source through the Control read model.

Results point back to ordinary authored files. Retrieval honours excluded subtrees, so a broad search can remain useful without exposing every personal file.

**Source:** [../github-recovery-mirror/mirror/Central/issues/5.json](../../../github-recovery-mirror/mirror/Central/issues/5.json); [../github-recovery-mirror/mirror/Central/issues/12.json](../../../github-recovery-mirror/mirror/Central/issues/12.json).

**Implementation:** [ctrl/src/action.rs](../../ctrl/src/action.rs); [ctrl/src/control.rs](../../ctrl/src/control.rs). **Test definitions:** [ctrl/tests/control_source.rs](../../ctrl/tests/control_source.rs); [ctrl/tests/product_acceptance.rs](../../ctrl/tests/product_acceptance.rs).

**Verification limit:** source-inspected; Selected native suites passed 2026-09-06: control_source. Other referenced tests were not rerun.

**Account relation:** [[central]] · [capability reading](central.html#q1/entry).

<a id="cap-central-machine-declaration"></a>
### Describe the machine needed for work

`cap.central.machine-declaration` · #capability #central

A person needs to state the environment they rely on so it can be inspected and recovered later. machine.declaration reads a versioned machine-role source; machine.inspect asks available providers for the observed environment.

The declaration records desired capabilities, packages, configuration and services; inspection supplies current observations for comparison. Editing the authored declaration changes the next reading directly.

**Source:** [../github-recovery-mirror/mirror/Central/issues/8.json](../../../github-recovery-mirror/mirror/Central/issues/8.json); [../github-recovery-mirror/mirror/Central/issues/13.json](../../../github-recovery-mirror/mirror/Central/issues/13.json).

**Implementation:** [ctrl/src/machine.rs](../../ctrl/src/machine.rs). **Test definitions:** [ctrl/tests/machine_declaration.rs](../../ctrl/tests/machine_declaration.rs); [ctrl/tests/machine_plan.rs](../../ctrl/tests/machine_plan.rs).

**Verification limit:** Source and test definitions inspected; this capability was not exercised in the selected run.

**Account relation:** [[central]] · [capability reading](central.html#q1/environment).

<a id="cap-central-machine-reconciliation"></a>
### Bring an environment toward its declared state

`cap.central.machine-reconciliation` · #capability #central

A person needs to see what is missing and apply supported changes with a clear account of the result. machine.plan compares desired and observed state; machine.apply delegates selected changes through Ports; machine.verify re-inspects the result.

A structured plan distinguishes satisfied, changeable and unsupported requirements. Application reports complete, partial or failed outcomes and verification mismatches.

**Source:** [../github-recovery-mirror/mirror/Central/issues/9.json](../../../github-recovery-mirror/mirror/Central/issues/9.json); [../github-recovery-mirror/mirror/Central/issues/10.json](../../../github-recovery-mirror/mirror/Central/issues/10.json); [../github-recovery-mirror/mirror/Central/issues/16.json](../../../github-recovery-mirror/mirror/Central/issues/16.json); [../github-recovery-mirror/mirror/Central/issues/17.json](../../../github-recovery-mirror/mirror/Central/issues/17.json).

**Implementation:** [ctrl/src/machine.rs](../../ctrl/src/machine.rs). **Test definitions:** [ctrl/tests/machine_plan.rs](../../ctrl/tests/machine_plan.rs); [ctrl/tests/machine_reconciliation.rs](../../ctrl/tests/machine_reconciliation.rs).

**Verification limit:** Source inspected; controller tests include fixture providers and were not run. Real workstation/server provider acceptance remains a separate physical gate in issues 16 and 17.

**Account relation:** [[central]] · [capability reading](central.html#q1/environment).

<a id="cap-central-machine-recovery"></a>
### Recover a working environment

`cap.central.machine-recovery` · #capability #central

After a machine change or loss, a person needs a repeatable route back to their declared tools and configuration. Recovery planning reads the role recovery declaration, previews synchronisation and environment reconciliation, then recovery delegates through providers and verifies.

The recovery report retains per-stage results and remaining differences. Recovery depends on the configured source, synchronisation and environment providers.

**Source:** [../github-recovery-mirror/mirror/Central/issues/21.json](../../../github-recovery-mirror/mirror/Central/issues/21.json); [../github-recovery-mirror/mirror/Central/issues/16.json](../../../github-recovery-mirror/mirror/Central/issues/16.json); [../github-recovery-mirror/mirror/Central/issues/17.json](../../../github-recovery-mirror/mirror/Central/issues/17.json).

**Implementation:** [ctrl/src/recovery.rs](../../ctrl/src/recovery.rs). **Test definitions:** [ctrl/tests/recovery.rs](../../ctrl/tests/recovery.rs).

**Verification limit:** Source inspected; recovery controller tests include provider fixtures and were not run. Issue 21 records hosted provider proof; named owner-machine acceptance requires its own evidence.

**Account relation:** [[central]] · [capability reading](central.html#q2/restore).

<a id="cap-central-projectcentral-lifecycle"></a>
### Establish project continuity

`cap.central.projectcentral-lifecycle` · #capability #central

A project needs stable source relationships and places for human ground and collaborative material as it develops. ProjectCentral inspect, init, adopt and migrate operations establish identity and selected source bindings with previews and provenance.

An ordinary project gains the ProjectCentral filesystem convention and root registration. Existing compatible sources can be adopted in place or explicitly migrated with retained history.

**Source:** [../github-recovery-mirror/mirror/Central/issues/65.json](../../../github-recovery-mirror/mirror/Central/issues/65.json); [../github-recovery-mirror/mirror/Central/issues/66.json](../../../github-recovery-mirror/mirror/Central/issues/66.json); [../github-recovery-mirror/mirror/Central/issues/67.json](../../../github-recovery-mirror/mirror/Central/issues/67.json).

**Implementation:** [ctrl/src/projectcentral.rs](../../ctrl/src/projectcentral.rs); [ctrl/src/projectcentral_ops.rs](../../ctrl/src/projectcentral_ops.rs). **Test definitions:** [ctrl/src/projectcentral_ops.rs](../../ctrl/src/projectcentral_ops.rs).

**Verification limit:** Source and test definitions inspected; this capability was not exercised in the selected run.

**Account relation:** [[central]] · [capability reading](central.html#q2/starting).

<a id="cap-central-authored-ground"></a>
### Recognise the sources that guide development

`cap.central.authored-ground` · #capability #central

A person wants existing vision, UX and design decisions available to collaborators without rewriting every source. projectcentral.ground.inspect and plan discover candidates and propose treatments; apply records an explicitly accepted provenance, standing and role relation.

The recognised source becomes addressable as project ground while retaining its actual path and bytes. The current planner recommends source treatment; coherent intent recovery and authoring still require a human/agent workflow.

**Source:** [../github-recovery-mirror/mirror/Central/issues/70.json](../../../github-recovery-mirror/mirror/Central/issues/70.json); [../github-recovery-mirror/mirror/Central/issues/104.json](../../../github-recovery-mirror/mirror/Central/issues/104.json).

**Implementation:** [ctrl/src/projectcentral_ground.rs](../../ctrl/src/projectcentral_ground.rs). **Test definitions:** [ctrl/tests/projectcentral_ground_actions.rs](../../ctrl/tests/projectcentral_ground_actions.rs); [ctrl/tests/projectcentral_ground_portable_real.rs](../../ctrl/tests/projectcentral_ground_portable_real.rs).

**Verification limit:** source-inspected; Selected native suites passed 2026-09-06: projectcentral_ground_actions. Other referenced tests were not rerun.

**Account relation:** [[central]] · [capability reading](central.html#q1/ground).

<a id="cap-central-world-map"></a>
### See the current world and project state

`cap.central.world-map` · #capability #central

A person needs to locate work, current threads and source areas, including places whose structure is incomplete. central.world and central.world.project read Control, projects, source relations, Flow, NOW and related wiki-space state.

A structured map centres on the root or one project and reports missing, partial and faulty areas alongside the available material. It supplies a navigable consumer read model.

**Source:** [../github-recovery-mirror/mirror/Central/issues/51.json](../../../github-recovery-mirror/mirror/Central/issues/51.json); [../github-recovery-mirror/mirror/Central/issues/65.json](../../../github-recovery-mirror/mirror/Central/issues/65.json); [../github-recovery-mirror/mirror/Central/issues/76.json](../../../github-recovery-mirror/mirror/Central/issues/76.json); [../github-recovery-mirror/mirror/Central/issues/93.json](../../../github-recovery-mirror/mirror/Central/issues/93.json).

**Implementation:** [ctrl/src/world_map.rs](../../ctrl/src/world_map.rs). **Test definitions:** [ctrl/tests/world_map.rs](../../ctrl/tests/world_map.rs).

**Verification limit:** source-inspected; Selected native suites passed 2026-09-06: world_map. Other referenced tests were not rerun.

**Account relation:** [[central]] · [capability reading](central.html#q1/entry).

<a id="cap-central-reproject"></a>
### Add missing project structure

`cap.central.reproject` · #capability #central

An existing project should gain current facilities while preserving its accumulated files. central.world.reproject.plan lists missing canonical scaffold; reproject.apply stamps only missing structure.

The receipt identifies added scaffolding and retained existing material. This additive repair covers missing structure; full source reorganisation and installed-world migration remain broader work.

**Source:** [../github-recovery-mirror/mirror/Central/issues/67.json](../../../github-recovery-mirror/mirror/Central/issues/67.json); [../github-recovery-mirror/mirror/Central/issues/76.json](../../../github-recovery-mirror/mirror/Central/issues/76.json).

**Implementation:** [ctrl/src/world_map.rs](../../ctrl/src/world_map.rs). **Test definitions:** [ctrl/tests/world_map.rs](../../ctrl/tests/world_map.rs).

**Verification limit:** source-inspected; Selected native suites passed 2026-09-06: world_map. Other referenced tests were not rerun.

**Account relation:** [[central]] · [capability reading](central.html#q3/tree).

<a id="cap-central-source-reading"></a>
### Read the exact source behind a view

`cap.central.source-reading` · #capability #central

A reader needs to inspect the material underlying an account or decision and know which revision they are seeing. read_world_source resolves one participating SourceRef and checks retrieval eligibility.

The reading contains source content, exact revision and provenance. Masked or non-participating sources produce an explicit refusal.

**Source:** [../github-recovery-mirror/mirror/Central/issues/70.json](../../../github-recovery-mirror/mirror/Central/issues/70.json); [../github-recovery-mirror/mirror/Central/issues/89.json](../../../github-recovery-mirror/mirror/Central/issues/89.json); [../github-recovery-mirror/mirror/Central/issues/100.json](../../../github-recovery-mirror/mirror/Central/issues/100.json).

**Implementation:** [ctrl/src/world_source.rs](../../ctrl/src/world_source.rs). **Test definitions:** [ctrl/tests/world_source_actions.rs](../../ctrl/tests/world_source_actions.rs).

**Verification limit:** source-inspected; Selected native suites passed 2026-09-06: world_source_actions. Other referenced tests were not rerun.

**Account relation:** [[central]] · [capability reading](central.html#q1/ground).

<a id="cap-central-wiki-reading"></a>
### Read wiki content as typed, versioned source

`cap.central.wiki-reading` · #capability #central

A person or agent needs to read wiki content as typed, versioned source with revision identity and provenance, not as raw text. central.wiki.read resolves one wiki node address and returns its typed reading; projectcentral.wiki.read scopes the same seam to a project wiki.

The reading carries exact resource ref, revision and provenance; display or unread references are explicit states and can never record use.

**Implementation:** [ctrl/src/wiki_read.rs](../../ctrl/src/wiki_read.rs). **Test definitions:** [ctrl/tests/foundation.rs](../../ctrl/tests/foundation.rs).

**Verification limit:** source-inspected; native suite verified by the orchestrator on the landing tree (cargo test -p ctrl 320 passed).

**Account relation:** [[central]] · [capability reading](central.html#q1/ground).

<a id="cap-central-source-editing"></a>
### Edit shared material without overwriting newer work

`cap.central.source-editing` · #capability #central

A person and an agent may both change a file between reads, so each edit needs a known basis. write_world_source checks the expected content revision and actor attribution, then writes or returns the appropriate proposal.

Accepted edits emit attributed source-change records. A stale basis produces a conflict; agent changes to recognised human ground return through proposal authority.

**Source:** [../github-recovery-mirror/mirror/Central/issues/70.json](../../../github-recovery-mirror/mirror/Central/issues/70.json); [../github-recovery-mirror/mirror/Central/issues/89.json](../../../github-recovery-mirror/mirror/Central/issues/89.json); [../github-recovery-mirror/mirror/Central/issues/100.json](../../../github-recovery-mirror/mirror/Central/issues/100.json).

**Implementation:** [ctrl/src/world_source.rs](../../ctrl/src/world_source.rs). **Test definitions:** [ctrl/tests/world_source_actions.rs](../../ctrl/tests/world_source_actions.rs).

**Verification limit:** source-inspected; Selected native suites passed 2026-09-06: world_source_actions. Other referenced tests were not rerun.

**Account relation:** [[central]] · [capability reading](central.html#q2/recognised-ground).

<a id="cap-central-source-change"></a>
### Recover what changed since the last reading

`cap.central.source-change` · #capability #central

People need to keep using ordinary editors while later sessions discover relevant source changes. Project and Control reconciliation observes content revisions; horizon reads expose ordered changes since a consumer cursor and recover offline edits.

Consumers receive source identities, revisions and retained standing without reading every payload. The horizon records change; interpretation and follow-up belong to its consumers.

**Source:** [../github-recovery-mirror/mirror/Central/issues/89.json](../../../github-recovery-mirror/mirror/Central/issues/89.json).

**Implementation:** [ctrl/src/source_horizon.rs](../../ctrl/src/source_horizon.rs). **Test definitions:** [ctrl/tests/source_change_horizon.rs](../../ctrl/tests/source_change_horizon.rs); [ctrl/tests/source_change_authority_override.rs](../../ctrl/tests/source_change_authority_override.rs).

**Verification limit:** source-inspected; Selected native suites passed 2026-09-06: source_change_horizon. Other referenced tests were not rerun.

**Account relation:** [[central]] · [capability reading](central.html#q3/contracts).

<a id="cap-central-source-history"></a>
### Compare earlier material and prepare recovery

`cap.central.source-history` · #capability #central

A person needs to understand how a source changed or retrieve a prior version before deciding what to restore. projectcentral.source.history, compare and recovery.preview request bounded history from an optional native provider and check the current recovery basis.

The caller receives revision entries, differences or historical content. Applying recovered content uses the normal source-write/proposal path; the preview itself leaves current content unchanged.

**Source:** [../github-recovery-mirror/mirror/Central/issues/100.json](../../../github-recovery-mirror/mirror/Central/issues/100.json).

**Implementation:** [ctrl/src/source_history.rs](../../ctrl/src/source_history.rs); [connectors/git-sync/src/source_history.rs](../../connectors/git-sync/src/source_history.rs). **Test definitions:** [connectors/git-sync/src/source_history.rs](../../connectors/git-sync/src/source_history.rs); [ctrl/src/source_history.rs](../../ctrl/src/source_history.rs).

**Verification limit:** Source inspected; real Git connector test and controller fixture test inspected, neither executed. Full provider-to-recovery user acceptance is not established here.

**Account relation:** [[central]] · [capability reading](central.html#q3/contracts).

<a id="cap-central-now"></a>
### Resume current collaboration across sessions

`cap.central.now` · #capability #central

Current scratch, questions and handoffs need to remain available after a chat ends. projectcentral.now.init and inspect establish/read the temporal field; now.return and update record attributed questions, notes and handoffs with lifecycle and external references.

The next session can recover human scratch and bounded agent returns from files, including open work and supporting evidence references.

**Source:** [../github-recovery-mirror/mirror/Central/issues/74.json](../../../github-recovery-mirror/mirror/Central/issues/74.json).

**Implementation:** [ctrl/src/projectcentral_now.rs](../../ctrl/src/projectcentral_now.rs). **Test definitions:** [ctrl/tests/projectcentral_now.rs](../../ctrl/tests/projectcentral_now.rs); [ctrl/tests/projectcentral_now_portable_real.rs](../../ctrl/tests/projectcentral_now_portable_real.rs).

**Verification limit:** source-inspected; Selected native suites passed 2026-09-06: projectcentral_now. Other referenced tests were not rerun.

**Account relation:** [[central]] · [capability reading](central.html#q2/resume).

<a id="cap-central-day-rollover"></a>
### Close the day and carry useful work forward

`cap.central.day-rollover` · #capability #central

A person needs a dated account of work while keeping unfinished material available and promoting useful findings deliberately. now.rollover snapshots the chosen local civil day and carries live material; now.promote records the move from temporal material toward a durable owner.

DAY records preserve source state and carry-forward decisions. Eligible completed clutter can clear; protected material remains. Agent promotion returns material to wiki ownership, where semantic integration is a later operation.

**Source:** [../github-recovery-mirror/mirror/Central/issues/74.json](../../../github-recovery-mirror/mirror/Central/issues/74.json); [../github-recovery-mirror/mirror/Central/issues/93.json](../../../github-recovery-mirror/mirror/Central/issues/93.json).

**Implementation:** [ctrl/src/projectcentral_now.rs](../../ctrl/src/projectcentral_now.rs). **Test definitions:** [ctrl/tests/projectcentral_now.rs](../../ctrl/tests/projectcentral_now.rs); [ctrl/tests/projectcentral_now_portable_real.rs](../../ctrl/tests/projectcentral_now_portable_real.rs).

**Verification limit:** source-inspected; Selected native suites passed 2026-09-06: projectcentral_now. Other referenced tests were not rerun.

**Account relation:** [[central]] · [capability reading](central.html#q2/resume).

<a id="cap-central-flow"></a>
### Develop one thread over time

`cap.central.flow` · #capability #central

A developing idea needs continuity across sessions, days and changes of filename. Flow create/adopt/read/write/rename/lifecycle/history operations maintain a stable FlowRef and revision-safe ordinary-file content.

One thread retains identity while active, dormant or closed; renaming preserves continuity and DAY snapshots retain earlier states. Desktop conversation and knowledge interpretation consume this source lifecycle.

**Source:** [../github-recovery-mirror/mirror/Central/issues/93.json](../../../github-recovery-mirror/mirror/Central/issues/93.json).

**Implementation:** [ctrl/src/projectcentral_flow.rs](../../ctrl/src/projectcentral_flow.rs). **Test definitions:** [ctrl/tests/projectcentral_flow.rs](../../ctrl/tests/projectcentral_flow.rs).

**Verification limit:** source-inspected; Selected native suites passed 2026-09-06: projectcentral_flow. Other referenced tests were not rerun.

**Account relation:** [[central]] · [capability reading](central.html#q2/develop-thought).

<a id="cap-central-governance"></a>
### Carry collaboration guidance into each project

`cap.central.governance` · #capability #central

A person needs durable collaboration preferences and local engineering rules that can be found and revised in their owning scope. Root/project governance inspection discovers recognised sources and native candidates; accepted relations record scope, roles and provenance. The CLI also exposes preview/stamp for missing source templates and plan/render for the engineering prompt derived from those sources.

Guidance remains inspectable in its native location with personal and project applicability. Runtime activation and precedence are resolved by the consuming harness/AIKit layer.

**Source:** [../github-recovery-mirror/mirror/Central/issues/72.json](../../../github-recovery-mirror/mirror/Central/issues/72.json); [../github-recovery-mirror/mirror/Central/issues/104.json](../../../github-recovery-mirror/mirror/Central/issues/104.json).

**Implementation:** [ctrl/src/agent_governance.rs](../../ctrl/src/agent_governance.rs). **Test definitions:** [ctrl/tests/agent_governance.rs](../../ctrl/tests/agent_governance.rs).

**Verification limit:** source-inspected; Selected native suites passed 2026-09-06: agent_governance. Other referenced tests were not rerun.

**Account relation:** [[central]] · [capability reading](central.html#q1/practice).

<a id="cap-central-skills"></a>
### Maintain reusable ways of working

`cap.central.skills` · #capability #central

People need reusable skills to remain discoverable while outdated methods can be withdrawn and later restored. control.skills.inspect reads personal, machine and project scope; retire and restore update a skill manifest with lifecycle provenance.

Retirement records actor and reason while retaining the skill body. Source changes participate in the horizon. Contextual Method composition and runtime use are consumer responsibilities.

**Source:** [../github-recovery-mirror/mirror/Central/issues/82.json](../../../github-recovery-mirror/mirror/Central/issues/82.json).

**Implementation:** [ctrl/src/control_skills.rs](../../ctrl/src/control_skills.rs). **Test definitions:** [ctrl/tests/control_skills.rs](../../ctrl/tests/control_skills.rs).

**Verification limit:** source-inspected; Selected native suites passed 2026-09-06: control_skills. Other referenced tests were not rerun.

**Account relation:** [[central]] · [capability reading](central.html#q1/practice).

<a id="cap-central-agent-profiles"></a>
### Save how an agent should work in a scope

`cap.central.agent-profiles` · #capability #central

A person needs durable agent configuration that survives sessions and can vary between personal and project work. agent-profile.list/read/save/remove operate on source-backed profiles; saves and removals use expected revisions where required.

The profile stores references for an existing Agent in its selected scope and returns a write/removal receipt. It is a saved configuration relation; effective runtime resolution requires the consuming agent system.

**Source:** [../github-recovery-mirror/mirror/Central/issues/108.json](../../../github-recovery-mirror/mirror/Central/issues/108.json); [../github-recovery-mirror/mirror/Central/issues/112.json](../../../github-recovery-mirror/mirror/Central/issues/112.json); [../github-recovery-mirror/mirror/Central/issues/118.json](../../../github-recovery-mirror/mirror/Central/issues/118.json).

**Implementation:** [ctrl/src/agent_profile_actions.rs](../../ctrl/src/agent_profile_actions.rs); [ctrl/src/agent_profile_store.rs](../../ctrl/src/agent_profile_store.rs). **Test definitions:** [ctrl/src/agent_profile_actions.rs](../../ctrl/src/agent_profile_actions.rs).

**Verification limit:** Source and test definitions inspected; this capability was not exercised in the selected run.

**Account relation:** [[central]] · [capability reading](central.html#q2/reuse-practice).

<a id="cap-central-pasu-identity"></a>
### Name every bounded participant the same way

`cap.central.pasu-identity` · #capability #central

A person needs their human identity grounded as a versioned bounded-entity subject the whole toolchain can name without a second registry. `central.pasu/v1` parses and validates nara, agent and agent-set refs; the nara identity-source manifest anchors the authored `Control/user/identity` folder; ground relations carry the subject ref; `central.world` exposes identity state with per-file content revisions.

The grammar and read model surface through `central.world` and the ground-relations read path; the capability owns no CLI command of its own. Identity ownership stays in authored Central ground while Factory, Actuation and AIKit reference one stable subject.

<a id="cap-central-bounded-relations"></a>
### Persist composed participation and world relations

`cap.central.bounded-relations` · #capability #central

A person needs durable authored AgentSets and world relations so composed participation and source propagation survive sessions and stay revisable. `central.agent-set.*` and `central.world-relations.*` persist and resolve the versioned records under compare-and-swap revisions; `central.agent-set.resolve` partitions authored membership against declared availability and rejects cycles; `central.world.effective-sources` resolves ancestry propagation with per-hop provenance.

The Actions carry the already-implemented domain model to disk; authored records never rewrite on availability changes and effective relations stay attributable per hop.

<a id="cap-central-personal-surface"></a>
### Reach personal source and work from one place

`cap.central.personal-surface` · #capability #central

A person needs an understandable entry into their own preferences, agent guidance, machines and projects. personal.show reads the actual Central root and projects its principal areas.

The public read model presents You, Agents, Machines and Work with their filesystem locations for a desktop or other surface to display.

**Source:** [../github-recovery-mirror/mirror/Central/issues/51.json](../../../github-recovery-mirror/mirror/Central/issues/51.json).

**Implementation:** [ctrl/src/personal.rs](../../ctrl/src/personal.rs). **Test definitions:** [ctrl/src/personal.rs](../../ctrl/src/personal.rs).

**Verification limit:** Source and test definitions inspected; this capability was not exercised in the selected run.

**Account relation:** [[central]] · [capability reading](central.html#q1/entry).

<a id="cap-central-remembered-note"></a>
### Remember one selection as a proposal until recognised

`cap.central.remembered-note` · #capability #central

A person needs to remember one selection into a durable destination as a proposal until recognised, so a thought survives the session that produced it. central.remember and projectcentral.remember write one selection verbatim as a content-addressed remembered note stamped generated-proposal/unrecognised.

The note lands under Control/agents/remembered with provenance (verbatim selection, source ref, origin action); recognition stays a separate human-owned act with no machine path.

**Implementation:** [ctrl/src/remember.rs](../../ctrl/src/remember.rs); [ctrl/src/remember_actions.rs](../../ctrl/src/remember_actions.rs); [ctrl/src/remember_store.rs](../../ctrl/src/remember_store.rs). **Test definitions:** [ctrl/tests/foundation.rs](../../ctrl/tests/foundation.rs).

**Verification limit:** source-inspected; native suite verified by the orchestrator on the landing tree (cargo test -p ctrl 320 passed).

**Account relation:** [[central]] · [capability reading](central.html#q2/resume).

<a id="cap-central-control-proposals"></a>
### Adopt a durable preference after review

`cap.central.control-proposals` · #capability #central

An agent suggestion needs a visible review step before it becomes lasting personal or collaboration source. control.propose-change stores proposed content and reason; review-proposal exposes it; apply-proposal records acceptance and writes the selected target.

The person can inspect the proposed change before source mutation. Accepted application updates the actual Control file and retains a proposal receipt.

**Source:** [../github-recovery-mirror/mirror/Central/issues/51.json](../../../github-recovery-mirror/mirror/Central/issues/51.json).

**Implementation:** [ctrl/src/personal.rs](../../ctrl/src/personal.rs). **Test definitions:** [ctrl/src/personal.rs](../../ctrl/src/personal.rs).

**Verification limit:** Source and test definitions inspected; this capability was not exercised in the selected run.

**Account relation:** [[central]] · [capability reading](central.html#q2/recognised-ground).

<a id="cap-central-notification"></a>
### Request the person’s attention

`cap.central.notification` · #capability #central

A task needs to alert the person through the installed machine while retaining which work caused the request. personal.notify routes title/body, subject and caller lineage through the UserNotification Port to an available provider.

A provider receipt reports delivery or degradation; the macOS connector uses Notification Center via osascript. Delivery records an attention request, while any subsequent decision remains separately recorded.

**Source:** [../github-recovery-mirror/mirror/Central/issues/52.json](../../../github-recovery-mirror/mirror/Central/issues/52.json).

**Implementation:** [ctrl/src/personal.rs](../../ctrl/src/personal.rs); [crates/connector-sdk/src/notification.rs](../../crates/connector-sdk/src/notification.rs). **Test definitions:** [ctrl/tests/oi_watch_notification_conformance.rs](../../ctrl/tests/oi_watch_notification_conformance.rs); [ctrl/src/personal.rs](../../ctrl/src/personal.rs).

**Verification limit:** Source inspected; controller tests use notification fixtures and were not run. Physical permission/delivery and complete desktop acceptance are not established here.

**Account relation:** [[central]] · [capability reading](central.html#q3/interaction).

<a id="cap-central-guided-ground-authoring"></a>
### Recover and review a coherent project account

`cap.central.guided-ground-authoring` · #capability #central #intent

A person needs scattered intention recovered into a clear account, with attention concentrated on consequential judgments. The intended journey presents the product definition, desired encounters, source basis and unresolved decisions together, then supports review and recognition of the proposed ground.

**Status:** intent-only as an integrated Central experience. Native inspection and relation planning supply parts of the foundation. The authoring Skills can be exercised now, as in this consolidation; a complete guided native experience still needs development. Code and test references are therefore absent for this target rather than borrowed from its supporting parts.

**Source:** [reason-first authored-ground intent](../../../github-recovery-mirror/mirror/Central/issues/71.json); [authored-ground specification](../../docs/PROJECTCENTRAL-AUTHORED-GROUND.md). **Account relation:** [bring an existing project into view](central.html#q2/starting).

## Follow a capability through the account

The [What seed](central.html#whole/what) defines Central's recursive filesystem home. [Project entry](central.html#q2/starting) develops the experience of bringing existing work into that home. [Recognising authored ground](#cap-central-authored-ground) identifies the native inspect/plan/apply power supporting that experience, links its actual implementation and checks, and returns to the account that governs its purpose. A gap in guided authorship can therefore lead to a bounded product change without losing the original need.

For a cross-product question, inspect a relevant suite-view cell alongside the capability. For example, Central's `H0->A2` ground-context cell asks about the relation to AIKit. Source-reading and authored-ground capabilities provide a concrete basis for examining that question; they do not by themselves prove complete AIKit disclosure. Preserve the original cell's reading while recording current evidence and the specific integration gap at their own sources.

## Relational context: how and why

The capability entries answer what a person can do and what result to expect. The suite relational view asks how Central can support, receive from or remain usefully indirect to the other products. It is an anchoring set of directed questions for checking architecture and finding missing capability relations. A relation code expresses the source-defined relational reading; capability verification is recorded separately above.

The `suite-relations` view uses the same CSV contract as `product-field`. Its H/A identifiers retain the human-facing and agent-facing orientations of the six products. Source-defined relation readings and annotations remain attached to each determination in `extensions`; coverage is independent of implementation status. The manifest declares the selected scope and axis meaning.

The `suite-relations` view uses the same CSV contract as `product-field`. Its H/A identifiers retain the human-facing and agent-facing orientations of the six products. Source-defined relation readings and annotations remain attached to each determination in `extensions`; coverage is independent of implementation status. The manifest declares the selected scope and axis meaning.

Use a product relation to examine a concrete dependency or return, then link to the corresponding capability and its owning source. An unimplemented possibility can remain visible as such; integration distance may be an intentional design choice.

### Original Central relational grid

| from / to | H0 | H1 | H2 | H3 | H4 | H5 | A0 | A1 | A2 | A3 | A4 | A5 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| H0 | I | H | L | L | W | H | H | H | L | L | W | H |
| H1 | H | — | — | — | — | — | H | — | — | — | — | — |
| H2 | L | — | — | — | — | — | L | — | — | — | — | — |
| H3 | L | — | — | — | — | — | L | — | — | — | — | — |
| H4 | W | — | — | — | — | — | W | — | — | — | — | — |
| H5 | H | — | — | — | — | — | H | — | — | — | — | — |
| A0 | H | H | L | L | W | H | I | H | L | L | W | H |
| A1 | H | — | — | — | — | — | H | — | — | — | — | — |
| A2 | L | — | — | — | — | — | L | — | — | — | — | — |
| A3 | L | — | — | — | — | — | L | — | — | — | — | — |
| A4 | W | — | — | — | — | — | W | — | — | — | — | — |
| A5 | H | — | — | — | — | — | H | — | — | — | — | — |

The em dash means outside this slice. All included cells are fully retained in the lossless CSV appendix below.

## CSV convention and complete source

The `suite-relations` view uses the same CSV contract as `product-field`. Its H/A identifiers retain the human-facing and agent-facing orientations of the six products. Source-defined relation readings and annotations remain attached to each determination in `extensions`; coverage is independent of implementation status. The manifest declares the selected scope and axis meaning.

[Download the complete CSV](capability-matrix.csv). The block below is the exact, lossless CSV content.

```csv
id,record_type,view_id,row_id,column_id,capability_refs,need,operation,outcome,implementation_status,standing,source_refs,code_refs,test_refs,account_ref,relation,coverage,extensions,question
cap.central.root,capability,,,,[],A person needs a durable place for personal source and ordinary projects that tools can find consistently.,"central.root, central.init and central.doctor resolve, initialise and inspect the selected root.",The root contains Control and Work with structured health diagnostics. Repeated initialisation preserves existing authored material.,source-inspected; Source and test definitions inspected; these tests were not executed in this documentation pass. Desktop and owner-machine acceptance is not established here.,agent-inference,../github-recovery-mirror/mirror/Central/issues/2.json;../github-recovery-mirror/mirror/Central/issues/19.json,ctrl/src/root.rs;ctrl/src/action.rs,ctrl/tests/cli_contract.rs;ctrl/tests/product_acceptance.rs,central.html#q1/entry,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""c305f22c02710fb8e246eac3ccd50651fa12f22d20da8c42c2c126e60530ec66"", ""cli_commands"": [""central.doctor"", ""central.init"", ""central.root""], ""cli_exposure"": {""kind"": ""direct"", ""reason"": ""The listed native Action identities are invocable through ctrl action run and discoverable through ctrl action list.""}, ""maintenance"": {""updated_at"": ""2026-09-09"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/Central/issues/2.json"", ""../github-recovery-mirror/mirror/Central/issues/19.json"", ""central:74258b3"", ""central:d357e94"", ""reconcile:central-pr-132"", ""cradle:Central:w2-w4:PR-133""], ""code_basis"": {""ctrl/src/root.rs"": ""eb209d6ba1feef679047dd687e395089df63611b119d11e0380e9a52882f62d4"", ""ctrl/src/action.rs"": ""8956891ff55e96514266ca3ad00dbf80a0902f37e0d5a1fe18dadfe59cb6ca1b""}}, ""last_reconciled_at"": ""2026-09-09T15:11:26.113026+00:00""}",
cap.central.action-discovery,capability,,,,[],A person or agent needs to discover what this installed Central can actually do before choosing a command.,ctrl capabilities and action.list expose the native Action registry; the guided picker helps select an operation and its inputs.,"Callers receive named operations, input definitions, mutation classes and capability requirements. The same canonical operation can be selected through different surfaces.",source-inspected; Source and test definitions inspected; these tests were not executed in this documentation pass. Desktop and owner-machine acceptance is not established here.,agent-inference,../github-recovery-mirror/mirror/Central/issues/3.json;../github-recovery-mirror/mirror/Central/issues/7.json;../github-recovery-mirror/mirror/Central/issues/114.json,ctrl/src/action.rs;ctrl/src/picker.rs;ctrl/src/cli.rs,ctrl/tests/cli_doorway.rs;ctrl/tests/picker.rs,central.html#q3/interaction,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""c305f22c02710fb8e246eac3ccd50651fa12f22d20da8c42c2c126e60530ec66"", ""cli_commands"": [""action.list""], ""cli_exposure"": {""kind"": ""direct"", ""reason"": ""The listed native Action identities are invocable through ctrl action run and discoverable through ctrl action list.""}, ""maintenance"": {""updated_at"": ""2026-09-09"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/Central/issues/3.json"", ""../github-recovery-mirror/mirror/Central/issues/7.json"", ""../github-recovery-mirror/mirror/Central/issues/114.json"", ""central:74258b3"", ""central:d357e94"", ""EpiLogos/Central#127 (W10 V2: agent-set/world persistence Actions + attribution seam; cli/now/agent-profile carriers moved — code bases re-reviewed at this revision)"", ""reconcile:central-pr-132"", ""cradle:Central:w2-w4:PR-133""], ""code_basis"": {""ctrl/src/action.rs"": ""8956891ff55e96514266ca3ad00dbf80a0902f37e0d5a1fe18dadfe59cb6ca1b"", ""ctrl/src/picker.rs"": ""b1c04622a2118b28763d9e9c5a747a8b2523d943a741ebbcf868ec5686af8665"", ""ctrl/src/cli.rs"": ""6bc0bd5210e98012beda1df9d82ca03db3af956396e2f6c2fcc428c912501422""}}, ""last_reconciled_at"": ""2026-09-09T15:11:26.113026+00:00""}",
cap.central.work-entry,capability,,,,[],A person wants to move from a project name to the actual working directory without maintaining a duplicate project catalogue.,work.list and work.search discover ordinary Work entries; work.open and work.reveal invoke the native opening or revealing Port.,The result identifies the selected filesystem project and native provider outcome. Opening and revealing depend on an available native Connector.,source-inspected; Selected native suites passed 2026-09-06: work_entry. Other referenced tests were not rerun; physical desktop and owner-machine effects remain outside this selection.,agent-inference,../github-recovery-mirror/mirror/Central/issues/6.json;../github-recovery-mirror/mirror/Central/issues/14.json;../github-recovery-mirror/mirror/Central/issues/15.json,ctrl/src/action.rs,ctrl/tests/work_entry.rs,central.html#q1/entry,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""c305f22c02710fb8e246eac3ccd50651fa12f22d20da8c42c2c126e60530ec66"", ""cli_commands"": [""work.list"", ""work.open"", ""work.reveal"", ""work.search""], ""cli_exposure"": {""kind"": ""direct"", ""reason"": ""The listed native Action identities are invocable through ctrl action run and discoverable through ctrl action list.""}, ""maintenance"": {""updated_at"": ""2026-09-09"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/Central/issues/6.json"", ""../github-recovery-mirror/mirror/Central/issues/14.json"", ""../github-recovery-mirror/mirror/Central/issues/15.json"", ""central:74258b3"", ""central:d357e94"", ""reconcile:central-pr-132"", ""cradle:Central:w2-w4:PR-133""], ""code_basis"": {""ctrl/src/action.rs"": ""8956891ff55e96514266ca3ad00dbf80a0902f37e0d5a1fe18dadfe59cb6ca1b""}}, ""last_reconciled_at"": ""2026-09-09T15:11:26.113026+00:00""}",
cap.central.control-reading,capability,,,,[],"People and agents need to consult durable preferences, collaboration guidance and machine declarations where they are authored.",control.open selects a Control area; control.search retrieves matching eligible source through the Control read model.,"Results point back to ordinary authored files. Retrieval honours excluded subtrees, so a broad search can remain useful without exposing every personal file.",source-inspected; Selected native suites passed 2026-09-06: control_source. Other referenced tests were not rerun; physical desktop and owner-machine effects remain outside this selection.,agent-inference,../github-recovery-mirror/mirror/Central/issues/5.json;../github-recovery-mirror/mirror/Central/issues/12.json,ctrl/src/action.rs;ctrl/src/control.rs,ctrl/tests/control_source.rs;ctrl/tests/product_acceptance.rs,central.html#q1/entry,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""c305f22c02710fb8e246eac3ccd50651fa12f22d20da8c42c2c126e60530ec66"", ""cli_commands"": [""control.open"", ""control.search""], ""cli_exposure"": {""kind"": ""direct"", ""reason"": ""The listed native Action identities are invocable through ctrl action run and discoverable through ctrl action list.""}, ""maintenance"": {""updated_at"": ""2026-09-09"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/Central/issues/5.json"", ""../github-recovery-mirror/mirror/Central/issues/12.json"", ""central:74258b3"", ""central:d357e94"", ""reconcile:central-pr-132"", ""cradle:Central:w2-w4:PR-133""], ""code_basis"": {""ctrl/src/action.rs"": ""8956891ff55e96514266ca3ad00dbf80a0902f37e0d5a1fe18dadfe59cb6ca1b"", ""ctrl/src/control.rs"": ""8434c710179f59f9d8dee7832f4848cf238ed90b30b096f75df0e9506b1d30bb""}}, ""last_reconciled_at"": ""2026-09-09T15:11:26.113026+00:00""}",
cap.central.machine-declaration,capability,,,,[],A person needs to state the environment they rely on so it can be inspected and recovered later.,machine.declaration reads a versioned machine-role source; machine.inspect asks available providers for the observed environment.,"The declaration records desired capabilities, packages, configuration and services; inspection supplies current observations for comparison. Editing the authored declaration changes the next reading directly.",source-inspected; Source and test definitions inspected; these tests were not executed in this documentation pass. Desktop and owner-machine acceptance is not established here.,agent-inference,../github-recovery-mirror/mirror/Central/issues/8.json;../github-recovery-mirror/mirror/Central/issues/13.json,ctrl/src/machine.rs,ctrl/tests/machine_declaration.rs;ctrl/tests/machine_plan.rs,central.html#q1/environment,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""c305f22c02710fb8e246eac3ccd50651fa12f22d20da8c42c2c126e60530ec66"", ""cli_commands"": [""machine.declaration""], ""cli_exposure"": {""kind"": ""direct"", ""reason"": ""The listed native Action identities are invocable through ctrl action run and discoverable through ctrl action list.""}, ""maintenance"": {""updated_at"": ""2026-09-08"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/Central/issues/8.json"", ""../github-recovery-mirror/mirror/Central/issues/13.json"", ""central:74258b3"", ""central:d357e94"", ""reconcile:central-pr-132""], ""code_basis"": {""ctrl/src/machine.rs"": ""0962157ecb5143e643e30b0c9f95d09ac87adcf47899a8471537162f9ed23db4""}}, ""last_reconciled_at"": ""2026-09-08T11:52:07.000112+00:00""}",
cap.central.machine-reconciliation,capability,,,,[],A person needs to see what is missing and apply supported changes with a clear account of the result.,machine.plan compares desired and observed state; machine.apply delegates selected changes through Ports; machine.verify re-inspects the result.,"A structured plan distinguishes satisfied, changeable and unsupported requirements. Application reports complete, partial or failed outcomes and verification mismatches.",source-inspected; Source inspected; controller tests include fixture providers and were not run. Real workstation/server provider acceptance remains a separate physical gate in issues 16 and 17.,agent-inference,../github-recovery-mirror/mirror/Central/issues/9.json;../github-recovery-mirror/mirror/Central/issues/10.json;../github-recovery-mirror/mirror/Central/issues/16.json;../github-recovery-mirror/mirror/Central/issues/17.json,ctrl/src/machine.rs,ctrl/tests/machine_plan.rs;ctrl/tests/machine_reconciliation.rs,central.html#q1/environment,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""c305f22c02710fb8e246eac3ccd50651fa12f22d20da8c42c2c126e60530ec66"", ""cli_commands"": [""machine.account"", ""machine.apply"", ""machine.inspect"", ""machine.plan"", ""machine.verify"", ""machine.adopt-current""], ""cli_exposure"": {""kind"": ""direct"", ""reason"": ""The listed native Action identities are invocable through ctrl action run and discoverable through ctrl action list.""}, ""maintenance"": {""updated_at"": ""2026-09-08"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/Central/issues/9.json"", ""../github-recovery-mirror/mirror/Central/issues/10.json"", ""../github-recovery-mirror/mirror/Central/issues/16.json"", ""../github-recovery-mirror/mirror/Central/issues/17.json"", ""central:6167493"", ""reconcile:central-pr-132""], ""code_basis"": {""ctrl/src/machine.rs"": ""0962157ecb5143e643e30b0c9f95d09ac87adcf47899a8471537162f9ed23db4""}}, ""last_reconciled_at"": ""2026-09-08T11:52:07.000112+00:00""}",
cap.central.machine-recovery,capability,,,,[],"After a machine change or loss, a person needs a repeatable route back to their declared tools and configuration.","Recovery planning reads the role recovery declaration, previews synchronisation and environment reconciliation, then recovery delegates through providers and verifies.","The recovery report retains per-stage results and remaining differences. Recovery depends on the configured source, synchronisation and environment providers.",source-inspected; Source inspected; recovery controller tests include provider fixtures and were not run. Issue 21 records hosted provider proof; named owner-machine acceptance requires its own evidence.,agent-inference,../github-recovery-mirror/mirror/Central/issues/21.json;../github-recovery-mirror/mirror/Central/issues/16.json;../github-recovery-mirror/mirror/Central/issues/17.json,ctrl/src/recovery.rs,ctrl/tests/recovery.rs,central.html#q2/restore,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""c305f22c02710fb8e246eac3ccd50651fa12f22d20da8c42c2c126e60530ec66"", ""cli_commands"": [""central.recover"", ""central.recovery.plan""], ""cli_exposure"": {""kind"": ""direct"", ""reason"": ""The listed native Action identities are invocable through ctrl action run and discoverable through ctrl action list.""}, ""maintenance"": {""updated_at"": ""2026-09-08"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/Central/issues/21.json"", ""../github-recovery-mirror/mirror/Central/issues/16.json"", ""../github-recovery-mirror/mirror/Central/issues/17.json"", ""reconcile:central-pr-132""], ""code_basis"": {""ctrl/src/recovery.rs"": ""e0433ff26dedfcf0b15045a1be64a09b5f0f46934055e8d20ad7edbdc3b193ec""}}, ""last_reconciled_at"": ""2026-09-08T11:52:07.000112+00:00""}",
cap.central.projectcentral-lifecycle,capability,,,,[],A project needs stable source relationships and places for human ground and collaborative material as it develops.,"ProjectCentral inspect, init, adopt and migrate operations establish identity and selected source bindings with previews and provenance.",An ordinary project gains the ProjectCentral filesystem convention and root registration. Existing compatible sources can be adopted in place or explicitly migrated with retained history.,source-inspected; Source and test definitions inspected; these tests were not executed in this documentation pass. Desktop and owner-machine acceptance is not established here.,agent-inference,../github-recovery-mirror/mirror/Central/issues/65.json;../github-recovery-mirror/mirror/Central/issues/66.json;../github-recovery-mirror/mirror/Central/issues/67.json,ctrl/src/projectcentral.rs;ctrl/src/projectcentral_ops.rs,ctrl/src/projectcentral_ops.rs,central.html#q2/starting,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""c305f22c02710fb8e246eac3ccd50651fa12f22d20da8c42c2c126e60530ec66"", ""cli_commands"": [""central.recognize"", ""projectcentral.adopt"", ""projectcentral.adopt.preview"", ""projectcentral.doctor"", ""projectcentral.init"", ""projectcentral.inspect"", ""projectcentral.migrate"", ""projectcentral.migrate.preview""], ""cli_exposure"": {""kind"": ""direct"", ""reason"": ""The listed native Action identities are invocable through ctrl action run and discoverable through ctrl action list.""}, ""maintenance"": {""updated_at"": ""2026-09-08"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/Central/issues/65.json"", ""../github-recovery-mirror/mirror/Central/issues/66.json"", ""../github-recovery-mirror/mirror/Central/issues/67.json"", ""reconcile:central-pr-132""], ""code_basis"": {""ctrl/src/projectcentral.rs"": ""18bfe9bf4aebb435b71b020f99224e9abdf8bc71155265dd0fe90466bce4c88c"", ""ctrl/src/projectcentral_ops.rs"": ""6452207a8a1279f6fbf9bb02462d2e3af060bbce8e2878d17abb83f136eeef53""}}, ""last_reconciled_at"": ""2026-09-08T11:53:39.350344+00:00""}",
cap.central.authored-ground,capability,,,,[],"A person wants existing vision, UX and design decisions available to collaborators without rewriting every source.","projectcentral.ground.inspect and plan discover candidates and propose treatments; apply records an explicitly accepted provenance, standing and role relation.",The recognised source becomes addressable as project ground while retaining its actual path and bytes. The current planner recommends source treatment; coherent intent recovery and authoring still require a human/agent workflow.,source-inspected; Selected native suites passed 2026-09-06: projectcentral_ground_actions. Other referenced tests were not rerun; physical desktop and owner-machine effects remain outside this selection.,agent-inference,../github-recovery-mirror/mirror/Central/issues/70.json;../github-recovery-mirror/mirror/Central/issues/104.json,ctrl/src/projectcentral_ground.rs,ctrl/tests/projectcentral_ground_actions.rs;ctrl/tests/projectcentral_ground_portable_real.rs,central.html#q1/ground,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""c305f22c02710fb8e246eac3ccd50651fa12f22d20da8c42c2c126e60530ec66"", ""cli_commands"": [""projectcentral.ground.apply"", ""projectcentral.ground.inspect"", ""projectcentral.ground.plan""], ""cli_exposure"": {""kind"": ""direct"", ""reason"": ""The listed native Action identities are invocable through ctrl action run and discoverable through ctrl action list.""}, ""maintenance"": {""updated_at"": ""2026-09-08"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/Central/issues/70.json"", ""../github-recovery-mirror/mirror/Central/issues/104.json"", ""reconcile:central-pr-132""], ""code_basis"": {""ctrl/src/projectcentral_ground.rs"": ""acb84b452b644b0a9efd074f4937e15d9dd1abb122cce63a9b815f6d1f7f3071""}}, ""last_reconciled_at"": ""2026-09-08T11:52:07.000112+00:00""}",
cap.central.world-map,capability,,,,[],"A person needs to locate work, current threads and source areas, including places whose structure is incomplete.","central.world and central.world.project read Control, projects, source relations, Flow, NOW and related wiki-space state.","A structured map centres on the root or one project and reports missing, partial and faulty areas alongside the available material. It supplies a navigable consumer read model.",source-inspected; Selected native suites passed 2026-09-06: world_map. Other referenced tests were not rerun; physical desktop and owner-machine effects remain outside this selection.,agent-inference,../github-recovery-mirror/mirror/Central/issues/51.json;../github-recovery-mirror/mirror/Central/issues/65.json;../github-recovery-mirror/mirror/Central/issues/76.json;../github-recovery-mirror/mirror/Central/issues/93.json,ctrl/src/world_map.rs,ctrl/tests/world_map.rs,central.html#q1/entry,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""c305f22c02710fb8e246eac3ccd50651fa12f22d20da8c42c2c126e60530ec66"", ""cli_commands"": [""central.world"", ""central.world.project""], ""cli_exposure"": {""kind"": ""direct"", ""reason"": ""The listed native Action identities are invocable through ctrl action run and discoverable through ctrl action list.""}, ""maintenance"": {""updated_at"": ""2026-09-08"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/Central/issues/51.json"", ""../github-recovery-mirror/mirror/Central/issues/65.json"", ""../github-recovery-mirror/mirror/Central/issues/76.json"", ""../github-recovery-mirror/mirror/Central/issues/93.json"", ""EpiLogos/Central#126 (W10 V1: central.pasu/v1 identity grammar; world-map/source-change/agent-profile carriers moved — code bases re-reviewed at this revision)"", ""reconcile:central-pr-132""], ""code_basis"": {""ctrl/src/world_map.rs"": ""f3427aa7efc243249bbf370f2cbf7d47c54de046a10d8b50a7d193345cd502ff""}}, ""last_reconciled_at"": ""2026-09-08T11:52:07.000112+00:00""}",
cap.central.reproject,capability,,,,[],An existing project should gain current facilities while preserving its accumulated files.,central.world.reproject.plan lists missing canonical scaffold; reproject.apply stamps only missing structure.,The receipt identifies added scaffolding and retained existing material. This additive repair covers missing structure; full source reorganisation and installed-world migration remain broader work.,source-inspected; Selected native suites passed 2026-09-06: world_map. Other referenced tests were not rerun; physical desktop and owner-machine effects remain outside this selection.,agent-inference,../github-recovery-mirror/mirror/Central/issues/67.json;../github-recovery-mirror/mirror/Central/issues/76.json,ctrl/src/world_map.rs,ctrl/tests/world_map.rs,central.html#q3/tree,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""c305f22c02710fb8e246eac3ccd50651fa12f22d20da8c42c2c126e60530ec66"", ""cli_commands"": [""central.world.reproject.apply"", ""central.world.reproject.plan""], ""cli_exposure"": {""kind"": ""direct"", ""reason"": ""The listed native Action identities are invocable through ctrl action run and discoverable through ctrl action list.""}, ""maintenance"": {""updated_at"": ""2026-09-08"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/Central/issues/67.json"", ""../github-recovery-mirror/mirror/Central/issues/76.json"", ""EpiLogos/Central#126 (W10 V1: central.pasu/v1 identity grammar; world-map/source-change/agent-profile carriers moved — code bases re-reviewed at this revision)"", ""reconcile:central-pr-132""], ""code_basis"": {""ctrl/src/world_map.rs"": ""f3427aa7efc243249bbf370f2cbf7d47c54de046a10d8b50a7d193345cd502ff""}}, ""last_reconciled_at"": ""2026-09-08T11:52:07.000112+00:00""}",
cap.central.source-reading,capability,,,,[],A reader needs to inspect the material underlying an account or decision and know which revision they are seeing.,read_world_source resolves one participating SourceRef and checks retrieval eligibility.,"The reading contains source content, exact revision and provenance. Masked or non-participating sources produce an explicit refusal.",source-inspected; Selected native suites passed 2026-09-06: world_source_actions. Other referenced tests were not rerun; physical desktop and owner-machine effects remain outside this selection.,agent-inference,../github-recovery-mirror/mirror/Central/issues/70.json;../github-recovery-mirror/mirror/Central/issues/89.json;../github-recovery-mirror/mirror/Central/issues/100.json,ctrl/src/world_source.rs,ctrl/tests/world_source_actions.rs,central.html#q1/ground,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""c305f22c02710fb8e246eac3ccd50651fa12f22d20da8c42c2c126e60530ec66"", ""cli_commands"": [""projectcentral.source.read""], ""cli_exposure"": {""kind"": ""direct"", ""reason"": ""The listed native Action identities are invocable through ctrl action run and discoverable through ctrl action list.""}, ""maintenance"": {""updated_at"": ""2026-09-08"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/Central/issues/70.json"", ""../github-recovery-mirror/mirror/Central/issues/89.json"", ""../github-recovery-mirror/mirror/Central/issues/100.json"", ""reconcile:central-pr-132""], ""code_basis"": {""ctrl/src/world_source.rs"": ""bdfc596671d3c92377ddcb02b6a92e1210bf5de5ada71dc6f340428bce30fe0a""}}, ""last_reconciled_at"": ""2026-09-08T11:52:07.000112+00:00""}",
cap.central.source-editing,capability,,,,[],"A person and an agent may both change a file between reads, so each edit needs a known basis.","write_world_source checks the expected content revision and actor attribution, then writes or returns the appropriate proposal.",Accepted edits emit attributed source-change records. A stale basis produces a conflict; agent changes to recognised human ground return through proposal authority.,source-inspected; Selected native suites passed 2026-09-06: world_source_actions. Other referenced tests were not rerun; physical desktop and owner-machine effects remain outside this selection.,agent-inference,../github-recovery-mirror/mirror/Central/issues/70.json;../github-recovery-mirror/mirror/Central/issues/89.json;../github-recovery-mirror/mirror/Central/issues/100.json,ctrl/src/world_source.rs,ctrl/tests/world_source_actions.rs,central.html#q2/recognised-ground,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""c305f22c02710fb8e246eac3ccd50651fa12f22d20da8c42c2c126e60530ec66"", ""cli_commands"": [""projectcentral.source.return"", ""projectcentral.source.return_accept"", ""projectcentral.source.return_read"", ""projectcentral.source.return_reject"", ""projectcentral.source.returns"", ""projectcentral.source.write""], ""cli_exposure"": {""kind"": ""direct"", ""reason"": ""The listed native Action identities are invocable through ctrl action run and discoverable through ctrl action list.""}, ""maintenance"": {""updated_at"": ""2026-09-08"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/Central/issues/70.json"", ""../github-recovery-mirror/mirror/Central/issues/89.json"", ""../github-recovery-mirror/mirror/Central/issues/100.json"", ""reconcile:central-pr-132""], ""code_basis"": {""ctrl/src/world_source.rs"": ""bdfc596671d3c92377ddcb02b6a92e1210bf5de5ada71dc6f340428bce30fe0a""}}, ""last_reconciled_at"": ""2026-09-08T11:53:39.350344+00:00""}",
cap.central.source-change,capability,,,,[],People need to keep using ordinary editors while later sessions discover relevant source changes.,Project and Control reconciliation observes content revisions; horizon reads expose ordered changes since a consumer cursor and recover offline edits.,"Consumers receive source identities, revisions and retained standing without reading every payload. The horizon records change; interpretation and follow-up belong to its consumers.",source-inspected; Selected native suites passed 2026-09-06: source_change_horizon. Other referenced tests were not rerun; physical desktop and owner-machine effects remain outside this selection.,agent-inference,../github-recovery-mirror/mirror/Central/issues/89.json,ctrl/src/source_horizon.rs,ctrl/tests/source_change_horizon.rs;ctrl/tests/source_change_authority_override.rs,central.html#q3/contracts,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""c305f22c02710fb8e246eac3ccd50651fa12f22d20da8c42c2c126e60530ec66"", ""cli_commands"": [""projectcentral.change.ack"", ""projectcentral.change.horizon"", ""projectcentral.change.reconcile""], ""cli_exposure"": {""kind"": ""direct"", ""reason"": ""The listed native Action identities are invocable through ctrl action run and discoverable through ctrl action list.""}, ""maintenance"": {""updated_at"": ""2026-09-09"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/Central/issues/89.json"", ""EpiLogos/Central#126 (W10 V1: central.pasu/v1 identity grammar; world-map/source-change/agent-profile carriers moved — code bases re-reviewed at this revision)"", ""reconcile:central-pr-132"", ""cradle:Central:w2-w4:PR-133""], ""code_basis"": {""ctrl/src/source_horizon.rs"": ""4d77d9c11c442e45d50ca844d67d5eca060b7beea0abf07643c9699f97cbcf96""}}, ""last_reconciled_at"": ""2026-09-09T15:11:26.113026+00:00""}",
cap.central.source-history,capability,,,,[],A person needs to understand how a source changed or retrieve a prior version before deciding what to restore.,"projectcentral.source.history, compare and recovery.preview request bounded history from an optional native provider and check the current recovery basis.","The caller receives revision entries, differences or historical content. Applying recovered content uses the normal source-write/proposal path; the preview itself leaves current content unchanged.","source-inspected; Source inspected; real Git connector test and controller fixture test inspected, neither executed. Full provider-to-recovery user acceptance is not established here.",agent-inference,../github-recovery-mirror/mirror/Central/issues/100.json,ctrl/src/source_history.rs;connectors/git-sync/src/source_history.rs,connectors/git-sync/src/source_history.rs;ctrl/src/source_history.rs,central.html#q3/contracts,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""c305f22c02710fb8e246eac3ccd50651fa12f22d20da8c42c2c126e60530ec66"", ""cli_commands"": [""projectcentral.source.compare"", ""projectcentral.source.history"", ""projectcentral.source.recovery.preview""], ""cli_exposure"": {""kind"": ""direct"", ""reason"": ""The listed native Action identities are invocable through ctrl action run and discoverable through ctrl action list.""}, ""maintenance"": {""updated_at"": ""2026-09-08"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/Central/issues/100.json"", ""reconcile:central-pr-132""], ""code_basis"": {""ctrl/src/source_history.rs"": ""1407fa32db1b3f3ede7585e6602b17c58fdb843c99391f10b0c5f2924c9c4401"", ""connectors/git-sync/src/source_history.rs"": ""2ead2869e3cf02ed79036cfd25a682188cc1886a9cd59251e0bffa4d6c065a76""}}, ""last_reconciled_at"": ""2026-09-08T11:52:07.000112+00:00""}",
cap.central.now,capability,,,,[],"Current scratch, questions and handoffs need to remain available after a chat ends.","projectcentral.now.init and inspect establish/read the temporal field; now.return and update record attributed questions, notes and handoffs with lifecycle and external references.","The next session can recover human scratch and bounded agent returns from files, including open work and supporting evidence references.",source-inspected; Selected native suites passed 2026-09-06: projectcentral_now. Other referenced tests were not rerun; physical desktop and owner-machine effects remain outside this selection.,agent-inference,../github-recovery-mirror/mirror/Central/issues/74.json,ctrl/src/projectcentral_now.rs,ctrl/tests/projectcentral_now.rs;ctrl/tests/projectcentral_now_portable_real.rs,central.html#q2/resume,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""c305f22c02710fb8e246eac3ccd50651fa12f22d20da8c42c2c126e60530ec66"", ""cli_commands"": [""projectcentral.now.init"", ""projectcentral.now.inspect"", ""projectcentral.now.return"", ""projectcentral.now.update""], ""cli_exposure"": {""kind"": ""direct"", ""reason"": ""The listed native Action identities are invocable through ctrl action run and discoverable through ctrl action list.""}, ""maintenance"": {""updated_at"": ""2026-09-08"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/Central/issues/74.json"", ""EpiLogos/Central#127 (W10 V2: agent-set/world persistence Actions + attribution seam; cli/now/agent-profile carriers moved — code bases re-reviewed at this revision)"", ""reconcile:central-pr-132""], ""code_basis"": {""ctrl/src/projectcentral_now.rs"": ""b0ce2abd5829f870c66c7a409e3a528cb5498e47df491ad7e07dabb547d1e0fc""}}, ""last_reconciled_at"": ""2026-09-08T11:52:07.000112+00:00""}",
cap.central.day-rollover,capability,,,,[],A person needs a dated account of work while keeping unfinished material available and promoting useful findings deliberately.,now.rollover snapshots the chosen local civil day and carries live material; now.promote records the move from temporal material toward a durable owner.,"DAY records preserve source state and carry-forward decisions. Eligible completed clutter can clear; protected material remains. Agent promotion returns material to wiki ownership, where semantic integration is a later operation.",source-inspected; Selected native suites passed 2026-09-06: projectcentral_now. Other referenced tests were not rerun; physical desktop and owner-machine effects remain outside this selection.,agent-inference,../github-recovery-mirror/mirror/Central/issues/74.json;../github-recovery-mirror/mirror/Central/issues/93.json,ctrl/src/projectcentral_now.rs,ctrl/tests/projectcentral_now.rs;ctrl/tests/projectcentral_now_portable_real.rs,central.html#q2/resume,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""c305f22c02710fb8e246eac3ccd50651fa12f22d20da8c42c2c126e60530ec66"", ""cli_commands"": [""projectcentral.now.promote"", ""projectcentral.now.rollover""], ""cli_exposure"": {""kind"": ""direct"", ""reason"": ""The listed native Action identities are invocable through ctrl action run and discoverable through ctrl action list.""}, ""maintenance"": {""updated_at"": ""2026-09-08"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/Central/issues/74.json"", ""../github-recovery-mirror/mirror/Central/issues/93.json"", ""EpiLogos/Central#127 (W10 V2: agent-set/world persistence Actions + attribution seam; cli/now/agent-profile carriers moved — code bases re-reviewed at this revision)"", ""reconcile:central-pr-132""], ""code_basis"": {""ctrl/src/projectcentral_now.rs"": ""b0ce2abd5829f870c66c7a409e3a528cb5498e47df491ad7e07dabb547d1e0fc""}}, ""last_reconciled_at"": ""2026-09-08T11:52:07.000112+00:00""}",
cap.central.flow,capability,,,,[],"A developing idea needs continuity across sessions, days and changes of filename.",Flow create/adopt/read/write/rename/lifecycle/history operations maintain a stable FlowRef and revision-safe ordinary-file content.,"One thread retains identity while active, dormant or closed; renaming preserves continuity and DAY snapshots retain earlier states. Desktop conversation and knowledge interpretation consume this source lifecycle.",source-inspected; Selected native suites passed 2026-09-06: projectcentral_flow. Other referenced tests were not rerun; physical desktop and owner-machine effects remain outside this selection.,agent-inference,../github-recovery-mirror/mirror/Central/issues/93.json,ctrl/src/projectcentral_flow.rs,ctrl/tests/projectcentral_flow.rs,central.html#q2/develop-thought,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""c305f22c02710fb8e246eac3ccd50651fa12f22d20da8c42c2c126e60530ec66"", ""cli_commands"": [""projectcentral.flow.adopt"", ""projectcentral.flow.create"", ""projectcentral.flow.history"", ""projectcentral.flow.inspect"", ""projectcentral.flow.lifecycle"", ""projectcentral.flow.list"", ""projectcentral.flow.read"", ""projectcentral.flow.rename"", ""projectcentral.flow.write"", ""projectcentral.flow.now""], ""cli_exposure"": {""kind"": ""direct"", ""reason"": ""The listed native Action identities are invocable through ctrl action run and discoverable through ctrl action list.""}, ""maintenance"": {""updated_at"": ""2026-09-09"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/Central/issues/93.json"", ""reconcile:central-pr-132"", ""cradle:Central:w2-w4:PR-133""], ""code_basis"": {""ctrl/src/projectcentral_flow.rs"": ""084780209c761d37f6d3756f21753b5dafac0ec27d986dd9fe2b321da32558c5""}}, ""last_reconciled_at"": ""2026-09-09T15:11:26.113026+00:00""}",
cap.central.governance,capability,,,,[],A person needs durable collaboration preferences and local engineering rules that can be found and revised in their owning scope.,"Root/project governance inspection discovers recognised sources and native candidates; accepted relations record scope, roles and provenance. central.template.preview/stamp expose additive source templates; control.engineering-ground.plan/render maintain the derived engineering prompt from its source.",Guidance remains inspectable in its native location with personal and project applicability. Runtime activation and precedence are resolved by the consuming harness/AIKit layer.,source-inspected; Selected native suites passed 2026-09-06: agent_governance. Other referenced tests were not rerun; physical desktop and owner-machine effects remain outside this selection.,agent-inference,../github-recovery-mirror/mirror/Central/issues/72.json;../github-recovery-mirror/mirror/Central/issues/104.json,ctrl/src/agent_governance.rs;ctrl/src/template_stamp.rs;ctrl/src/engineering_ground.rs,ctrl/tests/agent_governance.rs,central.html#q1/practice,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""c305f22c02710fb8e246eac3ccd50651fa12f22d20da8c42c2c126e60530ec66"", ""cli_commands"": [""central.template.preview"", ""central.template.stamp"", ""control.engineering-ground.plan"", ""control.engineering-ground.render""], ""cli_exposure"": {""kind"": ""direct"", ""reason"": ""The listed native Action identities are invocable through ctrl action run and discoverable through ctrl action list.""}, ""maintenance"": {""updated_at"": ""2026-09-08"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/Central/issues/72.json"", ""../github-recovery-mirror/mirror/Central/issues/104.json"", ""central:74258b3"", ""central:d357e94"", ""reconcile:central-pr-132""], ""code_basis"": {""ctrl/src/agent_governance.rs"": ""3dcc5f060786973e6f6695eaf53729b28d48514bfbcde517fd2e7707230b8391"", ""ctrl/src/template_stamp.rs"": ""cdd3b6cad5eeb39868068fbaff8f2bcc80505c8f6ad4978d584c782a27a30645"", ""ctrl/src/engineering_ground.rs"": ""88d8fb7b27b0107a840c29c611ec96f4726af570e0635ca27ad2a25ad1789cb7""}}, ""last_reconciled_at"": ""2026-09-08T11:52:07.000112+00:00""}",
cap.central.skills,capability,,,,[],People need reusable skills to remain discoverable while outdated methods can be withdrawn and later restored.,"control.skills.inspect reads personal, machine and project scope; retire and restore update a skill manifest with lifecycle provenance.",Retirement records actor and reason while retaining the skill body. Source changes participate in the horizon. Contextual Method composition and runtime use are consumer responsibilities.,source-inspected; Selected native suites passed 2026-09-06: control_skills. Other referenced tests were not rerun; physical desktop and owner-machine effects remain outside this selection.,agent-inference,../github-recovery-mirror/mirror/Central/issues/82.json,ctrl/src/control_skills.rs,ctrl/tests/control_skills.rs,central.html#q1/practice,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""c305f22c02710fb8e246eac3ccd50651fa12f22d20da8c42c2c126e60530ec66"", ""cli_commands"": [""control.skills.inspect"", ""control.skills.restore"", ""control.skills.retire""], ""cli_exposure"": {""kind"": ""direct"", ""reason"": ""The listed native Action identities are invocable through ctrl action run and discoverable through ctrl action list.""}, ""maintenance"": {""updated_at"": ""2026-09-08"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/Central/issues/82.json"", ""reconcile:central-pr-132""], ""code_basis"": {""ctrl/src/control_skills.rs"": ""f9ff1bbfd2fd3fe73719a096c5d873a67f9e3751b9c7a71996466ccdc0754dfa""}}, ""last_reconciled_at"": ""2026-09-08T11:52:07.000112+00:00""}",
cap.central.agent-profiles,capability,,,,[],A person needs durable agent configuration that survives sessions and can vary between personal and project work.,agent-profile.list/read/save/remove operate on source-backed profiles; saves and removals use expected revisions where required.,The profile stores references for an existing Agent in its selected scope and returns a write/removal receipt. It is a saved configuration relation; effective runtime resolution requires the consuming agent system.,source-inspected; Source and test definitions inspected; these tests were not executed in this documentation pass. Desktop and owner-machine acceptance is not established here.,agent-inference,../github-recovery-mirror/mirror/Central/issues/108.json;../github-recovery-mirror/mirror/Central/issues/112.json;../github-recovery-mirror/mirror/Central/issues/118.json,ctrl/src/agent_profile_actions.rs;ctrl/src/agent_profile_store.rs;ctrl/src/agent_profile.rs,ctrl/src/agent_profile_actions.rs,central.html#q2/reuse-practice,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""c305f22c02710fb8e246eac3ccd50651fa12f22d20da8c42c2c126e60530ec66"", ""cli_commands"": [""agent-profile.list"", ""agent-profile.read"", ""agent-profile.remove"", ""agent-profile.save"", ""agent-profile.propose""], ""cli_exposure"": {""kind"": ""direct"", ""reason"": ""The listed native Action identities are invocable through ctrl action run and discoverable through ctrl action list.""}, ""maintenance"": {""updated_at"": ""2026-09-09"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/Central/issues/108.json"", ""../github-recovery-mirror/mirror/Central/issues/112.json"", ""../github-recovery-mirror/mirror/Central/issues/118.json"", ""EpiLogos/Central#126 (W10 V1: central.pasu/v1 identity grammar; world-map/source-change/agent-profile carriers moved — code bases re-reviewed at this revision)"", ""reconcile:central-pr-132"", ""cradle:Central:w2-w4:PR-133""], ""code_basis"": {""ctrl/src/agent_profile_actions.rs"": ""0ec8928e0119169522309779f5560b663bd8a894f96fb479e87600d446c6457a"", ""ctrl/src/agent_profile_store.rs"": ""96784994e27b167f79e5b33934f42bfce343457af7076b32e58ae05752d60420"", ""ctrl/src/agent_profile.rs"": ""53ff9454c9f32568a5473fb5e0e6a93b5f8be953f38f776017c33f943c60042a""}}, ""last_reconciled_at"": ""2026-09-09T15:11:26.113026+00:00""}",
cap.central.personal-surface,capability,,,,[],"A person needs an understandable entry into their own preferences, agent guidance, machines and projects.",personal.show reads the actual Central root and projects its principal areas.,"The public read model presents You, Agents, Machines and Work with their filesystem locations for a desktop or other surface to display.",source-inspected; Source and test definitions inspected; these tests were not executed in this documentation pass. Desktop and owner-machine acceptance is not established here.,agent-inference,../github-recovery-mirror/mirror/Central/issues/51.json,ctrl/src/personal.rs,ctrl/src/personal.rs,central.html#q1/entry,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""c305f22c02710fb8e246eac3ccd50651fa12f22d20da8c42c2c126e60530ec66"", ""cli_commands"": [], ""cli_exposure"": {""kind"": ""composed"", ""reason"": ""The personal surface composes Control and World operations; it is not an additional Action identity.""}, ""maintenance"": {""updated_at"": ""2026-09-08"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/Central/issues/51.json"", ""reconcile:central-pr-132""], ""code_basis"": {""ctrl/src/personal.rs"": ""8b61fa887e574b1f09d4cb19635e5fd2ee8e62d85aa6f6940a9c7d21921e4415""}}, ""last_reconciled_at"": ""2026-09-08T11:52:07.000112+00:00""}",
cap.central.control-proposals,capability,,,,[],An agent suggestion needs a visible review step before it becomes lasting personal or collaboration source.,control.propose-change stores proposed content and reason; review-proposal exposes it; apply-proposal records acceptance and writes the selected target.,The person can inspect the proposed change before source mutation. Accepted application updates the actual Control file and retains a proposal receipt.,source-inspected; Source and test definitions inspected; these tests were not executed in this documentation pass. Desktop and owner-machine acceptance is not established here.,agent-inference,../github-recovery-mirror/mirror/Central/issues/51.json,ctrl/src/personal.rs,ctrl/src/personal.rs,central.html#q2/recognised-ground,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""c305f22c02710fb8e246eac3ccd50651fa12f22d20da8c42c2c126e60530ec66"", ""cli_commands"": [], ""cli_exposure"": {""kind"": ""library"", ""reason"": ""The personal source proposal API is a library operation; no proposal Action is declared in the current core CLI registry.""}, ""maintenance"": {""updated_at"": ""2026-09-08"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/Central/issues/51.json"", ""reconcile:central-pr-132""], ""code_basis"": {""ctrl/src/personal.rs"": ""8b61fa887e574b1f09d4cb19635e5fd2ee8e62d85aa6f6940a9c7d21921e4415""}}, ""last_reconciled_at"": ""2026-09-08T11:52:07.000112+00:00""}",
cap.central.notification,capability,,,,[],A task needs to alert the person through the installed machine while retaining which work caused the request.,"personal.notify routes title/body, subject and caller lineage through the UserNotification Port to an available provider.","A provider receipt reports delivery or degradation; the macOS connector uses Notification Center via osascript. Delivery records an attention request, while any subsequent decision remains separately recorded.",source-inspected; Source inspected; controller tests use notification fixtures and were not run. Physical permission/delivery and complete desktop acceptance are not established here.,agent-inference,../github-recovery-mirror/mirror/Central/issues/52.json,ctrl/src/personal.rs;crates/connector-sdk/src/notification.rs,ctrl/tests/oi_watch_notification_conformance.rs;ctrl/src/personal.rs,central.html#q3/interaction,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""c305f22c02710fb8e246eac3ccd50651fa12f22d20da8c42c2c126e60530ec66"", ""cli_commands"": [], ""cli_exposure"": {""kind"": ""library"", ""reason"": ""Notification is a provider-neutral library Port consumed by surfaces/connectors; the core CLI does not declare a notification Action.""}, ""maintenance"": {""updated_at"": ""2026-09-08"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/Central/issues/52.json"", ""reconcile:central-pr-132""], ""code_basis"": {""ctrl/src/personal.rs"": ""8b61fa887e574b1f09d4cb19635e5fd2ee8e62d85aa6f6940a9c7d21921e4415"", ""crates/connector-sdk/src/notification.rs"": ""30f20a2ecb5d6c429332faa0e56bb10df2348a067d35e5ef4226909e46e17b7a""}}, ""last_reconciled_at"": ""2026-09-08T11:52:07.000112+00:00""}",
cap.central.guided-ground-authoring,capability,,,,[],"A person needs an existing project’s scattered intention recovered into a clear account, with attention concentrated on consequential judgments.","Present a source-grounded account from retained files and development refinements, then guide review and recognition of the proposed product ground.","Intended integrated journey: the person sees the concrete product definition, desired encounters, source basis and unresolved decisions in one reviewable account. Native source inspection and relation planning support parts of it; the complete guided experience still needs development.","intent-only; the integrated guided authoring experience is a development target, not an implemented Central Action.",agent-inference,../github-recovery-mirror/mirror/Central/issues/71.json;docs/PROJECTCENTRAL-AUTHORED-GROUND.md,,,central.html#q2/starting,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""c305f22c02710fb8e246eac3ccd50651fa12f22d20da8c42c2c126e60530ec66"", ""cli_commands"": [], ""cli_exposure"": {""kind"": ""intent"", ""reason"": ""The integrated authoring journey remains intended; these documentation tools do not establish it as a native Central Action.""}, ""maintenance"": {""updated_at"": ""2026-09-08"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/Central/issues/71.json"", ""docs/PROJECTCENTRAL-AUTHORED-GROUND.md"", ""reconcile:central-pr-132""], ""code_basis"": {}}, ""last_reconciled_at"": ""2026-09-08T11:52:07.000112+00:00""}",
H0->H0,relation,suite-relations,H0,H0,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,self:Central,I,"{""src_product"": ""Central"", ""dst_product"": ""Central"", ""ql"": """", ""cf_view"": ""CF1"", ""seam"": ""self:Central"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
H0->H1,relation,suite-relations,H0,H1,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,01:grounded-agency,H,"{""src_product"": ""Central"", ""dst_product"": ""Actuation"", ""ql"": ""A1"", ""cf_view"": ""CF2"", ""seam"": ""01:grounded-agency"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/Actuation#4"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
H0->H2,relation,suite-relations,H0,H2,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,02:ground-context,L,"{""src_product"": ""Central"", ""dst_product"": ""AIKit"", ""ql"": """", ""cf_view"": ""CF3"", ""seam"": ""02:ground-context"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/ai-kit#58(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
H0->H3,relation,suite-relations,H0,H3,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,03:ground-development,L,"{""src_product"": ""Central"", ""dst_product"": ""Factory"", ""ql"": """", ""cf_view"": ""CF4"", ""seam"": ""03:ground-development"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/agent-system-design#142(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
H0->H4,relation,suite-relations,H0,H4,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,04:ground-materialisation,W,"{""src_product"": ""Central"", ""dst_product"": ""Workcell"", ""ql"": """", ""cf_view"": ""CF5/CF6-field"", ""seam"": ""04:ground-materialisation"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/Workcell#18(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
H0->H5,relation,suite-relations,H0,H5,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,05/50:ground-synthesis,H,"{""src_product"": ""Central"", ""dst_product"": ""QL"", ""ql"": ""B3|C1"", ""cf_view"": ""CF7"", ""seam"": ""05/50:ground-synthesis"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/QL-MEF#19(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
H0->A0,relation,suite-relations,H0,A0,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,conjugation:Central,H,"{""src_product"": ""Central"", ""dst_product"": ""Central"", ""ql"": ""D1"", ""cf_view"": ""CF1"", ""seam"": ""conjugation:Central"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
H0->A1,relation,suite-relations,H0,A1,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,01:grounded-agency,H,"{""src_product"": ""Central"", ""dst_product"": ""Actuation"", ""ql"": ""D2-transform"", ""cf_view"": ""CF2"", ""seam"": ""01:grounded-agency"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/Actuation#4"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
H0->A2,relation,suite-relations,H0,A2,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,02:ground-context,L,"{""src_product"": ""Central"", ""dst_product"": ""AIKit"", ""ql"": """", ""cf_view"": ""CF3"", ""seam"": ""02:ground-context"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/ai-kit#58(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
H0->A3,relation,suite-relations,H0,A3,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,03:ground-development,L,"{""src_product"": ""Central"", ""dst_product"": ""Factory"", ""ql"": """", ""cf_view"": ""CF4"", ""seam"": ""03:ground-development"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/agent-system-design#142(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
H0->A4,relation,suite-relations,H0,A4,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,04:ground-materialisation,W,"{""src_product"": ""Central"", ""dst_product"": ""Workcell"", ""ql"": """", ""cf_view"": ""CF5/CF6-field"", ""seam"": ""04:ground-materialisation"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/Workcell#18(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
H0->A5,relation,suite-relations,H0,A5,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,05/50:ground-synthesis,H,"{""src_product"": ""Central"", ""dst_product"": ""QL"", ""ql"": ""D2-require|D2-complete"", ""cf_view"": ""CF7"", ""seam"": ""05/50:ground-synthesis"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/QL-MEF#19(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
H1->H0,relation,suite-relations,H1,H0,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,01:grounded-agency,H,"{""src_product"": ""Actuation"", ""dst_product"": ""Central"", ""ql"": ""A1"", ""cf_view"": ""CF2"", ""seam"": ""01:grounded-agency"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/Actuation#4"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
H1->A0,relation,suite-relations,H1,A0,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,01:grounded-agency,H,"{""src_product"": ""Actuation"", ""dst_product"": ""Central"", ""ql"": ""D2-require"", ""cf_view"": ""CF2"", ""seam"": ""01:grounded-agency"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/Actuation#4"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
H2->H0,relation,suite-relations,H2,H0,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,02:ground-context,L,"{""src_product"": ""AIKit"", ""dst_product"": ""Central"", ""ql"": """", ""cf_view"": ""CF3"", ""seam"": ""02:ground-context"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/ai-kit#58(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
H2->A0,relation,suite-relations,H2,A0,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,02:ground-context,L,"{""src_product"": ""AIKit"", ""dst_product"": ""Central"", ""ql"": """", ""cf_view"": ""CF3"", ""seam"": ""02:ground-context"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/ai-kit#58(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
H3->H0,relation,suite-relations,H3,H0,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,03:ground-development,L,"{""src_product"": ""Factory"", ""dst_product"": ""Central"", ""ql"": """", ""cf_view"": ""CF4"", ""seam"": ""03:ground-development"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/agent-system-design#142(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
H3->A0,relation,suite-relations,H3,A0,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,03:ground-development,L,"{""src_product"": ""Factory"", ""dst_product"": ""Central"", ""ql"": """", ""cf_view"": ""CF4"", ""seam"": ""03:ground-development"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/agent-system-design#142(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
H4->H0,relation,suite-relations,H4,H0,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,04:ground-materialisation,W,"{""src_product"": ""Workcell"", ""dst_product"": ""Central"", ""ql"": """", ""cf_view"": ""CF5/CF6-field"", ""seam"": ""04:ground-materialisation"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/Workcell#18(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
H4->A0,relation,suite-relations,H4,A0,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,04:ground-materialisation,W,"{""src_product"": ""Workcell"", ""dst_product"": ""Central"", ""ql"": """", ""cf_view"": ""CF5/CF6-field"", ""seam"": ""04:ground-materialisation"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/Workcell#18(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
H5->H0,relation,suite-relations,H5,H0,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,05/50:ground-synthesis,H,"{""src_product"": ""QL"", ""dst_product"": ""Central"", ""ql"": ""B3|C1"", ""cf_view"": ""CF7"", ""seam"": ""05/50:ground-synthesis"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/QL-MEF#19(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
H5->A0,relation,suite-relations,H5,A0,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,05/50:ground-synthesis,H,"{""src_product"": ""QL"", ""dst_product"": ""Central"", ""ql"": ""D2-transform|D2-complete"", ""cf_view"": ""CF7"", ""seam"": ""05/50:ground-synthesis"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/QL-MEF#19(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
A0->H0,relation,suite-relations,A0,H0,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,conjugation:Central,H,"{""src_product"": ""Central"", ""dst_product"": ""Central"", ""ql"": ""D1.inverse"", ""cf_view"": ""CF1"", ""seam"": ""conjugation:Central"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
A0->H1,relation,suite-relations,A0,H1,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,01:grounded-agency,H,"{""src_product"": ""Central"", ""dst_product"": ""Actuation"", ""ql"": ""D2-require.inverse"", ""cf_view"": ""CF2"", ""seam"": ""01:grounded-agency"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/Actuation#4"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
A0->H2,relation,suite-relations,A0,H2,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,02:ground-context,L,"{""src_product"": ""Central"", ""dst_product"": ""AIKit"", ""ql"": """", ""cf_view"": ""CF3"", ""seam"": ""02:ground-context"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/ai-kit#58(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
A0->H3,relation,suite-relations,A0,H3,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,03:ground-development,L,"{""src_product"": ""Central"", ""dst_product"": ""Factory"", ""ql"": """", ""cf_view"": ""CF4"", ""seam"": ""03:ground-development"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/agent-system-design#142(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
A0->H4,relation,suite-relations,A0,H4,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,04:ground-materialisation,W,"{""src_product"": ""Central"", ""dst_product"": ""Workcell"", ""ql"": """", ""cf_view"": ""CF5/CF6-field"", ""seam"": ""04:ground-materialisation"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/Workcell#18(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
A0->H5,relation,suite-relations,A0,H5,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,05/50:ground-synthesis,H,"{""src_product"": ""Central"", ""dst_product"": ""QL"", ""ql"": ""D2-transform.inverse|D2-complete.inverse"", ""cf_view"": ""CF7"", ""seam"": ""05/50:ground-synthesis"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/QL-MEF#19(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
A0->A0,relation,suite-relations,A0,A0,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,self:Central,I,"{""src_product"": ""Central"", ""dst_product"": ""Central"", ""ql"": """", ""cf_view"": ""CF1"", ""seam"": ""self:Central"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
A0->A1,relation,suite-relations,A0,A1,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,01:grounded-agency,H,"{""src_product"": ""Central"", ""dst_product"": ""Actuation"", ""ql"": ""D3:A1"", ""cf_view"": ""CF2"", ""seam"": ""01:grounded-agency"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/Actuation#4"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
A0->A2,relation,suite-relations,A0,A2,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,02:ground-context,L,"{""src_product"": ""Central"", ""dst_product"": ""AIKit"", ""ql"": """", ""cf_view"": ""CF3"", ""seam"": ""02:ground-context"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/ai-kit#58(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
A0->A3,relation,suite-relations,A0,A3,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,03:ground-development,L,"{""src_product"": ""Central"", ""dst_product"": ""Factory"", ""ql"": """", ""cf_view"": ""CF4"", ""seam"": ""03:ground-development"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/agent-system-design#142(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
A0->A4,relation,suite-relations,A0,A4,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,04:ground-materialisation,W,"{""src_product"": ""Central"", ""dst_product"": ""Workcell"", ""ql"": """", ""cf_view"": ""CF5/CF6-field"", ""seam"": ""04:ground-materialisation"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/Workcell#18(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
A0->A5,relation,suite-relations,A0,A5,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,05/50:ground-synthesis,H,"{""src_product"": ""Central"", ""dst_product"": ""QL"", ""ql"": ""D3:B3|D3:C1"", ""cf_view"": ""CF7"", ""seam"": ""05/50:ground-synthesis"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/QL-MEF#19(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
A1->H0,relation,suite-relations,A1,H0,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,01:grounded-agency,H,"{""src_product"": ""Actuation"", ""dst_product"": ""Central"", ""ql"": ""D2-transform.inverse"", ""cf_view"": ""CF2"", ""seam"": ""01:grounded-agency"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/Actuation#4"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
A1->A0,relation,suite-relations,A1,A0,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,01:grounded-agency,H,"{""src_product"": ""Actuation"", ""dst_product"": ""Central"", ""ql"": ""D3:A1"", ""cf_view"": ""CF2"", ""seam"": ""01:grounded-agency"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/Actuation#4"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
A2->H0,relation,suite-relations,A2,H0,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,02:ground-context,L,"{""src_product"": ""AIKit"", ""dst_product"": ""Central"", ""ql"": """", ""cf_view"": ""CF3"", ""seam"": ""02:ground-context"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/ai-kit#58(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
A2->A0,relation,suite-relations,A2,A0,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,02:ground-context,L,"{""src_product"": ""AIKit"", ""dst_product"": ""Central"", ""ql"": """", ""cf_view"": ""CF3"", ""seam"": ""02:ground-context"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/ai-kit#58(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
A3->H0,relation,suite-relations,A3,H0,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,03:ground-development,L,"{""src_product"": ""Factory"", ""dst_product"": ""Central"", ""ql"": """", ""cf_view"": ""CF4"", ""seam"": ""03:ground-development"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/agent-system-design#142(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
A3->A0,relation,suite-relations,A3,A0,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,03:ground-development,L,"{""src_product"": ""Factory"", ""dst_product"": ""Central"", ""ql"": """", ""cf_view"": ""CF4"", ""seam"": ""03:ground-development"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/agent-system-design#142(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
A4->H0,relation,suite-relations,A4,H0,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,04:ground-materialisation,W,"{""src_product"": ""Workcell"", ""dst_product"": ""Central"", ""ql"": """", ""cf_view"": ""CF5/CF6-field"", ""seam"": ""04:ground-materialisation"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/Workcell#18(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
A4->A0,relation,suite-relations,A4,A0,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,04:ground-materialisation,W,"{""src_product"": ""Workcell"", ""dst_product"": ""Central"", ""ql"": """", ""cf_view"": ""CF5/CF6-field"", ""seam"": ""04:ground-materialisation"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/Workcell#18(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
A5->H0,relation,suite-relations,A5,H0,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,05/50:ground-synthesis,H,"{""src_product"": ""QL"", ""dst_product"": ""Central"", ""ql"": ""D2-require.inverse|D2-complete.inverse"", ""cf_view"": ""CF7"", ""seam"": ""05/50:ground-synthesis"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/QL-MEF#19(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
A5->A0,relation,suite-relations,A5,A0,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,05/50:ground-synthesis,H,"{""src_product"": ""QL"", ""dst_product"": ""Central"", ""ql"": ""D3:B3|D3:C1"", ""cf_view"": ""CF7"", ""seam"": ""05/50:ground-synthesis"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/QL-MEF#19(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
rel.central.q0.S,relation,product-field,q0,S,[],,,,,agent-inference,ProjectCentral/user/central.html#whole-why,,,central.html#whole/why,"Central aims to fill a gap in agentic work: bringing together the agentic World born in the Human–Agent relationship. An Agent encounters a World through the context it receives. The context of that context is a filesystem. Central gives that filesystem a structure, relating each project’s own Central to common Central ground. Instances of agency on a machine can then be understood as nodes in a field of activity, whose context files and code carry the inputs and outputs of agentic work.",,"{""basis"": ""Exact overview seed text. Capability links follow their existing governing expanded account units; cell placement is editorial inference."", ""seed_ref"": ""central:seed:q0"", ""source_unit"": ""whole-why"", ""seed_sha256"": ""ddc584de51434211c77c553fc7dd55ac64b18878ed644e42751caa6249c1e232"", ""source_standing"": ""authored-human-position"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",Why?
rel.central.q1.S,relation,product-field,q1,S,"[""cap.central.root"", ""cap.central.work-entry"", ""cap.central.control-reading"", ""cap.central.machine-declaration"", ""cap.central.machine-reconciliation"", ""cap.central.authored-ground"", ""cap.central.world-map"", ""cap.central.source-reading"", ""cap.central.governance"", ""cap.central.skills"", ""cap.central.personal-surface"", ""cap.central.files""]",,,,,agent-inference,ProjectCentral/user/central.html#whole-what,,,central.html#whole/what,"Central is a CLI that safely and minimally structures a filesystem. It anchors relationships between projects, creates spaces for human- and Agent-developed artifacts, and provides a recursive home for AI work across the system.",,"{""basis"": ""Exact overview seed text; capability links follow their governing account units, including the separately sourced native filesystem-reading contract; cell placement is editorial inference."", ""seed_ref"": ""central:seed:q1"", ""source_unit"": ""whole-what"", ""seed_sha256"": ""33d6b5c46c5c8abcbf22d5f477b4d101bc97ef868ba9af244b9f59e41b74c59d"", ""source_standing"": ""authored-human-position"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T16:44:00.506207+00:00""}",What?
rel.central.q2.S,relation,product-field,q2,S,"[""cap.central.machine-recovery"", ""cap.central.projectcentral-lifecycle"", ""cap.central.source-editing"", ""cap.central.now"", ""cap.central.day-rollover"", ""cap.central.flow"", ""cap.central.agent-profiles"", ""cap.central.control-proposals"", ""cap.central.guided-ground-authoring""]",,,,,agent-inference,ProjectCentral/user/central.html#whole-how,,,central.html#whole/how,"Central projects a filesystem structure, laying the infrastructure for Agent navigation and human clarity around work. The conditions that determine an Agent’s World live outside any particular harness session and remain available between its activities. Tending those conditions becomes a quiet starting point for developing one’s own relationship with AI Agents and their work.",,"{""basis"": ""Exact overview seed text. Capability links follow their existing governing expanded account units; cell placement is editorial inference."", ""seed_ref"": ""central:seed:q2"", ""source_unit"": ""whole-how"", ""seed_sha256"": ""527ba9b416d79fc3170742a6afd464800f3728613f326f0640f0cca90b3e4a03"", ""source_standing"": ""authored-human-position"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",How?
rel.central.q3.S,relation,product-field,q3,S,"[""cap.central.action-discovery"", ""cap.central.reproject"", ""cap.central.source-change"", ""cap.central.source-history"", ""cap.central.notification""]",,,,,agent-inference,ProjectCentral/user/central.html#whole-whereby,,,central.html#whole/whereby,"For human users, Central is the means by which a simple foundation for organised work and a deliberate User–Agent relationship can be established. It also makes an O:I World—an Objective Internality constituting both Agent and user—available for inspection, sharing and research.",,"{""basis"": ""Exact overview seed text. Capability links follow their existing governing expanded account units; cell placement is editorial inference."", ""seed_ref"": ""central:seed:q3"", ""source_unit"": ""whole-whereby"", ""seed_sha256"": ""197ad399a15b4bf13300c58ec1f708042425b9081b105d900eb6b44f5a2b1ca5"", ""source_standing"": ""authored-human-position"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",Who / Whereby?
rel.central.q4.S,relation,product-field,q4,S,[],,,,,agent-inference,ProjectCentral/user/central.html#whole-context,,,central.html#whole/context,"Central is consistent and quiet: the relatively immutable structured ground that gives space and form to the relationships involved in agentic work, between Humans, Agents, Machines and Work.",,"{""basis"": ""Exact overview seed text. Capability links follow their existing governing expanded account units; cell placement is editorial inference."", ""seed_ref"": ""central:seed:q4"", ""source_unit"": ""whole-context"", ""seed_sha256"": ""7e3d0a0497328789aba959820948dfbcbfc2bdf91ce2173e4b95024a57ad7d6e"", ""source_standing"": ""authored-human-position"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",Where / When?
rel.central.q5.S,relation,product-field,q5,S,[],,,,,agent-inference,ProjectCentral/user/central.html#whole-purpose,,,central.html#whole/purpose,"Central provides the foundational bootstrap for the O:I paradigm. It is intended as the first install and the only necessary package for beginning to work in that paradigm. A person can keep their existing setup while Central gives it a more integrated, relationally aware structure. This foundation supports the other O:I packages: it gives AIKit’s wiki functions a source home and receives projections from Workcell machine setups. Its first concern is the initial disclosure of a person’s World to an Agent: making the context being offered clear and worth tending. Central gives durable principles, governance and user context a common home from which they can inform projects at the depth and scope the user chooses. The user determines how that material propagates and lives. Central gives the ratified basis a home.",,"{""basis"": ""Exact overview seed text. Capability links follow their existing governing expanded account units; cell placement is editorial inference."", ""seed_ref"": ""central:seed:q5"", ""source_unit"": ""whole-purpose"", ""seed_sha256"": ""7fc462f4d49368faf273584a6e94c4ec4e7c22535f7d04be59c4439e47d650d1"", ""source_standing"": ""authored-human-position"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",Why-For?
rel.central.q5.S2,relation,product-field,q5,S2,"[""cap.central.authored-ground"", ""cap.central.source-reading""]",,,,,agent-inference,ProjectCentral/user/central.html#q5-coherence,,,central.html#q5/coherence,AIKit’s wiki functions can develop and disclose knowledge relative to its sources.,,"{""basis"": ""Account sentence, with source capability links added as editorial inference."", ""source_unit"": ""q5-coherence"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
rel.central.q4.S4,relation,product-field,q4,S4,[],,,,,agent-inference,ProjectCentral/user/central.html#q4-contexts,,,central.html#q4/contexts,Central preserves the declared source and native operations; Workcell and the other execution owners determine how work becomes operative on a machine.,,"{""basis"": ""Exact account sentence; cell placement is editorial inference."", ""source_unit"": ""q4-contexts"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:23.679879+00:00""}",
cap.central.files,capability,,,,[],A person or agent needs to inspect ordinary Central-root material without adopting it or inventing a semantic source identity.,central.files.list returns bounded directory entries and owner locations; central.files.read returns a validated returned location as bounded UTF-8 text.,Eligible regular files and directories can be inspected with exact locations and content revisions while retrieval exclusions and redirected paths refuse access.,Implemented source contract; the gated release executable listed the real Central root in this documentation pass. Linked source tests were not freshly executed.,observed-evidence,docs/NATIVE-FILESYSTEM-READING.md,ctrl/src/files.rs,ctrl/src/files.rs;ctrl/tests/foundation.rs,central.html#q1/entry,,,"{""basis"": ""Native filesystem-reading contract and real gated release execution against the configured Central root."", ""cli_commands"": [""central.files.history"", ""central.files.list"", ""central.files.read"", ""central.files.recovery_preview"", ""central.files.restore"", ""central.files.write""], ""cli_exposure"": {""kind"": ""direct"", ""reason"": ""Both read-only actions are discoverable from Central's native Action registry and executable through ctrl action run.""}, ""maintenance"": {""updated_at"": ""2026-09-08"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""reconcile:central-pr-132""], ""code_basis"": {""ctrl/src/files.rs"": ""4ad2f41d8e8fd2b1b91f3feb1c271bfff51b110a86c96f9b9e66569a7021fb11""}}, ""last_reconciled_at"": ""2026-09-08T11:53:39.350344+00:00""}",
cap.central.pasu-identity,capability,,,,[],A person needs their human identity grounded as a versioned bounded-entity subject the whole toolchain can name without a second registry.,central.pasu/v1 parses and validates nara/agent/agent-set refs; the nara identity-source manifest anchors the authored Control/user/identity folder; ground relations carry the subject ref; central.world exposes identity state with per-file content revisions.,"Identity ownership stays in authored Central ground while Factory, Actuation and AIKit reference one stable subject; identity-source edits are observable as content-revision changes.","source-inspected; exercised by unit, integration and live central.world runs during the authoring session.",agent-inference,../github-recovery-mirror/mirror/Central/issues/108.json;../github-recovery-mirror/mirror/Central/issues/117.json,ctrl/src/pasu.rs;ctrl/src/lib.rs,ctrl/tests/world_map.rs,central.html#q1/entry,,,"{""basis"": ""Owner-directed identity carrier (W10 V1); original verification limits retained."", ""cli_commands"": [], ""cli_exposure"": {""kind"": ""library"", ""reason"": ""The grammar and identity read-model surface through central.world and the ground-relations read path; the capability owns no CLI command of its own.""}, ""maintenance"": {""updated_at"": ""2026-09-09"", ""change_refs"": [""EpiLogos/Central#126"", ""zcode-session-2026-09-07"", ""EpiLogos/Central#126 (W10 V1: central.pasu/v1 identity grammar; world-map/source-change/agent-profile carriers moved — code bases re-reviewed at this revision)"", ""EpiLogos/Central#127 (W10 V2: agent-set/world persistence Actions + attribution seam; cli/now/agent-profile carriers moved — code bases re-reviewed at this revision)"", ""reconcile:central-pr-132"", ""cradle:Central:w2-w4:PR-133""], ""code_basis"": {""ctrl/src/pasu.rs"": ""7e28f75ab14d6404d638a5c1a8bebacae3115a9660638dc7c0ab883fc5b3f606"", ""ctrl/src/lib.rs"": ""4f13b0f4a6102a22a82bf7369c795329e11b46dc39cc3a6b196ab0aeae18d564""}}, ""last_reconciled_at"": ""2026-09-09T15:11:26.113026+00:00""}",
cap.central.bounded-relations,capability,,,,[],A person needs durable authored AgentSets and world relations so composed participation and source propagation survive sessions and stay revisable.,central.agent-set.* and central.world-relations.* persist and resolve central.agent-set/v1 and central.world-relations/v1 records under compare-and-swap revisions; central.agent-set.resolve partitions authored membership against declared availability and rejects cycles; central.world.effective-sources resolves ancestry propagation with per-hop provenance.,Composed agent participation and world source propagation become real operations on real files; authored records never rewrite on availability changes and effective relations stay attributable per hop.,"source-inspected; exercised by unit, integration and live Action runs during the authoring session.",agent-inference,../github-recovery-mirror/mirror/Central/issues/108.json,ctrl/src/agent_set_store.rs;ctrl/src/agent_set_actions.rs;ctrl/src/lib.rs,ctrl/tests/foundation.rs,central.html#q3/interaction,,,"{""basis"": ""Owner-directed persistence slice (W10 V2); the domain model's owned Actions."", ""cli_commands"": [""central.agent-set.save"", ""central.agent-set.list"", ""central.agent-set.read"", ""central.agent-set.remove"", ""central.agent-set.resolve"", ""central.world-relations.save"", ""central.world-relations.list"", ""central.world-relations.read"", ""central.world-relations.remove"", ""central.world.effective-sources""], ""cli_exposure"": {""kind"": ""direct"", ""reason"": ""The listed native Action identities are invocable through ctrl action run and discoverable through ctrl action list.""}, ""maintenance"": {""updated_at"": ""2026-09-09"", ""change_refs"": [""EpiLogos/Central#127"", ""zcode-session-2026-09-07"", ""EpiLogos/Central#127 (W10 V2: agent-set/world persistence Actions + attribution seam; cli/now/agent-profile carriers moved — code bases re-reviewed at this revision)"", ""reconcile:central-pr-132"", ""cradle:Central:w2-w4:PR-133""], ""code_basis"": {""ctrl/src/agent_set_store.rs"": ""2cc946c24b4407f820f59cd78b57cbf3751d7e95b863e97a154b4b0be37723c0"", ""ctrl/src/agent_set_actions.rs"": ""26d978c4964224d64c4dd5acc08395a72e2087b3a1dc4e345841b21b5138b307"", ""ctrl/src/lib.rs"": ""4f13b0f4a6102a22a82bf7369c795329e11b46dc39cc3a6b196ab0aeae18d564""}}, ""last_reconciled_at"": ""2026-09-09T15:11:26.113026+00:00""}",
cap.central.wiki-reading,capability,,,,[],"A person or agent needs to read wiki content as typed, versioned source with revision identity and provenance, not as raw text.",central.wiki.read resolves one wiki node address and returns its typed reading; projectcentral.wiki.read scopes the same seam to a project wiki.,"The reading carries exact resource ref, revision and provenance; display or unread references are explicit states and can never record use.",source-inspected; native suite verified by the orchestrator on the landing tree (cargo test -p ctrl 320 passed).,agent-inference,../O-I/docs/OI-DESKTOP-CRADLE-REBUILD-WAYFINDER.md,ctrl/src/wiki_read.rs,ctrl/tests/foundation.rs,central.html#q1/ground,,,"{""basis"": ""Cradle wave landing evidence; reconciled through the product-ground machinery."", ""cli_commands"": [""central.wiki.read"", ""projectcentral.wiki.read""], ""cli_exposure"": {""kind"": ""direct"", ""reason"": ""The listed native Action identities are invocable through ctrl action run and discoverable through ctrl action list.""}, ""maintenance"": {""updated_at"": ""2026-09-09"", ""change_refs"": [""cradle:Central:w2-w4:PR-133""], ""code_basis"": {""ctrl/src/wiki_read.rs"": ""dfb9f0aceaeec363eb624aab7b729f7c558e25f380be01294b4939f551090398""}}, ""last_reconciled_at"": ""2026-09-09T15:11:26.113026+00:00""}",
cap.central.remembered-note,capability,,,,[],"A person needs to remember one selection into a durable destination as a proposal until recognised, so a thought survives the session that produced it.",central.remember and projectcentral.remember write one selection verbatim as a content-addressed remembered note stamped generated-proposal/unrecognised.,"The note lands under Control/agents/remembered with provenance (verbatim selection, source ref, origin action); recognition stays a separate human-owned act with no machine path.",source-inspected; native suite verified by the orchestrator on the landing tree (cargo test -p ctrl 320 passed).,agent-inference,../O-I/docs/OI-DESKTOP-CRADLE-REBUILD-WAYFINDER.md,ctrl/src/remember.rs;ctrl/src/remember_actions.rs;ctrl/src/remember_store.rs,ctrl/tests/foundation.rs,central.html#q2/resume,,,"{""basis"": ""Cradle wave landing evidence; reconciled through the product-ground machinery."", ""cli_commands"": [""central.remember"", ""projectcentral.remember""], ""cli_exposure"": {""kind"": ""direct"", ""reason"": ""The listed native Action identities are invocable through ctrl action run and discoverable through ctrl action list.""}, ""maintenance"": {""updated_at"": ""2026-09-09"", ""change_refs"": [""cradle:Central:w2-w4:PR-133""], ""code_basis"": {""ctrl/src/remember.rs"": ""8888fb67aeb3d8175cf8305fd66af86d03ad4d0b169e27164f634a2dccda3907"", ""ctrl/src/remember_actions.rs"": ""2c764241e95baa3aa5176dc8c4ae23783c74c2b89906626df235014f2da19b4b"", ""ctrl/src/remember_store.rs"": ""fe570a67c36c4101f8c5fb35072bd48ed38301c9e7e6cde5b974e5a8611d546f""}}, ""last_reconciled_at"": ""2026-09-09T15:11:26.113026+00:00""}",
```
