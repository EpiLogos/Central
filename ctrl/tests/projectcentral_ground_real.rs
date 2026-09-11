use central_ctrl::{inspect_project_ground, GroundStatus};
use std::path::PathBuf;

#[test]
fn current_central_checkout_is_a_real_project_ground_specimen() {
    let ctrl = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let project = ctrl.parent().expect("ctrl lives inside Central repository");
    assert!(project.join("README.md").is_file());
    assert!(project.join("docs/CENTRAL-VISION.md").is_file());
    assert!(project.join("ProjectCentral/project.json").is_file());

    let inspection = inspect_project_ground(project).expect("real Central checkout is inspectable");

    // The checkout carries its own established project ground (the Central
    // pilot account convention), and inspection reports it without adopting
    // anything itself. Deviation D5, Work/wiki-continuity-2026-09-06.
    assert!(inspection.projectcentral_ready);
    assert_eq!(inspection.status, GroundStatus::Partial);
    assert!(inspection
        .account_handoff
        .recognised_human_sources
        .is_empty());

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
}

#[test]
fn a_directory_without_projectcentral_inspects_as_empty_and_is_never_adopted() {
    let base = std::env::temp_dir().join(format!("central-ground-specimen-{}", std::process::id()));
    std::fs::create_dir_all(&base).expect("temp ground");
    std::fs::write(base.join("README.md"), "# Purpose\n").expect("native candidate");

    let inspection = inspect_project_ground(&base).expect("plain directory is inspectable");

    // The read-model guarantee the repository specimen used to carry: a ground
    // without a ProjectCentral manifest is reported honestly as empty, and
    // inspection never converts it into a Project.
    assert!(!inspection.projectcentral_ready);
    assert_eq!(inspection.status, GroundStatus::Empty);
    assert!(!base.join("ProjectCentral").exists());
    let _ = std::fs::remove_dir_all(&base);
}
