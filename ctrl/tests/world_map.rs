use central_ctrl::{
    apply_reproject, create_flow, explain_project_world_map, explain_world_map, initialize_central,
    initialize_now, initialize_projectcentral, map_project_world, map_world, plan_reproject,
    read_project_manifest, run_cli, CliEnvironment, GroundState, ProjectCentralState,
    REPROJECT_PLAN_SCHEMA, REPROJECT_RECEIPT_SCHEMA, ResultStatus, ROOT_AGENT_GOVERNANCE_DIR,
    ROOT_HUMAN_SOURCE_DIR, ROOT_WIKI_SOURCE, WORLD_MAP_SCHEMA,
};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_TEMP_ROOT: AtomicU64 = AtomicU64::new(0);

struct TempRoot(PathBuf);

impl TempRoot {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let sequence = NEXT_TEMP_ROOT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "central-world-map-{label}-{}-{nonce}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path.join("Central"))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A healthy ground: required Control roots, an authored user source, the root
/// Wiki, and one conformant ProjectCentral Project.
fn healthy_ground(label: &str) -> TempRoot {
    let temp = TempRoot::new(label);
    let root = temp.path();
    initialize_central(root).unwrap();
    fs::create_dir_all(root.join(ROOT_HUMAN_SOURCE_DIR)).unwrap();
    fs::write(root.join(ROOT_HUMAN_SOURCE_DIR).join("identity.md"), "who I am\n").unwrap();
    fs::write(
        root.join(ROOT_AGENT_GOVERNANCE_DIR).join("engineering.md"),
        "You change the smallest thing that can work.\n",
    )
    .unwrap();

    let project = root.join("Work/garden");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("README.md"), "native project source\n").unwrap();
    initialize_projectcentral(root, &project, "garden").unwrap();
    temp
}

fn control_relations(root: &Path) -> PathBuf {
    root.join("Control/relations/source-relations.json")
}

#[test]
fn healthy_ground_maps_control_root_wiki_and_conformant_project() {
    let temp = healthy_ground("healthy");
    let root = temp.path();

    let map = map_world(root).unwrap();

    assert_eq!(map.schema, WORLD_MAP_SCHEMA);
    assert_eq!(map.ground_state, GroundState::Healthy);
    assert!(map.valid);
    assert!(!map.mixed_root.detected);

    // Control areas are counted without inventing new structure.
    assert!(map.control.user.exists);
    assert_eq!(map.control.user.sources, 1);
    assert_eq!(map.control.agent_governance.sources, 1);
    assert_eq!(map.control.agent_expressions.sources, 0);
    assert_eq!(map.control.machines.sources, 0);

    // The root Wiki is present with one federation ref, satisfied by the Project.
    assert!(map.control.agent_wiki.wiki.present);
    assert_eq!(map.control.agent_wiki.wiki.space_ref.as_deref(), Some("central:wiki:root"));
    assert_eq!(map.control.agent_wiki.wiki.child_space_refs, vec!["central:wiki:project:garden"]);
    assert!(map.control.agent_wiki.wiki.dangling_child_space_refs.is_empty());

    // No declared Control relations yet. Three sources participate: the authored
    // user source, the governance source, and the root Wiki, which carries
    // agent-maintained provenance rather than the unresolved tree stamp.
    assert!(!map.control.relations.present);
    assert_eq!(map.control.relations.declared_overrides, 0);
    assert_eq!(map.control.source_bindings, 3);
    assert_eq!(map.control.unresolved_provenance_sources, 2);

    let garden = map
        .work
        .projects
        .iter()
        .find(|project| project.name == "garden")
        .expect("garden is mapped");
    assert_eq!(garden.projectcentral.state, ProjectCentralState::Healthy);
    assert_eq!(garden.projectcentral.user.sources, 0);
    assert!(garden.projectcentral.agent_wiki.exists);
    assert!(garden.projectcentral.agent_wiki.wiki.present);
    assert_eq!(
        garden.projectcentral.agent_wiki.wiki.space_ref.as_deref(),
        Some("central:wiki:project:garden")
    );
    // A fresh ProjectCentral holds no Flow registry and no NOW folder; the
    // absence is data either way.
    assert!(!garden.projectcentral.flows.present);
    assert!(garden.projectcentral.flows.flows.is_empty());
    assert!(!garden.projectcentral.now.present);
    assert_eq!(garden.source_files, 1);
}

#[test]
fn absent_and_partial_projectcentral_are_reported_as_data() {
    let temp = healthy_ground("partial");
    let root = temp.path();

    // An ordinary native Project has no ProjectCentral at all.
    let native = root.join("Work/native");
    fs::create_dir_all(&native).unwrap();
    fs::write(native.join("README.md"), "native\n").unwrap();

    // A hand-made partial ProjectCentral: human source only, no manifest, no agents.
    let partial = root.join("Work/partial");
    fs::create_dir_all(partial.join("ProjectCentral/user")).unwrap();
    fs::write(partial.join("ProjectCentral/user/learnings.md"), "learned\n").unwrap();

    let map = map_world(root).unwrap();

    let native = map
        .work
        .projects
        .iter()
        .find(|project| project.name == "native")
        .unwrap();
    assert_eq!(native.projectcentral.state, ProjectCentralState::Absent);
    assert!(!native.projectcentral.agent_wiki.exists);
    assert!(!native.projectcentral.agent_wiki.wiki.present);
    assert!(!native.projectcentral.flows.present);
    assert!(!native.projectcentral.now.present);
    assert_eq!(native.projectcentral.now.path, "Work/native/ProjectCentral/now");
    assert_eq!(native.projectcentral.relations.path, "Work/native/ProjectCentral/relations/source-relations.json");

    let partial = map
        .work
        .projects
        .iter()
        .find(|project| project.name == "partial")
        .unwrap();
    assert_eq!(partial.projectcentral.state, ProjectCentralState::Partial);
    let missing = partial.projectcentral.missing.as_ref().expect("missing pieces");
    assert_eq!(
        missing,
        &vec![
            "ProjectCentral/project.json".to_owned(),
            "ProjectCentral/agents/governance".to_owned(),
            "ProjectCentral/agents/wiki".to_owned(),
            "ProjectCentral/agents/wiki/wiki.json".to_owned(),
        ]
    );
    assert_eq!(partial.projectcentral.user.sources, 1);
    assert!(!partial.projectcentral.agent_wiki.wiki.present);
}

#[test]
fn dangling_root_child_refs_are_faults_reported_as_data() {
    let temp = healthy_ground("dangling");
    let root = temp.path();

    // Federate a child ref that no Project Wiki answers.
    let wiki_path = root.join(ROOT_WIKI_SOURCE);
    let mut wiki: Value =
        serde_json::from_slice(&fs::read(&wiki_path).unwrap()).unwrap();
    wiki["objects"][0]["child_space_refs"]
        .as_array_mut()
        .unwrap()
        .push(json!("central:wiki:project:ghost"));
    fs::write(&wiki_path, serde_json::to_vec_pretty(&wiki).unwrap()).unwrap();

    let map = map_world(root).unwrap();

    // The map still succeeds: the fault is data, not a failure.
    assert_eq!(map.ground_state, GroundState::Healthy);
    assert_eq!(
        map.control.agent_wiki.wiki.child_space_refs,
        vec![
            "central:wiki:project:garden".to_owned(),
            "central:wiki:project:ghost".to_owned()
        ]
    );
    assert_eq!(
        map.control.agent_wiki.wiki.dangling_child_space_refs,
        vec!["central:wiki:project:ghost".to_owned()]
    );

    let human = explain_world_map(&serde_json::to_value(&map).unwrap());
    assert!(human.contains("2 child refs; 1 dangling"), "{human}");
    assert!(human.contains("Central ground healthy"), "{human}");
}

#[test]
fn declared_control_relations_override_unresolved_provenance() {
    let temp = healthy_ground("relations");
    let root = temp.path();
    let before = map_world(root).unwrap();
    let governed = before.control.source_bindings;
    let unresolved_before = before.control.unresolved_provenance_sources;
    assert_eq!(unresolved_before, governed - 1);

    fs::create_dir_all(root.join("Control/relations")).unwrap();
    fs::write(
        control_relations(root),
        serde_json::to_string_pretty(&json!({
            "schema": "central.control.ground-relations/v1",
            "project_id": "control:root",
            "relations": [{
                "ref": "central:source:control:root:Control/agents/governance/engineering.md",
                "path": "Control/agents/governance/engineering.md",
                "provenance": "generated-suggestion",
                "standing": "draft-source",
                "roles": ["agent-governance-source"],
                "treatment": "control-governance-retained-in-place"
            }]
        }))
        .unwrap(),
    )
    .unwrap();

    let after = map_world(root).unwrap();

    assert!(after.control.relations.present);
    assert_eq!(after.control.relations.declared_overrides, 1);
    assert_eq!(
        after.control.relations.schema.as_deref(),
        Some("central.control.ground-relations/v1")
    );
    assert_eq!(after.control.source_bindings, governed);
    assert_eq!(
        after.control.unresolved_provenance_sources,
        unresolved_before - 1
    );
}

#[test]
fn control_relations_with_the_wrong_world_id_are_an_error_field_not_a_crash() {
    let temp = healthy_ground("wrong-relations");
    let root = temp.path();
    fs::create_dir_all(root.join("Control/relations")).unwrap();
    fs::write(
        control_relations(root),
        json!({
            "schema": "central.project.ground-relations/v1",
            "project_id": "some/project",
            "relations": []
        })
        .to_string(),
    )
    .unwrap();

    let map = map_world(root).unwrap();

    assert!(map.control.relations.present);
    assert_eq!(map.control.relations.declared_overrides, 0);
    assert!(map.control.relations.error.is_some());
    assert!(map.control.bindings_error.is_some());
}

#[test]
fn mixed_root_is_the_ground_state() {
    let temp = TempRoot::new("mixed");
    let root = temp.path();
    initialize_central(root).unwrap();
    // Product-specific signals, per the mixed-root diagnostic contract.
    fs::write(root.join("Cargo.toml"), "[workspace]\n").unwrap();
    fs::create_dir_all(root.join("ctrl/src")).unwrap();

    let map = map_world(root).unwrap();

    assert_eq!(map.ground_state, GroundState::MixedRoot);
    assert!(map.mixed_root.detected);
    let human = explain_world_map(&serde_json::to_value(&map).unwrap());
    assert!(human.contains("Central ground mixed_root"), "{human}");
}

#[test]
fn the_world_command_is_read_only_and_stable_in_json() {
    let temp = healthy_ground("readonly");
    let root = temp.path();

    fn ground_fingerprint(root: &Path) -> Vec<(String, u64)> {
        fn walk(dir: &Path, out: &mut Vec<(String, u64)>) {
            for entry in fs::read_dir(dir).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, out);
                } else {
                    let length = fs::metadata(&path).unwrap().len();
                    out.push((path.to_string_lossy().into_owned(), length));
                }
            }
        }
        let mut out = Vec::new();
        walk(root, &mut out);
        out.sort();
        out
    }

    let before = ground_fingerprint(root);
    let environment = CliEnvironment { configured_root: Some(root.to_path_buf()), home: None };

    let structured = run_cli(&["--json".to_owned(), "world".to_owned()], &environment);
    let structured_again = run_cli(&["--json".to_owned(), "world".to_owned()], &environment);
    let human = run_cli(&["world".to_owned()], &environment);

    assert_eq!(structured.result.status, ResultStatus::Success);
    assert_eq!(structured.result.action.as_deref(), Some("central.world"));
    assert_eq!(structured.exit_code, 0);

    // The machine projection is schema-tagged and stable across runs.
    let data = structured.result.data.clone().expect("world data");
    assert_eq!(data["schema"], WORLD_MAP_SCHEMA);
    assert_eq!(data["ground_state"], "healthy");
    assert_eq!(data["control"]["agent_wiki"]["wiki"]["space_ref"], "central:wiki:root");
    assert_eq!(data["work"]["projects"][0]["name"], "garden");
    assert_eq!(
        serde_json::to_string(&structured.result).unwrap(),
        serde_json::to_string(&structured_again.result).unwrap()
    );

    // The human projection is the same world, readable.
    assert_eq!(human.result.status, ResultStatus::Success);
    assert!(human.output.contains("Central ground healthy"), "{}", human.output);
    assert!(human.output.contains("garden — ProjectCentral healthy"), "{}", human.output);
    assert!(human.output.contains("unresolved provenance —"), "{}", human.output);

    // Read-only: nothing on the ground changed, not even a derived file.
    assert_eq!(before, ground_fingerprint(root));
}

fn ground_fingerprint(root: &Path) -> Vec<(String, u64)> {
    fn walk(dir: &Path, out: &mut Vec<(String, u64)>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else {
                let length = fs::metadata(&path).unwrap().len();
                out.push((path.to_string_lossy().into_owned(), length));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out.sort();
    out
}

#[test]
fn flows_and_now_are_disclosed_and_absence_is_data() {
    let temp = healthy_ground("flows");
    let root = temp.path();
    let project = root.join("Work/garden");

    initialize_now(&project).unwrap();
    let flow = create_flow(
        &project,
        Some("2026-09-05-0900"),
        None,
        Some("Garden log".to_owned()),
        "human:test",
        "human",
        None,
    )
    .unwrap();
    let registry_bytes = fs::read(project.join(".central/flows.json")).unwrap();

    let map = map_world(root).unwrap();
    let garden = map
        .work
        .projects
        .iter()
        .find(|project| project.name == "garden")
        .expect("garden is mapped");

    // The Flow is disclosed with its identity, revision and lifecycle.
    assert!(garden.projectcentral.flows.present);
    assert_eq!(garden.projectcentral.flows.flows.len(), 1);
    let entry = &garden.projectcentral.flows.flows[0];
    assert_eq!(entry.flow_ref, flow.flow_ref);
    assert_eq!(entry.source_ref, flow.source_ref);
    assert_eq!(entry.path, flow.path);
    assert_eq!(entry.lifecycle, "active");
    assert_eq!(entry.title.as_deref(), Some("Garden log"));
    assert!(entry.revision.starts_with("central.content-fnv1a64/v1:"));
    assert_eq!(entry.revisions_recorded, 1);
    assert_eq!(entry.uncommitted_edits, Some(false));
    assert_eq!(garden.projectcentral.flows.active, 1);

    // The NOW folder is disclosed by counts, not contents.
    assert!(garden.projectcentral.now.present);
    assert_eq!(garden.projectcentral.now.path, "Work/garden/ProjectCentral/now");
    assert_eq!(garden.projectcentral.now.human_scratch, 0);
    assert_eq!(garden.projectcentral.now.active_items, 0);
    assert_eq!(garden.projectcentral.now.day_records, 0);

    // An external edit shows as uncommitted work, and reading the map never
    // absorbs it: the registry keeps its bytes.
    fs::write(project.join(&flow.path), "an external editor was here\n").unwrap();
    let after_edit = map_world(root).unwrap();
    let entry = &after_edit
        .work
        .projects
        .iter()
        .find(|project| project.name == "garden")
        .unwrap()
        .projectcentral
        .flows
        .flows[0];
    assert_eq!(entry.uncommitted_edits, Some(true));
    map_world(root).unwrap();
    assert_eq!(
        registry_bytes,
        fs::read(project.join(".central/flows.json")).unwrap(),
        "mapping must not reconcile or rewrite the Flow registry"
    );

    // A Project with no ProjectCentral reports the absence as data.
    let native = root.join("Work/native");
    fs::create_dir_all(&native).unwrap();
    let map = map_world(root).unwrap();
    let native = map
        .work
        .projects
        .iter()
        .find(|project| project.name == "native")
        .unwrap();
    assert!(!native.projectcentral.flows.present);
    assert!(native.projectcentral.flows.flows.is_empty());
    assert!(native.projectcentral.flows.error.is_none());
    assert!(!native.projectcentral.now.present);

    let human = explain_world_map(&serde_json::to_value(&map).unwrap());
    assert!(human.contains("flows 1 (1 active; 1 with uncommitted edits)"), "{human}");
    assert!(human.contains("now absent"), "{human}");
}

#[test]
fn sub_world_projection_centres_the_tree_on_one_project() {
    let temp = healthy_ground("sub-world");
    let root = temp.path();

    let before = ground_fingerprint(root);
    let projection = map_project_world(root, "garden").unwrap();

    assert_eq!(projection.schema, WORLD_MAP_SCHEMA);
    assert_eq!(projection.projection, central_ctrl::WorldProjection::Project);
    assert_eq!(projection.ground_state, GroundState::Healthy);
    assert_eq!(projection.project.name, "garden");
    assert_eq!(projection.project.path, "Work/garden");
    assert_eq!(projection.project.projectcentral.state, ProjectCentralState::Healthy);
    assert_eq!(
        projection.project.projectcentral.agent_wiki.wiki.space_ref.as_deref(),
        Some("central:wiki:project:garden")
    );

    // Sources, with the provenance the Source Change Horizon stamps.
    assert!(projection.sources.error.is_none());
    assert_eq!(projection.sources.bindings, 1);
    assert_eq!(projection.sources.unresolved_provenance, 0);
    assert_eq!(
        projection.sources.by_provenance.get("agent-maintained"),
        Some(&1)
    );

    // How it sits under Work.
    assert_eq!(projection.position.work_root, "Work");
    assert_eq!(projection.position.index, Some(1));
    assert_eq!(projection.position.project_count, 1);

    let human = explain_project_world_map(&serde_json::to_value(&projection).unwrap());
    assert!(human.contains("project garden (Work/garden)"), "{human}");
    assert!(human.contains("ProjectCentral (healthy)"), "{human}");
    assert!(human.contains("under Work — project 1 of 1"), "{human}");

    // A Project name is an address, not a path to escape through.
    assert!(map_project_world(root, "ghost").is_err());
    assert!(map_project_world(root, "../escape").is_err());

    assert_eq!(before, ground_fingerprint(root));
}

#[test]
fn sub_world_reports_partial_and_absent_pieces_as_data() {
    let temp = healthy_ground("sub-world-partial");
    let root = temp.path();

    let partial = root.join("Work/draft");
    fs::create_dir_all(partial.join("ProjectCentral/user")).unwrap();
    fs::write(partial.join("ProjectCentral/user/notes.md"), "notes\n").unwrap();

    let projection = map_project_world(root, "draft").unwrap();

    assert_eq!(projection.project.projectcentral.state, ProjectCentralState::Partial);
    let missing = projection
        .project
        .projectcentral
        .missing
        .as_ref()
        .expect("missing pieces are named");
    assert!(missing.contains(&"ProjectCentral/project.json".to_owned()));
    assert!(!projection.project.projectcentral.agent_wiki.wiki.present);
    assert!(!projection.project.projectcentral.flows.present);
    assert!(!projection.project.projectcentral.now.present);
    // No manifest means no Source Change Horizon bindings to count; the reason
    // is reported instead of guessed around.
    assert_eq!(projection.sources.bindings, 0);
    assert!(projection.sources.error.is_some());
    assert_eq!(projection.position.index, Some(1));
    assert_eq!(projection.position.project_count, 2);

    let human = explain_project_world_map(&serde_json::to_value(&projection).unwrap());
    assert!(human.contains("ProjectCentral (partial)"), "{human}");
    assert!(human.contains("missing ProjectCentral/project.json"), "{human}");
    assert!(human.contains("now absent"), "{human}");
}

fn draft_project(root: &Path) {
    let draft = root.join("Work/draft");
    fs::create_dir_all(draft.join("ProjectCentral/user")).unwrap();
    fs::write(draft.join("ProjectCentral/user/ideas.md"), "human idea\n").unwrap();
    fs::write(draft.join("ProjectCentral/left-over.txt"), "litter\n").unwrap();
}

#[test]
fn reproject_plan_reports_missing_canonical_scaffold_and_never_moves_anything() {
    let temp = healthy_ground("reproject-plan");
    let root = temp.path();
    draft_project(root);

    let before = ground_fingerprint(root);
    let plan = plan_reproject(root, "draft").unwrap();

    assert_eq!(plan.schema, REPROJECT_PLAN_SCHEMA);
    assert_eq!(plan.mutation, "additive-only");
    assert!(!plan.noop);
    // No manifest: the identity is derived, and the plan says so.
    assert_eq!(plan.project_id.as_deref(), Some("draft"));
    assert_eq!(plan.project_id_source.as_deref(), Some("derived-from-directory-name"));

    let stamped: Vec<&str> = plan
        .would_stamp
        .iter()
        .map(|step| step.path.as_str())
        .collect();
    assert_eq!(
        stamped,
        vec![
            "Work/draft/ProjectCentral/project.json",
            "Work/draft/ProjectCentral/agents/governance",
            "Work/draft/ProjectCentral/agents/wiki",
            "Work/draft/ProjectCentral/agents/wiki/wiki.json",
            "Work/draft/ProjectCentral/relations",
        ]
    );
    let present: Vec<&str> = plan
        .already_present
        .iter()
        .map(|step| step.path.as_str())
        .collect();
    assert_eq!(present, vec!["Work/draft/ProjectCentral", "Work/draft/ProjectCentral/user"]);

    // What the plan sees is classified, not acted on.
    let ideas = plan
        .left_alone
        .iter()
        .find(|entry| entry.path == "ProjectCentral/user/ideas.md")
        .expect("human file is classified");
    assert_eq!(ideas.provenance, "unresolved");
    assert_eq!(ideas.treatment.as_deref(), Some("projectcentral-user"));
    let litter = plan
        .left_alone
        .iter()
        .find(|entry| entry.path == "ProjectCentral/left-over.txt")
        .expect("litter is classified");
    assert!(litter.note.contains("out-of-place"), "{}", litter.note);
    assert!(plan.left_alone.iter().all(|entry| entry.provenance == "unresolved"));

    // The never-do contract is part of the plan itself.
    assert!(plan
        .would_not
        .contains(&"never move, rename, delete, or relabel anything".to_owned()));
    assert!(plan
        .would_not
        .contains(&"never write Wiki content: reprojection stamps structure only".to_owned()));
    assert!(plan.would_not.iter().any(|rule| rule.contains("source-relations.json")));

    // A plan touches nothing.
    assert_eq!(before, ground_fingerprint(root));

    // An already bound ProjectCentral plans only what the fractal is still
    // missing — here, just the relations container.
    let garden_plan = plan_reproject(root, "garden").unwrap();
    assert_eq!(
        garden_plan
            .would_stamp
            .iter()
            .map(|step| step.path.as_str())
            .collect::<Vec<_>>(),
        vec!["Work/garden/ProjectCentral/relations"]
    );
    assert!(!garden_plan.noop);
    assert_eq!(garden_plan.project_id_source.as_deref(), Some("manifest"));
    assert_eq!(garden_plan.project_id.as_deref(), Some("garden"));
}

#[test]
fn reproject_apply_stamps_only_missing_scaffold_and_leaves_litter_alone() {
    let temp = healthy_ground("reproject-apply");
    let root = temp.path();
    draft_project(root);

    let plan = plan_reproject(root, "draft").unwrap();
    let ideas_before = fs::read(root.join("Work/draft/ProjectCentral/user/ideas.md")).unwrap();
    let litter_before = fs::read(root.join("Work/draft/ProjectCentral/left-over.txt")).unwrap();
    let root_wiki_before = fs::read(root.join(ROOT_WIKI_SOURCE)).unwrap();

    let receipt = apply_reproject(root, "draft").unwrap();

    assert_eq!(receipt.schema, REPROJECT_RECEIPT_SCHEMA);
    assert_eq!(receipt.mutation, "additive-only");
    let stamped: Vec<&str> = receipt.stamped.iter().map(|step| step.path.as_str()).collect();
    let planned: Vec<&str> = plan.would_stamp.iter().map(|step| step.path.as_str()).collect();
    assert_eq!(stamped, planned);

    // The stamped manifest and Wiki space are canonical, and nothing else.
    let manifest = read_project_manifest(&root.join("Work/draft")).unwrap();
    assert!(manifest.validate().valid);
    assert_eq!(manifest.project_id, "draft");
    assert_eq!(manifest.human_source, "ProjectCentral/user");
    let wiki: Value = serde_json::from_slice(
        &fs::read(root.join("Work/draft/ProjectCentral/agents/wiki/wiki.json")).unwrap(),
    )
    .unwrap();
    let space = &wiki["objects"][0];
    assert_eq!(space["ref"], "central:wiki:project:draft");
    assert_eq!(space["parent_space_refs"], json!(["central:wiki:root"]));
    assert_eq!(space["child_space_refs"], json!([]));
    assert_eq!(space["node_refs"], json!([]));

    // Litter, human files and the root Wiki are exactly where they were.
    assert_eq!(ideas_before, fs::read(root.join("Work/draft/ProjectCentral/user/ideas.md")).unwrap());
    assert_eq!(litter_before, fs::read(root.join("Work/draft/ProjectCentral/left-over.txt")).unwrap());
    assert_eq!(root_wiki_before, fs::read(root.join(ROOT_WIKI_SOURCE)).unwrap());
    assert!(root.join("Work/draft/ProjectCentral/relations").is_dir());
    assert!(!root
        .join("Work/draft/ProjectCentral/relations/source-relations.json")
        .exists());
    assert!(!root.join("Work/draft/ProjectCentral/provenance.json").exists());

    // What was left alone is reported, not silently omitted.
    assert!(receipt
        .left_alone
        .iter()
        .any(|entry| entry.path == "ProjectCentral/left-over.txt"));
    assert!(receipt
        .left_alone
        .iter()
        .any(|entry| entry.path == "ProjectCentral/user/ideas.md"));

    // Reprojection is idempotent: the second apply stamps nothing and the
    // ground does not move.
    let stable = ground_fingerprint(root);
    let again = apply_reproject(root, "draft").unwrap();
    assert!(again.noop);
    assert!(again.stamped.is_empty());
    assert_eq!(stable, ground_fingerprint(root));
}

#[test]
fn reproject_leaves_human_files_untouched_and_reports_provenance_honestly() {
    let temp = healthy_ground("reproject-human");
    let root = temp.path();

    // An authored Wiki-area file sits in the fractal before reprojection runs.
    let authored = root.join("Work/authored");
    fs::create_dir_all(authored.join("ProjectCentral/agents/wiki")).unwrap();
    fs::write(
        authored.join("ProjectCentral/agents/wiki/scratch.md"),
        "wiki tooling's material, not mine to move\n",
    )
    .unwrap();
    fs::create_dir_all(authored.join("ProjectCentral/relations")).unwrap();
    fs::write(
        authored.join("ProjectCentral/relations/source-relations.json"),
        json!({
            "schema": "central.project.ground-relations/v1",
            "project_id": "authored",
            "relations": [{
                "ref": "central:source:project:authored:ProjectCentral/user/notes.md",
                "path": "ProjectCentral/user/notes.md",
                "provenance": "human-adopted",
                "standing": "adopted-source",
                "roles": ["project-human-source-aperture"],
                "treatment": "projectcentral-user"
            }]
        })
        .to_string(),
    )
    .unwrap();

    let plan = plan_reproject(root, "authored").unwrap();

    // The wiki directory already exists, so only its missing wiki.json would be
    // stamped; the authored file inside it is classified with the same
    // provenance the Source Change Horizon stamps Wiki sources with.
    let scratch = plan
        .left_alone
        .iter()
        .find(|entry| entry.path == "ProjectCentral/agents/wiki/scratch.md")
        .expect("wiki-area file is classified");
    assert_eq!(scratch.provenance, "agent-maintained");
    assert_eq!(scratch.treatment.as_deref(), Some("projectcentral-agent-wiki"));
    assert_eq!(
        plan.left_alone
            .iter()
            .find(|entry| entry.path == "ProjectCentral/relations/source-relations.json")
            .expect("relations file is classified")
            .note
            .contains("never stamped"),
        true
    );
    assert_eq!(plan.declared_relations, 1);
    assert!(plan
        .would_stamp
        .iter()
        .any(|step| step.path == "Work/authored/ProjectCentral/agents/wiki/wiki.json"));
    assert!(!plan
        .would_stamp
        .iter()
        .any(|step| step.path.ends_with("source-relations.json")));

    let scratch_before = fs::read(authored.join("ProjectCentral/agents/wiki/scratch.md")).unwrap();
    let relations_before =
        fs::read(authored.join("ProjectCentral/relations/source-relations.json")).unwrap();

    let receipt = apply_reproject(root, "authored").unwrap();

    assert_eq!(
        scratch_before,
        fs::read(authored.join("ProjectCentral/agents/wiki/scratch.md")).unwrap()
    );
    assert_eq!(
        relations_before,
        fs::read(authored.join("ProjectCentral/relations/source-relations.json")).unwrap()
    );
    assert!(receipt
        .would_not
        .contains(&"never write Wiki content: reprojection stamps structure only".to_owned()));
    assert!(receipt
        .would_not
        .iter()
        .any(|rule| rule.contains("never stamp authored relations")));
}

#[test]
fn reproject_with_an_unreadable_manifest_stamps_directories_only_and_never_rewrites() {
    let temp = healthy_ground("reproject-broken");
    let root = temp.path();

    let broken = root.join("Work/broken");
    fs::create_dir_all(broken.join("ProjectCentral")).unwrap();
    fs::write(broken.join("ProjectCentral/project.json"), "not json\n").unwrap();

    let plan = plan_reproject(root, "broken").unwrap();

    // The manifest exists but cannot be read: no identity, so no identity files.
    assert!(plan.project_id.is_none());
    let blocked = plan.blocked.expect("unreadable manifest blocks identity stamps");
    assert!(blocked.contains("cannot be read"), "{blocked}");
    assert!(plan
        .would_stamp
        .iter()
        .all(|step| step.kind == central_ctrl::ScaffoldKind::Directory));
    let manifest_entry = plan
        .left_alone
        .iter()
        .find(|entry| entry.path == "ProjectCentral/project.json")
        .expect("unreadable manifest is classified");
    assert_eq!(manifest_entry.provenance, "unresolved");

    let manifest_before = fs::read(broken.join("ProjectCentral/project.json")).unwrap();
    let receipt = apply_reproject(root, "broken").unwrap();

    assert_eq!(manifest_before, fs::read(broken.join("ProjectCentral/project.json")).unwrap());
    // Directories were stamped, identity files were not, and the receipt says so.
    assert!(!receipt.stamped.is_empty());
    assert!(receipt
        .stamped
        .iter()
        .all(|step| step.kind == central_ctrl::ScaffoldKind::Directory));
    assert!(!receipt.noop);
    assert_eq!(
        receipt.blocked.expect("blocked is reported").contains("cannot be read"),
        true
    );
    assert!(!broken.join("ProjectCentral/agents/wiki/wiki.json").exists());
}

/// W10 V1: the identity anchor (central.pasu/v1) is exposed by the world map,
/// the ground-relations subject ref validates against the grammar, and the
/// two sides of the subject-ref agreement are reported as data.
#[test]
fn world_map_exposes_the_pasu_identity_anchor_and_validates_the_subject_ref() {
    let temp = healthy_ground("pasu-identity");
    let root = temp.path();

    // Absence is data: no manifest, no declared subject ref.
    let map = map_world(root).unwrap();
    assert!(!map.control.identity.present);
    assert_eq!(map.control.identity.subject_ref_consistent, None);

    // The authored identity source stays as-is; the manifest is a new carrier.
    fs::create_dir_all(root.join("Control/user/identity/sources")).unwrap();
    fs::write(root.join("Control/user/identity/present.md"), "who I am now\n").unwrap();
    fs::write(
        root.join("Control/user/identity/sources/natal-chart.md"),
        "promoted copy\n",
    )
    .unwrap();
    let manifest = json!({
        "schema": "central.pasu.identity-manifest/v1",
        "revision": "1",
        "subject": {"ref": "central:pasu:nara:local"},
        "identity_source": {
            "path": "Control/user/identity",
            "provenance_law": "vault-first; promote by recognised promotion",
            "sources": [
                {"path": "Control/user/identity/present.md", "standing": "authored-ground"},
                {"path": "Control/user/identity/sources/natal-chart.md",
                 "standing": "promoted-source", "promoted": "2026-09-03", "revision": "1"}
            ]
        }
    });
    fs::write(
        root.join("Control/user/identity/manifest.json"),
        serde_json::to_string_pretty(&manifest).unwrap() + "\n",
    )
    .unwrap();

    fs::create_dir_all(root.join("Control/relations")).unwrap();
    let relations = json!({
        "schema": "central.control.ground-relations/v1",
        "project_id": "control:root",
        "subject_ref": "central:pasu:nara:local",
        "relations": []
    });
    fs::write(
        control_relations(root),
        serde_json::to_string_pretty(&relations).unwrap() + "\n",
    )
    .unwrap();

    let map = map_world(root).unwrap();
    let identity = &map.control.identity;
    assert!(identity.present);
    assert_eq!(identity.subject_ref.as_deref(), Some("central:pasu:nara:local"));
    assert_eq!(identity.form.as_deref(), Some("nara"));
    assert_eq!(identity.ground_relations_subject_ref.as_deref(), Some("central:pasu:nara:local"));
    assert_eq!(identity.subject_ref_consistent, Some(true));
    assert_eq!(identity.sourced_files.len(), 2);
    assert!(identity.sourced_files.iter().all(|file| file.present));
    assert!(identity.sourced_files.iter().all(|file| file.content_revision.is_some()));
    assert!(identity.error.is_none());

    // A subject ref outside the grammar is an invalid ground-relations file.
    let bad = json!({
        "schema": "central.control.ground-relations/v1",
        "project_id": "control:root",
        "subject_ref": "central:user",
        "relations": []
    });
    fs::write(
        control_relations(root),
        serde_json::to_string_pretty(&bad).unwrap() + "\n",
    )
    .unwrap();
    let map = map_world(root).unwrap();
    assert!(map
        .control
        .bindings_error
        .as_deref()
        .unwrap_or_default()
        .contains("subject ref is invalid"));
    // The identity side still reads; only the declared side is unavailable.
    assert!(map.control.identity.present);
    assert_eq!(map.control.identity.ground_relations_subject_ref, None);
}
