# Engineering-ground seed implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Engineering-norm statements in Control are distilled by ctrl into a `generated-derived` foundational prompt source, adoptable into project worlds, and named (not copied) by the ai-kit managed bootstrap.

**Architecture:** Four small pieces on existing machinery: (1) Control ground relations so Control sources carry machine-checked provenance like Project sources; (2) the statement content itself, seeded as `generated-suggestion` drafts awaiting human authorship; (3) a ctrl render action producing the derived foundational prompt with pinned statement provenance refs; (4) one provenance line in the ai-kit managed bootstrap render. No new ai-kit types; the seed rides existing source/horizon/binding/ContextResolution machinery.

**Tech Stack:** Rust (ctrl = `Central/ctrl`, ai-kit = `ai-kit/crates`), plain markdown (Control content), serde_json relations files.

**Spec:** `ai-kit/docs/superpowers/specs/2026-09-04-engineering-ground-seed-design.md` (committed on ai-kit main, 71a5962)

## Global Constraints

- Agent writes to human-authored ground are refused (propose instead) — never weaken `enforce_write_authority` in `ctrl/src/world_source.rs`.
- The distillate is `generated-derived`, never `human-authored`; human **adoption** promotes it. Edits happen at the statement layer.
- Bootstrap disclosure names the source, never copies payload (smallest-sufficient).
- Content revisions use the in-tree versioned FNV-1a (`content_revision` in `ctrl/src/source_horizon.rs`) — no new hash dependencies.
- Statement form: QL hexad (why / what / how / who-whereby / where-when / why-for), first person, spoken to the agent, movements unlabeled, natural prose. Template: `Control/agents/expressions/central-intent/templates/statement-form.md`.
- Statement files: one self-covering object per file, minimal but catching all angles.
- Ground relation schemas: `central.control.ground-relations/v1` at `Control/relations/source-relations.json`, world id `control:root`.
- TDD: failing test first, run to see it fail, minimal implementation, run to green, commit. One commit per task.
- Branches: Central work on `feat/engineering-ground-seed` branched from Central `main`; ai-kit work on `feat/engineering-ground-seed` branched from ai-kit `main`.

---

### Task 1: Control ground relations in the source horizon

Today `control_source_bindings` (`ctrl/src/source_horizon.rs:451`) assigns tree-based bindings with provenance `"unresolved"` and fixed roles; only Project sources read a ground-relations file. Control needs the same machine-checked override so the engineering statements can be declared `human-authored` and the distillate `generated-derived`.

**Files:**
- Modify: `Central/ctrl/src/source_horizon.rs` (consts ~line 26, `read_ground_relations` ~314, `control_source_bindings` ~451)
- Test: `Central/ctrl/tests/control_source.rs` (extend; mirror fixture style of `tests/source_change_horizon.rs:16-48`)

**Interfaces:**
- Consumes: `GroundRelationsFile` / `GroundRelation` structs (existing, lines 144-162), `safe_regular_file`, `retrieval_allowed`, `insert_tree_bindings` (existing private fns).
- Produces:
  - `pub const CONTROL_GROUND_RELATIONS_SOURCE: &str = "Control/relations/source-relations.json";`
  - `pub const CONTROL_GROUND_RELATIONS_SCHEMA: &str = "central.control.ground-relations/v1";`
  - `pub const CONTROL_WORLD_REF: &str = "control:root";`
  - `control_source_bindings` now overrides tree bindings from `Control/relations/source-relations.json` when present (same retain+insert semantics as the project flow at lines 373-394).

- [ ] **Step 1: Write the failing test**

In `Central/ctrl/tests/control_source.rs`, add (fixture helpers copied from `tests/source_change_horizon.rs:16-48` — `TempRoot`, `NEXT_TEMP_ROOT`):

```rust
use central_ctrl::{
    control_source_bindings, CONTROL_GROUND_RELATIONS_SCHEMA, CONTROL_GROUND_RELATIONS_SOURCE,
};

#[test]
fn control_ground_relations_override_tree_provenance() {
    let temp = TempRoot::new();
    let central = temp.path();
    let statement = central.join("Control/agents/governance/engineering/agent-operations.md");
    fs::create_dir_all(statement.parent().unwrap()).unwrap();
    fs::write(&statement, "You branch small and commit honestly.\n").unwrap();

    // Tree default before relations exist.
    let bindings = control_source_bindings(central).unwrap();
    let tree_binding = bindings
        .iter()
        .find(|binding| binding.path == "Control/agents/governance/engineering/agent-operations.md")
        .expect("statement appears in control bindings");
    assert_eq!(tree_binding.provenance, "unresolved");
    assert!(tree_binding
        .roles
        .iter()
        .any(|role| role == "agent-governance-source"));

    // Relations file promotes it to recognised human ground.
    let relations = central.join(CONTROL_GROUND_RELATIONS_SOURCE);
    fs::create_dir_all(relations.parent().unwrap()).unwrap();
    fs::write(
        &relations,
        serde_json::to_string_pretty(&json!({
            "schema": CONTROL_GROUND_RELATIONS_SCHEMA,
            "project_id": "control:root",
            "relations": [{
                "ref": "central:source:control:root:Control/agents/governance/engineering/agent-operations.md",
                "path": "Control/agents/governance/engineering/agent-operations.md",
                "provenance": "human-authored",
                "standing": "durable-source",
                "roles": ["agent-governance-source"],
                "treatment": "control-governance-retained-in-place"
            }]
        }))
        .unwrap(),
    )
    .unwrap();

    let bindings = control_source_bindings(central).unwrap();
    let relation_binding = bindings
        .iter()
        .find(|binding| binding.path == "Control/agents/governance/engineering/agent-operations.md")
        .expect("statement still appears");
    assert_eq!(relation_binding.provenance, "human-authored");
    assert_eq!(
        relation_binding.source_ref,
        "central:source:control:root:Control/agents/governance/engineering/agent-operations.md"
    );
    // One physical source, one logical binding: the tree fallback is replaced, not duplicated.
    assert_eq!(
        bindings
            .iter()
            .filter(|binding| binding.path == "Control/agents/governance/engineering/agent-operations.md")
            .count(),
        1
    );
}

#[test]
fn control_ground_relations_missing_file_keeps_tree_defaults() {
    let temp = TempRoot::new();
    let central = temp.path();
    let statement = central.join("Control/agents/governance/engineering/coding-approach.md");
    fs::create_dir_all(statement.parent().unwrap()).unwrap();
    fs::write(&statement, "You change the smallest thing that can work.\n").unwrap();

    let bindings = control_source_bindings(central).unwrap();
    let binding = bindings
        .iter()
        .find(|binding| binding.path.ends_with("coding-approach.md"))
        .expect("statement appears");
    assert_eq!(binding.provenance, "unresolved");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd Central && cargo test -p central-ctrl --test control_source control_ground`
Expected: FAIL — `CONTROL_GROUND_RELATIONS_SOURCE` / `CONTROL_GROUND_RELATIONS_SCHEMA` not exported from `central_ctrl` (compile error).

- [ ] **Step 3: Write minimal implementation**

In `Central/ctrl/src/source_horizon.rs`:

Add consts next to the existing ground-relations consts (lines 26-27):

```rust
pub const CONTROL_GROUND_RELATIONS_SOURCE: &str = "Control/relations/source-relations.json";
pub const CONTROL_GROUND_RELATIONS_SCHEMA: &str = "central.control.ground-relations/v1";
pub const CONTROL_WORLD_REF: &str = "control:root";
```

Generalise the reader. Replace `read_ground_relations` (lines 314-331) with a shared helper and two thin callers:

```rust
fn read_relations_file(path: &Path, schema: &str, expected_id: &str) -> io::Result<Vec<GroundRelation>> {
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let relations: GroundRelationsFile = serde_json::from_slice(&fs::read(path)?)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if relations.schema != schema || relations.project_id != expected_id {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "ground relations have an unsupported schema or world id",
        ));
    }
    Ok(relations.relations)
}

fn read_ground_relations(project_root: &Path, expected_project_id: &str) -> io::Result<Vec<GroundRelation>> {
    read_relations_file(
        &project_root.join(GROUND_RELATIONS_SOURCE),
        GROUND_RELATIONS_SCHEMA,
        expected_project_id,
    )
}

fn read_control_ground_relations(central_root: &Path) -> io::Result<Vec<GroundRelation>> {
    read_relations_file(
        &central_root.join(CONTROL_GROUND_RELATIONS_SOURCE),
        CONTROL_GROUND_RELATIONS_SCHEMA,
        CONTROL_WORLD_REF,
    )
}
```

Note: `read_ground_relations` currently errors when the file is missing only if `fs::read` fails on a present-but-unreadable file; check current behaviour at lines 314-321 and preserve the project contract exactly (missing file → empty relations) via the `!path.is_file()` guard above — if the project tests pin different missing-file behaviour, keep `read_ground_relations` byte-identical and only share the parse/validate tail. Run `cargo test -p central-ctrl --test source_change_horizon --test projectcentral_ground_real` after implementing to confirm no regression.

In `control_source_bindings` (line 451): replace the literal `let world_ref = "control:root";` with `let world_ref = CONTROL_WORLD_REF;` and append the relations override before `Ok(bindings.into_values().collect())` (line 484):

```rust
    for relation in read_control_ground_relations(central_root)? {
        let relative = relation.path.clone();
        let path = central_root.join(&relative);
        if !safe_regular_file(central_root, &path)? {
            continue;
        }
        // Same law as the project flow: an explicit recognised relation is the
        // identity/standing authority for its path; the tree fallback is replaced.
        bindings.retain(|_, binding| binding.path != relative);
        bindings.insert(
            relation.source_ref.clone(),
            SourceBinding {
                source_ref: relation.source_ref,
                path: relative,
                roles: relation.roles,
                provenance: relation.provenance,
                standing: relation.standing,
                treatment: relation.treatment,
                agent_retrieval_allowed: retrieval_allowed(central_root, &path),
            },
        );
    }
```

Export the two new consts from `Central/ctrl/src/lib.rs` in the existing re-export list near `GROUND_RELATIONS_SOURCE`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd Central && cargo test -p central-ctrl --test control_source --test source_change_horizon --test projectcentral_ground_real --test world_source_actions`
Expected: all PASS (new tests + no regression in project relations/horizon/source-write suites).

- [ ] **Step 5: Commit**

```bash
git add ctrl/src/source_horizon.rs ctrl/src/lib.rs ctrl/tests/control_source.rs
git commit -m "feat(ctrl): control ground relations override tree bindings

Control sources now read Control/relations/source-relations.json
(central.control.ground-relations/v1, world control:root) with the same
retain+insert override law as project ground relations, so provenance and
standing are machine-checked rather than tree-inferred."
```

---

### Task 2: Engineering statements seed (Control content)

The authored layer: folder, README, four statement stubs in hexad form, and ground relations declaring them `generated-suggestion` drafts until the person authors/adopts them (drafts must not present as human ground — the write gate and the provenance law both depend on that honesty).

**Files:**
- Create: `/Users/admin/Central/Control/agents/governance/engineering/README.md`
- Create: `/Users/admin/Central/Control/agents/governance/engineering/agent-operations.md`
- Create: `/Users/admin/Central/Control/agents/governance/engineering/coding-approach.md`
- Create: `/Users/admin/Central/Control/agents/governance/engineering/verification.md`
- Create: `/Users/admin/Central/Control/agents/governance/engineering/base-skillset.md`
- Create: `/Users/admin/Central/Control/relations/source-relations.json`

**Interfaces:**
- Produces: the four statement paths consumed by Task 3's render (fixed set, this order: `agent-operations.md`, `coding-approach.md`, `verification.md`, `base-skillset.md`); the relations file consumed by Task 1's reader.

- [ ] **Step 1: Create the relations file**

`/Users/admin/Central/Control/relations/source-relations.json`:

```json
{
  "schema": "central.control.ground-relations/v1",
  "project_id": "control:root",
  "relations": [
    {
      "ref": "central:source:control:root:Control/agents/governance/engineering/agent-operations.md",
      "path": "Control/agents/governance/engineering/agent-operations.md",
      "provenance": "generated-suggestion",
      "standing": "draft-source",
      "roles": ["agent-governance-source"],
      "treatment": "control-governance-retained-in-place"
    },
    {
      "ref": "central:source:control:root:Control/agents/governance/engineering/coding-approach.md",
      "path": "Control/agents/governance/engineering/coding-approach.md",
      "provenance": "generated-suggestion",
      "standing": "draft-source",
      "roles": ["agent-governance-source"],
      "treatment": "control-governance-retained-in-place"
    },
    {
      "ref": "central:source:control:root:Control/agents/governance/engineering/verification.md",
      "path": "Control/agents/governance/engineering/verification.md",
      "provenance": "generated-suggestion",
      "standing": "draft-source",
      "roles": ["agent-governance-source"],
      "treatment": "control-governance-retained-in-place"
    },
    {
      "ref": "central:source:control:root:Control/agents/governance/engineering/base-skillset.md",
      "path": "Control/agents/governance/engineering/base-skillset.md",
      "provenance": "generated-suggestion",
      "standing": "draft-source",
      "roles": ["agent-governance-source"],
      "treatment": "control-governance-retained-in-place"
    }
  ]
}
```

- [ ] **Step 2: Create the README**

`/Users/admin/Central/Control/agents/governance/engineering/README.md`:

```markdown
# Control/agents/governance/engineering/

**Status:** seeded 2026-09-04 as generated-suggestion drafts; not yet authored ground
**Form:** `Control/agents/expressions/central-intent/templates/statement-form.md` (QL hexad, unlabeled)
**Keeper:** central-intent expression

## What this is

How the person wants engineering work done on their behalf: agent operations
(git norms, session behaviour), coding approach, verification expectations, and
the liked base skillset. One self-covering statement per file, first person,
spoken to the agent.

## Provenance law

- These files are `generated-suggestion` drafts until the person authors or
  adopts them. A draft is not human ground; do not treat it as a preference.
- Authorship happens in the person's own words, in place, per statement. On
  adoption, the relation in `Control/relations/source-relations.json` flips to
  `provenance: human-authored`, `standing: durable-source`.
- Agents propose changes (target, reason, supporting context, final diff);
  they do not write here. Recognition/adoption closes the transition.
- `foundational-prompt.md` in this folder is derived by
  `control.engineering-ground.render`; edit the statements, never the distillate.
```

- [ ] **Step 3: Create the four statement stubs**

Each stub is deliberately a *draft sketch*, not a voice-authored statement — it exists so the form, length, and register are visible, and so the render has real input. Model register on `Control/agents/governance/communicate-and-engage/smallest-sufficient.md` (short paragraphs, "You…" address) but do not copy its content. Every stub carries a first-line draft marker.

`agent-operations.md`:

```markdown
<!-- draft: generated-suggestion, not authored ground. The person authors this
     in their own voice; the hexad movements are present but unlabeled. -->

# Agent operations

Why agents act for me at all, and what that relation is for.

You work on my repositories as I would want to be found to have worked: branches
small and honest, commits that say what changed and why, no rewrite of shared
history. [The person replaces this sketch with their own account: how sessions
behave, what may run unattended, what always asks first.]

How you operate day to day, and by what means you are recognised as mine.

Where and when this holds — every session, including small work — and what it
is kept for: so increasing artificial agency returns more room for human agency.
```

`coding-approach.md`, `verification.md`, `base-skillset.md`: same skeleton, replacing the middle paragraph per subject — coding-approach (smallest change that can work, shape over volume, return-difference discipline), verification (what "done" means before return: tests, review posture, evidence expected), base-skillset (the liked global skills, prose plus skill refs; the setup conversation's curated selection lands here).

- [ ] **Step 4: Verify bindings pick the drafts up**

Run: `cd Central && cargo test -p central-ctrl --test control_source`
Expected: PASS (Task 1 machinery reads the new relations file; no test asserts on the live root, this is a manual smoke check via a one-off `cargo run` or existing `ctrl control.*` listing if available — otherwise skip; the machine check is Task 3's test).

- [ ] **Step 5: Commit**

Control content lives outside git at the Central root; no commit for this task. If your Central root is mirrored into a repo, commit there with message `feat(control): seed engineering governance statements as generated-suggestion drafts`.

---

### Task 3: ctrl engineering-ground render action

The distillate: a deterministic render of the four statements into `foundational-prompt.md`, with a provenance header pinning each statement's source ref and content revision, CAS write (unchanged → no write), and two registered actions following the template-stamp pattern.

**Files:**
- Create: `Central/ctrl/src/engineering_ground.rs`
- Modify: `Central/ctrl/src/lib.rs` (module + re-exports)
- Modify: `Central/ctrl/src/cli.rs:384` (register actions next to template stamp)
- Test: `Central/ctrl/tests/engineering_ground.rs`

**Interfaces:**
- Consumes: `content_revision`, `SourceRevision`, `source_ref` (make `fn source_ref` at `source_horizon.rs:202` `pub(crate)`), `CONTROL_WORLD_REF`, relations via the statement files directly (render reads files; provenance refs are computed, not read from the relations file).
- Produces:
  - `pub const ENGINEERING_GROUND_DIR: &str = "Control/agents/governance/engineering";`
  - `pub const ENGINEERING_GROUND_STATEMENTS: [&str; 4] = ["agent-operations.md", "coding-approach.md", "verification.md", "base-skillset.md"];`
  - `pub const ENGINEERING_GROUND_OUTPUT: &str = "Control/agents/governance/engineering/foundational-prompt.md";`
  - `pub fn render_engineering_ground(central_root: &Path) -> io::Result<String>`
  - `pub struct EngineeringGroundRender { pub output_path: String, pub changed: bool, pub revision: SourceRevision }`
  - `pub fn write_engineering_ground(central_root: &Path) -> io::Result<EngineeringGroundRender>`
  - Actions `control.engineering-ground.plan` (ReadOnly) and `control.engineering-ground.render` (LocallyMutating), registered via `register_engineering_ground_actions(&mut ActionRegistry)`.

- [ ] **Step 1: Write the failing test**

`Central/ctrl/tests/engineering_ground.rs` (fixture helpers as in Task 1):

```rust
use central_ctrl::{
    content_revision, render_engineering_ground, write_engineering_ground,
    ENGINEERING_GROUND_OUTPUT,
};

fn central_with_statements(name: &str) -> TempRoot {
    let temp = TempRoot::new();
    let dir = temp.path().join("Control/agents/governance/engineering");
    fs::create_dir_all(&dir).unwrap();
    for (file, body) in [
        ("agent-operations.md", "You branch small and commit honestly.\n"),
        ("coding-approach.md", "You change the smallest thing that can work.\n"),
        ("verification.md", "You return with evidence of done.\n"),
        ("base-skillset.md", "You reach for the essentials first.\n"),
    ] {
        fs::write(dir.join(file), body).unwrap();
    }
    temp
}

#[test]
fn render_emits_derived_prompt_with_pinned_provenance() {
    let temp = central_with_statements("render");
    let rendered = render_engineering_ground(temp.path()).unwrap();

    assert!(rendered.contains("# Foundational engineering prompt"));
    assert!(rendered.contains("provenance: generated-derived"));
    assert!(rendered.contains(
        "central:source:control:root:Control/agents/governance/engineering/agent-operations.md"
    ));
    // Revision is pinned inline: content-fnv1a64 with the statement's hash.
    let statement = temp.path().join("Control/agents/governance/engineering/agent-operations.md");
    let revision = content_revision(&statement).unwrap();
    assert!(rendered.contains(&revision.revision));
    // Statement bodies are carried verbatim.
    assert!(rendered.contains("You branch small and commit honestly."));
    // The distillate is not human ground and says so.
    assert!(rendered.contains("Do not edit this file"));
    // Render is deterministic.
    assert_eq!(rendered, render_engineering_ground(temp.path()).unwrap());
}

#[test]
fn write_is_cas_stable_and_detects_statement_change() {
    let temp = central_with_statements("write");
    let first = write_engineering_ground(temp.path()).unwrap();
    assert!(first.changed);
    assert_eq!(first.output_path, ENGINEERING_GROUND_OUTPUT);

    let second = write_engineering_ground(temp.path()).unwrap();
    assert!(!second.changed);
    assert_eq!(first.revision, second.revision);

    fs::write(
        temp.path().join("Control/agents/governance/engineering/verification.md"),
        "You return with evidence of done, and say what was not done.\n",
    )
    .unwrap();
    let third = write_engineering_ground(temp.path()).unwrap();
    assert!(third.changed);
    assert_ne!(first.revision, third.revision);
}

#[test]
fn adopted_distillate_participates_as_project_source() {
    let temp = central_with_statements("adopt");
    write_engineering_ground(temp.path()).unwrap();

    let central = temp.path();
    let project = central.join("Work/example-adopt");
    fs::create_dir_all(&project).unwrap();
    initialize_projectcentral(central, &project, "example/adopt").unwrap();

    // Human adoption act: copy the distillate into the project human-source
    // aperture and declare the ground relation.
    let manifest_human_source = "ProjectCentral/user"; // initialise_projectcentral's aperture
    let adopted = project.join(manifest_human_source).join("foundational-prompt.md");
    fs::copy(
        central.join(ENGINEERING_GROUND_OUTPUT),
        &adopted,
    )
    .unwrap();
    fs::write(
        project.join(GROUND_RELATIONS_SOURCE),
        serde_json::to_string_pretty(&json!({
            "schema": "central.project.ground-relations/v1",
            "project_id": "example/adopt",
            "relations": [{
                "ref": "central:source:project:example/adopt:ProjectCentral/user/foundational-prompt.md",
                "path": "ProjectCentral/user/foundational-prompt.md",
                "provenance": "human-adopted",
                "standing": "durable-source",
                "roles": ["project-human-source-aperture"],
                "treatment": "retain-native-in-place"
            }]
        }))
        .unwrap(),
    )
    .unwrap();

    let bindings = project_source_bindings(&project).unwrap();
    let binding = bindings
        .iter()
        .find(|binding| binding.path == "ProjectCentral/user/foundational-prompt.md")
        .expect("adopted distillate participates as a project source");
    assert_eq!(binding.provenance, "human-adopted");
    assert!(binding.agent_retrieval_allowed);
}
```

If `initialize_projectcentral` names the human-source aperture differently, read the manifest it writes (`ProjectCentral/manifest.json`) and use its `human_source` value instead of the hardcoded `ProjectCentral/user` — assert against the manifest, never a guess.

- [ ] **Step 2: Run test to verify it fails**

Run: `cd Central && cargo test -p central-ctrl --test engineering_ground`
Expected: FAIL — `render_engineering_ground` / `write_engineering_ground` / consts not found in `central_ctrl` (compile error).

- [ ] **Step 3: Write minimal implementation**

`Central/ctrl/src/engineering_ground.rs`:

```rust
//! Engineering-ground distillation.
//!
//! Renders the authored engineering governance statements into one derived
//! foundational prompt source. The output is `generated-derived`: it is
//! machine-writable, re-derivable, and carries pinned provenance refs to the
//! statements it was rendered from. Human adoption promotes it; editing happens
//! at the statement layer, never on the distillate.

use crate::action::{
    ActionAvailability, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionOutputDefinition, ActionRegistry, MutationClass,
};
use crate::result::{ActionResult, ResultStatus};
use crate::root::resolve_central_root;
use crate::source_horizon::{
    content_revision, source_ref, SourceRevision, CONTROL_WORLD_REF,
};
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::io;
use std::path::Path;

pub const ENGINEERING_GROUND_DIR: &str = "Control/agents/governance/engineering";
pub const ENGINEERING_GROUND_STATEMENTS: [&str; 4] = [
    "agent-operations.md",
    "coding-approach.md",
    "verification.md",
    "base-skillset.md",
];
pub const ENGINEERING_GROUND_OUTPUT: &str =
    "Control/agents/governance/engineering/foundational-prompt.md";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EngineeringGroundRender {
    pub output_path: String,
    pub changed: bool,
    pub revision: SourceRevision,
}

fn statement_section(central_root: &Path, file: &str) -> io::Result<Option<(String, String, String)>> {
    let path = central_root.join(ENGINEERING_GROUND_DIR).join(file);
    if !path.is_file() {
        return Ok(None);
    }
    let body = fs::read_to_string(&path)?;
    let title = body
        .lines()
        .find(|line| line.starts_with("# "))
        .map(|line| line.trim_start_matches("# ").trim().to_owned())
        .unwrap_or_else(|| file.trim_end_matches(".md").to_owned());
    let reference = source_ref(CONTROL_WORLD_REF, &format!("{ENGINEERING_GROUND_DIR}/{file}"));
    Ok(Some((title, body, reference)))
}

pub fn render_engineering_ground(central_root: &Path) -> io::Result<String> {
    let mut sections = Vec::new();
    for file in ENGINEERING_GROUND_STATEMENTS {
        if let Some((title, body, reference)) = statement_section(central_root, file)? {
            let revision = content_revision(
                &central_root.join(ENGINEERING_GROUND_DIR).join(file),
            )?;
            sections.push((title, body, reference, revision.revision));
        }
    }
    if sections.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("no engineering statements found under {ENGINEERING_GROUND_DIR}"),
        ));
    }

    let mut out = String::from(
        "# Foundational engineering prompt\n\n\
         <!-- derived source — provenance: generated-derived.\n\
         Do not edit this file; edit the statements under Control/agents/governance/engineering/ \
         and re-run control.engineering-ground.render.\n\
         derived-from:\n",
    );
    for (_, _, reference, revision) in &sections {
        out.push_str(&format!("- {reference} @ {revision}\n"));
    }
    out.push_str(" -->\n\n");
    for (title, body, _, _) in &sections {
        out.push_str(&format!("## {title}\n\n{body}\n\n"));
    }
    Ok(out)
}

pub fn write_engineering_ground(central_root: &Path) -> io::Result<EngineeringGroundRender> {
    let rendered = render_engineering_ground(central_root)?;
    let target = central_root.join(ENGINEERING_GROUND_OUTPUT);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let changed = match fs::read_to_string(&target) {
        Ok(existing) => existing != rendered,
        Err(error) if error.kind() == io::ErrorKind::NotFound => true,
        Err(error) => return Err(error),
    };
    if changed {
        fs::write(&target, &rendered)?;
    }
    Ok(EngineeringGroundRender {
        output_path: ENGINEERING_GROUND_OUTPUT.to_owned(),
        changed,
        revision: content_revision(&target)?,
    })
}
```

Then the actions, following `template_stamp.rs:251-346` exactly (same `descriptor`/`io_failure` shapes, no `project` input — these always render the Control root):

```rust
fn plan_action(_: &ActionRegistry, _: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let action = "control.engineering-ground.plan";
    let root = match resolve_central_root(context.root_options) {
        Ok(root) => root.path,
        Err(message) => {
            return ActionResult::failure(Some(action), ResultStatus::InvalidInput, message, None)
        }
    };
    let outcome = || -> io::Result<serde_json::Value> {
        let rendered = render_engineering_ground(&root)?;
        let target = root.join(ENGINEERING_GROUND_OUTPUT);
        let current = fs::read_to_string(&target).unwrap_or_default();
        Ok(serde_json::json!({
            "output_path": ENGINEERING_GROUND_OUTPUT,
            "would_change": current != rendered,
            "statements": ENGINEERING_GROUND_STATEMENTS.to_vec(),
        }))
    }();
    outcome
        .map(|value| ActionResult::success(action, value))
        .unwrap_or_else(|error| io_failure(action, error))
}

fn render_action(_: &ActionRegistry, _: &Value, context: &ActionExecutionContext<'_>) -> ActionResult {
    let action = "control.engineering-ground.render";
    let root = match resolve_central_root(context.root_options) {
        Ok(root) => root.path,
        Err(message) => {
            return ActionResult::failure(Some(action), ResultStatus::InvalidInput, message, None)
        }
    };
    write_engineering_ground(&root)
        .map(|value| {
            ActionResult::success(action, serde_json::to_value(value).expect("render serializes"))
        })
        .unwrap_or_else(|error| io_failure(action, error))
}

fn io_failure(action: &str, error: io::Error) -> ActionResult {
    let status = match error.kind() {
        io::ErrorKind::InvalidInput | io::ErrorKind::NotFound => ResultStatus::InvalidInput,
        io::ErrorKind::InvalidData => ResultStatus::VerificationFailure,
        _ => ResultStatus::InternalFailure,
    };
    ActionResult::failure(Some(action), status, error.to_string(), None)
}

fn descriptor(id: &str, title: &str, description: &str, mutation_class: MutationClass, output_type: &str) -> ActionDescriptor {
    ActionDescriptor {
        id: id.to_owned(),
        title: title.to_owned(),
        description: description.to_owned(),
        inputs: vec![ActionInputDefinition {
            name: "project".to_owned(),
            input_type: "string".to_owned(),
            required: false,
            choices: None,
            selection: None,
        }],
        output: ActionOutputDefinition { output_type: output_type.to_owned() },
        mutation_class,
        preview_supported: mutation_class == MutationClass::ReadOnly,
        required_ports: vec![],
        availability: ActionAvailability { available: true, reason: None },
    }
}

pub fn register_engineering_ground_actions(registry: &mut ActionRegistry) {
    let actions: [(ActionDescriptor, crate::action::ActionHandler); 2] = [
        (
            descriptor(
                "control.engineering-ground.plan",
                "Preview engineering-ground render",
                "Preview whether re-rendering the derived foundational prompt from the Control engineering governance statements would change it, without mutation.",
                MutationClass::ReadOnly,
                "central-engineering-ground-plan",
            ),
            plan_action,
        ),
        (
            descriptor(
                "control.engineering-ground.render",
                "Render engineering-ground prompt",
                "Render the Control engineering governance statements into the derived foundational prompt source (generated-derived; provenance refs pinned). Writes only when content changed; the distillate is machine-derivable and human adoption promotes it.",
                MutationClass::LocallyMutating,
                "central-engineering-ground-render",
            ),
            render_action,
        ),
    ];
    for (descriptor, handler) in actions {
        registry
            .register(descriptor, handler)
            .expect("engineering ground action ids are valid");
    }
}
```

(If `MutationClass` is not `PartialEq`, set `preview_supported` literally per action instead of comparing.)

Wire-up:
- `Central/ctrl/src/source_horizon.rs`: make `fn source_ref` (line 202) `pub(crate)`.
- `Central/ctrl/src/lib.rs`: add `mod engineering_ground;` (or `pub mod`), re-export `render_engineering_ground, write_engineering_ground, EngineeringGroundRender, ENGINEERING_GROUND_DIR, ENGINEERING_GROUND_OUTPUT, ENGINEERING_GROUND_STATEMENTS, register_engineering_ground_actions`.
- `Central/ctrl/src/cli.rs:384`: add `crate::engineering_ground::register_engineering_ground_actions(&mut registry);` next to the template-stamp registration.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd Central && cargo test -p central-ctrl --test engineering_ground`
Expected: 3 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add ctrl/src/engineering_ground.rs ctrl/src/source_horizon.rs ctrl/src/lib.rs ctrl/src/cli.rs ctrl/tests/engineering_ground.rs
git commit -m "feat(ctrl): render engineering ground into a derived foundational prompt

control.engineering-ground.plan/.render distill the Control governance
engineering statements into foundational-prompt.md as a generated-derived
source, with pinned statement provenance refs (content-fnv1a64) and CAS
write stability. Adopted copies participate as ordinary project sources."
```

---

### Task 4: ai-kit bootstrap provenance line

The managed `aikit-context` skill names the engineering-ground source when it is resolved into the project context — one line, never a payload copy.

**Files:**
- Modify: `ai-kit/crates/aikit-adapters/src/clients/bootstrap.rs` (after the Horizons section, ~line 112)
- Test: `ai-kit/crates/aikit-adapters/tests/bootstrap_render.rs`

**Interfaces:**
- Consumes: `ActorBootstrap.context_sources.examples: Vec<ResourceRef>` (existing); render helper `actor_bootstrap()` from `tests/actor_bootstrap_projection_v2.rs:33-60`.
- Produces: a `## Engineering ground` section in the rendered skill when (and only when) an example ref contains the path segment `governance/engineering/`.

- [ ] **Step 1: Write the failing test**

`ai-kit/crates/aikit-adapters/tests/bootstrap_render.rs`:

```rust
mod common;

use aikit_adapters::clients::bootstrap::render_managed_bootstrap;
use aikit_core::actor_bootstrap::{ActorBootstrap, ResourceSetSummary};
use aikit_core::resource::ResourceRef;

fn r(raw: &str) -> ResourceRef {
    ResourceRef::parse(raw).unwrap()
}

fn actor_bootstrap_with_engineering_source() -> ActorBootstrap {
    let mut bootstrap = actor_bootstrap();
    bootstrap.context_sources = ResourceSetSummary {
        total: 2,
        available: 2,
        unresolved: 0,
        unavailable: 0,
        examples: vec![
            r("central:source:control:root:Control/agents/governance/engineering/foundational-prompt.md"),
            r("context-source/project/readme"),
        ],
        truncated: false,
    };
    bootstrap
}

#[test]
fn renders_engineering_ground_provenance_line_when_resolved() {
    let rendered = render_managed_bootstrap(&actor_bootstrap_with_engineering_source());
    assert!(rendered.contains("## Engineering ground"));
    assert!(rendered.contains(
        "central:source:control:root:Control/agents/governance/engineering/foundational-prompt.md"
    ));
    assert!(rendered.contains("retrieve on demand"));
    // Named, not copied: the payload must not be inlined.
    assert!(rendered.lines().filter(|line| line.contains("You branch small")).count() == 0);
}

#[test]
fn omits_engineering_ground_section_when_absent() {
    let rendered = render_managed_bootstrap(&actor_bootstrap());
    assert!(!rendered.contains("## Engineering ground"));
}
```

Copy `actor_bootstrap()` and `empty_summary()` from `tests/actor_bootstrap_projection_v2.rs:33-60` into this file (or extract to `tests/common/mod.rs` if that file already shares helpers — follow the existing `mod common` convention).

- [ ] **Step 2: Run test to verify it fails**

Run: `cd ai-kit && cargo test -p aikit-adapters --test bootstrap_render`
Expected: FAIL — `## Engineering ground` not present (assertion failure, not compile error).

- [ ] **Step 3: Write minimal implementation**

In `render_managed_bootstrap` (`bootstrap.rs`), insert after the `projection_targets` block (before the `runtime_body` block at ~line 114):

```rust
    let engineering_ground: Vec<String> = bootstrap
        .context_sources
        .examples
        .iter()
        .map(|example| example.to_string())
        .filter(|example| example.contains("governance/engineering/"))
        .collect();
    if !engineering_ground.is_empty() {
        body.push_str("\n## Engineering ground\n\n");
        for reference in &engineering_ground {
            body.push_str(&format!(
                "- `{reference}` — distilled from Control governance engineering statements; retrieve on demand with `aikit context`. Named, not copied.\n"
            ));
        }
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd ai-kit && cargo test -p aikit-adapters --test bootstrap_render --test actor_bootstrap_projection_v2`
Expected: all PASS (new tests + existing bootstrap projection render suite unchanged).

- [ ] **Step 5: Commit**

```bash
git add crates/aikit-adapters/src/clients/bootstrap.rs crates/aikit-adapters/tests/bootstrap_render.rs
git commit -m "feat(aikit): name the engineering-ground source in the managed bootstrap

When a governance/engineering source is resolved into project context, the
aikit-context skill names it with an on-demand retrieval pointer — provenance
disclosure only, no payload copy (smallest-sufficient)."
```

---

## Self-review notes

- **Spec coverage:** spec §1 (statements) → Task 2; §2 (Control-tree sources) → Task 1; §3 (distillate) → Task 3; §4 (resolution doors) → Task 3 adoption test (project door) + existing AgentProfile `knowledge_source_refs`/`governance_refs` (no code gap found — the profile door is a data entry, documented in Task 2 README); §5 (setup conversation) → no code by design (uses existing `aikit capabilities` + `HumanSourceRevisionProposal` flows); §6 (bootstrap disclosure) → Task 4.
- **Type consistency:** `EngineeringGroundRender.revision: SourceRevision` matches `content_revision` return; action signatures match `template_stamp.rs`; test helpers reused across Tasks 1/3; render matching segment `governance/engineering/` consistent between Task 3 output path and Task 4 filter.
- **Known follow-ups (out of scope):** the agent-led setup conversation procedure (operational, not code); live-root adoption of the drafts by the person (their authorship act).
