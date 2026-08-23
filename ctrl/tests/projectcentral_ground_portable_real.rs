use central_ctrl::{
    apply_accepted_ground_relation, initialize_projectcentral, inspect_project_ground, GroundStatus,
    SourceProvenance, SourceStanding, SourceTreatment,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_root() -> PathBuf {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("central-ground-real-copy-{}-{nonce}", std::process::id()))
}

#[test]
fn portable_exact_central_sources_preserve_authored_position_vs_current_implementation() {
    let ctrl = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let real_project = ctrl.parent().expect("ctrl lives inside Central repository");
    let central = temporary_root();
    let project = central.join("Work/Central-current");
    fs::create_dir_all(project.join("docs")).unwrap();
    fs::create_dir_all(project.join("ctrl/src")).unwrap();

    // Copy exact files from the current checked-out Central Project rather than inventing fixture prose.
    fs::copy(real_project.join("README.md"), project.join("README.md")).unwrap();
    fs::copy(
        real_project.join("docs/CENTRAL-VISION.md"),
        project.join("docs/CENTRAL-VISION.md"),
    )
    .unwrap();
    fs::copy(
        real_project.join("ctrl/src/projectcentral_ground.rs"),
        project.join("ctrl/src/projectcentral_ground.rs"),
    )
    .unwrap();
    let vision_before = fs::read(project.join("docs/CENTRAL-VISION.md")).unwrap();
    let implementation_before = fs::read(project.join("ctrl/src/projectcentral_ground.rs")).unwrap();

    initialize_projectcentral(&central, &project, "epilogos/central-current").unwrap();
    let discovered = inspect_project_ground(&project).unwrap();
    let vision_candidate = discovered
        .native_candidates
        .iter()
        .find(|candidate| candidate.path == "docs/CENTRAL-VISION.md")
        .expect("the real vision source is discoverable");
    assert_eq!(vision_candidate.authorship, "unresolved");

    // Human acceptance is explicit even though the source is existing Project canon.
    apply_accepted_ground_relation(
        &project,
        "docs/CENTRAL-VISION.md",
        SourceProvenance::HumanAdopted,
        SourceStanding::AuthoredHumanPosition,
        SourceTreatment::RetainNativeInPlace,
        vec!["vision".to_owned(), "purpose".to_owned()],
    )
    .unwrap();
    apply_accepted_ground_relation(
        &project,
        "ctrl/src/projectcentral_ground.rs",
        SourceProvenance::Observed,
        SourceStanding::ImplementationFact,
        SourceTreatment::OrdinaryProjectSource,
        vec!["implementation".to_owned()],
    )
    .unwrap();

    let inspection = inspect_project_ground(&project).unwrap();
    assert_eq!(inspection.status, GroundStatus::Established);
    assert_eq!(inspection.account_handoff.recognised_human_sources.len(), 1);
    assert_eq!(inspection.account_handoff.other_source_relations.len(), 1);
    assert_eq!(
        inspection.account_handoff.recognised_human_sources[0].standing,
        SourceStanding::AuthoredHumanPosition
    );
    assert_eq!(
        inspection.account_handoff.other_source_relations[0].standing,
        SourceStanding::ImplementationFact
    );
    assert!(!inspection.return_policy.difference_automatically_mutates_human_source);

    // Establishing the relation neither reorganises nor rewrites the real-project source copy.
    assert_eq!(fs::read(project.join("docs/CENTRAL-VISION.md")).unwrap(), vision_before);
    assert_eq!(
        fs::read(project.join("ctrl/src/projectcentral_ground.rs")).unwrap(),
        implementation_before
    );
    assert!(project.join("ProjectCentral/agents/wiki/wiki.json").is_file());
    assert!(project.join("ProjectCentral/relations/source-relations.json").is_file());

    let _ = fs::remove_dir_all(central);
}
