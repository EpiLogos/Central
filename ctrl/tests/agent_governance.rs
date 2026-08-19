use central_ctrl::{
    apply_project_governance_relation, initialize_projectcentral, inspect_project_governance,
    inspect_root_governance, plan_project_governance, GovernanceProvenance,
    AGENT_GOVERNANCE_DIR, AGENT_RETRIEVAL_DENY_MARKER, GOVERNANCE_RELATIONS_SCHEMA,
    GOVERNANCE_RELATIONS_SOURCE, ROOT_AGENT_GOVERNANCE_DIR,
};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "central-governance-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn public_read_model_exposes_root_and_project_sources_without_precedence_claim() {
    let central = temporary_directory("layering").join("Central");
    let project = central.join("Work/product");
    fs::create_dir_all(central.join(ROOT_AGENT_GOVERNANCE_DIR)).unwrap();
    fs::create_dir_all(&project).unwrap();
    fs::write(
        central.join(ROOT_AGENT_GOVERNANCE_DIR).join("collaboration.md"),
        "Use exact evidence for consequential completion claims.\n",
    )
    .unwrap();
    initialize_projectcentral(&central, &project, "example/product").unwrap();
    fs::write(
        project.join(AGENT_GOVERNANCE_DIR).join("meaning.md"),
        "When product meaning changes, consult authored Project ground.\n",
    )
    .unwrap();

    let root = inspect_root_governance(&central).unwrap();
    let local = inspect_project_governance(&project).unwrap();
    assert_eq!(root.sources.len(), 1);
    assert_eq!(local.canonical_sources.len(), 1);
    assert_eq!(root.sources[0].scope, "cross-project");
    assert_eq!(local.canonical_sources[0].scope, "project");
    assert_ne!(root.sources[0].source_ref, local.canonical_sources[0].source_ref);
    assert_eq!(local.composition.operational_resolution_owner, "AIKit");
    assert!(!local.composition.operational_precedence_defined_by_central);
    assert!(local.composition.conflicts_must_remain_explainable);
}

#[test]
fn project_native_instruction_can_be_adopted_in_place_with_relation_provenance() {
    let central = temporary_directory("native").join("Central");
    let project = central.join("Work/native");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("AGENTS.md"), "Native project instruction.\n").unwrap();
    initialize_projectcentral(&central, &project, "example/native").unwrap();

    let plan = plan_project_governance(&project).unwrap();
    assert_eq!(plan.current.native_candidates.len(), 1);
    assert_eq!(plan.current.native_candidates[0].provenance, "unresolved");
    assert!(plan.proposals[0]
        .changes
        .iter()
        .any(|value| value.contains("operational precedence/composition to AIKit")));

    let applied = apply_project_governance_relation(
        &project,
        "AGENTS.md",
        GovernanceProvenance::HumanAdopted,
        vec!["project-collaboration".to_owned()],
    )
    .unwrap();
    assert!(!applied.source_bytes_mutated);
    assert!(!applied.source_path_mutated);
    assert!(!applied.operational_precedence_mutated);
    assert_eq!(
        fs::read_to_string(project.join("AGENTS.md")).unwrap(),
        "Native project instruction.\n"
    );

    let relation: Value = serde_json::from_slice(
        &fs::read(project.join(GOVERNANCE_RELATIONS_SOURCE)).unwrap(),
    )
    .unwrap();
    assert_eq!(relation["schema"], GOVERNANCE_RELATIONS_SCHEMA);
    assert_eq!(relation["project_id"], "example/native");
    assert_eq!(relation["relations"][0]["path"], "AGENTS.md");
    assert_eq!(relation["relations"][0]["provenance"], "human-adopted");

    let inspection = inspect_project_governance(&project).unwrap();
    assert!(inspection.native_candidates.is_empty());
    assert_eq!(inspection.retained_native_sources.len(), 1);
}

#[test]
fn denied_governance_subtree_is_not_exposed_by_stock_agent_read_model() {
    let central = temporary_directory("private").join("Central");
    let private = central.join(ROOT_AGENT_GOVERNANCE_DIR).join("private");
    fs::create_dir_all(&private).unwrap();
    fs::write(private.join(AGENT_RETRIEVAL_DENY_MARKER), "").unwrap();
    fs::write(private.join("private.md"), "human source, not agent-readable\n").unwrap();

    let inspection = inspect_root_governance(&central).unwrap();
    assert!(inspection.sources.is_empty());
    assert_eq!(inspection.skipped_sources.len(), 1);
    assert_eq!(inspection.skipped_sources[0].reason, "not-agent-readable");
}

#[test]
fn maintenance_read_model_keeps_observation_proposal_and_skill_boundaries() {
    let central = temporary_directory("maintenance").join("Central");
    let project = central.join("Work/maintenance");
    fs::create_dir_all(&project).unwrap();
    initialize_projectcentral(&central, &project, "example/maintenance").unwrap();

    let inspection = inspect_project_governance(&project).unwrap();
    assert!(!inspection
        .maintenance
        .observation_automatically_mutates_governance);
    assert!(inspection.maintenance.proposal_then_human_adoption);
    assert!(inspection.maintenance.pruning_is_normal_maintenance);
    assert!(inspection.maintenance.procedures_should_prefer_skills);
}
