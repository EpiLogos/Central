use central_ctrl::{inspect_project_ground, GroundStatus};
use std::path::PathBuf;

#[test]
fn current_central_checkout_is_a_real_conservative_existing_project_specimen() {
    let ctrl = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let project = ctrl.parent().expect("ctrl lives inside Central repository");
    assert!(project.join("README.md").is_file());
    assert!(project.join("docs/CENTRAL-VISION.md").is_file());

    let inspection = inspect_project_ground(project).expect("real Central checkout is inspectable");

    // The repository itself is not silently converted into a ProjectCentral Project by inspection.
    assert!(!inspection.projectcentral_ready);
    assert_eq!(inspection.status, GroundStatus::Empty);
    assert!(inspection.account_handoff.recognised_human_sources.is_empty());

    // Role-like native source is discoverable, but authorship/authority remain unresolved.
    let readme = inspection
        .native_candidates
        .iter()
        .find(|candidate| candidate.path == "README.md")
        .expect("real README should be a ground candidate");
    assert_eq!(readme.authorship, "unresolved");
    assert_eq!(readme.standing, "unresolved");
    assert!(readme.role_hints.iter().any(|role| role == "purpose"));

    let vision = inspection
        .native_candidates
        .iter()
        .find(|candidate| candidate.path == "docs/CENTRAL-VISION.md")
        .expect("real Central vision should be a ground candidate");
    assert_eq!(vision.authorship, "unresolved");
    assert!(vision.role_hints.iter().any(|role| role == "vision"));

    // Inspection is a read model, not an adoption/migration operation.
    assert!(!project.join("ProjectCentral").exists());
}
