use central_ctrl::{
    initialize_projectcentral, inspect_project_governance, inspect_project_ground, GroundCandidate,
    WIKI_SOURCE,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "central-project-context-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn candidate<'a>(candidates: &'a [GroundCandidate], path: &str) -> &'a GroundCandidate {
    candidates
        .iter()
        .find(|candidate| candidate.path == path)
        .unwrap_or_else(|| panic!("missing Project-ground candidate: {path}"))
}

fn assert_unresolved(candidate: &GroundCandidate) {
    assert_eq!(candidate.authorship, "unresolved");
    assert_eq!(candidate.standing, "unresolved");
    assert!(candidate
        .reason
        .contains("does not infer human authorship or authority"));
}

#[test]
fn mixed_project_context_preserves_role_authority_and_presentation_distinctions() {
    let central = temporary_directory("mixed").join("Central");
    let project = central.join("Work/contextual");
    fs::create_dir_all(project.join("intent")).unwrap();
    fs::create_dir_all(project.join("docs")).unwrap();

    fs::write(
        project.join("VISION.md"),
        "A stabilised intended horizon for the Project.\n",
    )
    .unwrap();
    fs::write(
        project.join("intent/feature.md"),
        "Bounded desired change, constraints and success conditions.\n",
    )
    .unwrap();
    fs::write(
        project.join("docs/ARCHITECTURE.md"),
        "Current architectural relation.\n",
    )
    .unwrap();
    fs::write(
        project.join("experience-prototype.html"),
        "<main>Desired Project encounter</main>\n",
    )
    .unwrap();
    fs::write(
        project.join("AGENTS.md"),
        "Consult authored Project ground when product meaning is at stake.\n",
    )
    .unwrap();

    initialize_projectcentral(&central, &project, "example/contextual").unwrap();

    let ground = inspect_project_ground(&project).unwrap();
    assert!(project.join(WIKI_SOURCE).is_file());

    let vision = candidate(&ground.native_candidates, "VISION.md");
    assert!(vision.role_hints.iter().any(|role| role == "vision"));
    assert!(vision.role_hints.iter().any(|role| role == "purpose"));
    assert_unresolved(vision);

    let intent = candidate(&ground.native_candidates, "intent/feature.md");
    assert!(intent.role_hints.iter().any(|role| role == "intent"));
    assert!(intent.role_hints.iter().any(|role| role == "purpose"));
    assert_unresolved(intent);

    let architecture = candidate(&ground.native_candidates, "docs/ARCHITECTURE.md");
    assert!(architecture
        .role_hints
        .iter()
        .any(|role| role == "architecture"));
    assert_unresolved(architecture);

    let html = candidate(&ground.native_candidates, "experience-prototype.html");
    assert!(html
        .role_hints
        .iter()
        .any(|role| role == "html-prototype-or-presentation"));
    assert!(html
        .role_hints
        .iter()
        .any(|role| role == "mockup-prototype"));
    assert!(html
        .role_hints
        .iter()
        .any(|role| role == "desired-experience"));
    assert_unresolved(html);

    // Presentation is not source merely because the candidate is HTML.
    assert!(!ground.account_handoff.html_is_source);
    assert!(!ground.account_handoff.projection_is_source);

    // Agent-facing convention files live on the governance source path rather than
    // being folded into authored Project Ground by their filename.
    assert!(!ground
        .native_candidates
        .iter()
        .any(|candidate| candidate.path == "AGENTS.md"));

    let governance = inspect_project_governance(&project).unwrap();
    let agents = governance
        .native_candidates
        .iter()
        .find(|candidate| candidate.path == "AGENTS.md")
        .expect("AGENTS.md is a governance candidate");
    assert_eq!(agents.provenance, "unresolved");
    assert_eq!(agents.source_role, "possible-project-agent-governance");

    assert_eq!(governance.composition.operational_resolution_owner, "AIKit");
    assert!(!governance
        .composition
        .operational_precedence_defined_by_central);
}
