use central_ctrl::{
    initialize_projectcentral, stamp_plan_for_project, stamp_plan_for_root, stamp_project,
    stamp_root, PROJECTCENTRAL_DIR,
};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const ROOT_PROTOCOL_DEFAULT: &str = "Control/agents/governance/repos/repo-content-and-structure.md";
const ROOT_PLACEMENT_STATEMENT: &str = "Control/agents/governance/field-and-now/session-work-placement.md";
const ROOT_DAY_CLOSE_STATEMENT: &str = "Control/agents/governance/field-and-now/day-close.md";
const ROOT_WIKI_LAW_STATEMENT: &str = "Control/agents/governance/field-and-now/wiki-field-law.md";
const ROOT_NOW_POLICY: &str = "Control/agents/now/policy.json";
const ROOT_NOW_PROMOTIONS: &str = "Control/agents/now/promotions.json";
const ROOT_NOW_README: &str = "Control/agents/now/README.md";
const ROOT_DEFAULT_PATHS: &[&str] = &[
    ROOT_PROTOCOL_DEFAULT,
    ROOT_PLACEMENT_STATEMENT,
    ROOT_DAY_CLOSE_STATEMENT,
    ROOT_WIKI_LAW_STATEMENT,
    ROOT_NOW_POLICY,
    ROOT_NOW_PROMOTIONS,
    ROOT_NOW_README,
];
const PROJECT_STRUCTURE_STARTER: &str = "ProjectCentral/agents/governance/repo-structure.md";
const PROJECT_CONTENT_STARTER: &str = "ProjectCentral/agents/governance/repo-content.md";

fn temporary_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "central-template-stamp-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn initialized_central(label: &str) -> PathBuf {
    let central = temporary_directory(label);
    // central.init creates the Control root; the stamp seam requires it.
    fs::create_dir_all(central.join("Control")).unwrap();
    central
}

fn assert_marked_draft(path: &Path) {
    let content = fs::read_to_string(path).unwrap();
    assert!(
        content.contains("distributed default"),
        "{path:?} should carry distributed-default standing"
    );
}

#[test]
fn root_scope_creates_protocol_default_and_is_idempotent() {
    let central = initialized_central("root-creates");

    let plan = stamp_plan_for_root(&central).unwrap();
    let planned: Vec<String> = plan.files.iter().map(|file| file.path.clone()).collect();
    assert_eq!(planned, ROOT_DEFAULT_PATHS.to_vec());

    let result = stamp_root(&central).unwrap();
    assert_eq!(result.created, ROOT_DEFAULT_PATHS.to_vec());
    assert!(result.skipped_existing.is_empty());
    assert_marked_draft(&central.join(ROOT_PROTOCOL_DEFAULT));

    let second = stamp_root(&central).unwrap();
    assert!(second.created.is_empty());
    assert_eq!(second.skipped_existing, ROOT_DEFAULT_PATHS.to_vec());
}

#[test]
fn root_scope_requires_an_initialized_control_root() {
    let central = temporary_directory("root-uninitialised");
    let error = stamp_root(&central).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidInput);
}

#[test]
fn root_scope_never_overwrites_human_source() {
    let central = initialized_central("root-no-clobber");
    for relative in ROOT_DEFAULT_PATHS {
        let live = central.join(relative);
        fs::create_dir_all(live.parent().unwrap()).unwrap();
        fs::write(&live, "human-authored words\n").unwrap();
    }

    let result = stamp_root(&central).unwrap();
    assert!(result.created.is_empty());
    assert_eq!(result.skipped_existing, ROOT_DEFAULT_PATHS.to_vec());
    for relative in ROOT_DEFAULT_PATHS {
        assert_eq!(
            fs::read_to_string(central.join(relative)).unwrap(),
            "human-authored words\n"
        );
    }
}

#[test]
fn root_scope_stamps_now_field_and_field_and_now_governance() {
    let central = initialized_central("root-now-field");
    let result = stamp_root(&central).unwrap();

    for relative in [
        ROOT_PLACEMENT_STATEMENT,
        ROOT_DAY_CLOSE_STATEMENT,
        ROOT_WIKI_LAW_STATEMENT,
        ROOT_NOW_README,
    ] {
        assert!(result.created.contains(&relative.to_owned()));
        assert_marked_draft(&central.join(relative));
    }

    let placement = fs::read_to_string(central.join(ROOT_PLACEMENT_STATEMENT)).unwrap();
    assert!(placement.contains("Work that has nowhere to land lands everywhere."));

    let day_close = fs::read_to_string(central.join(ROOT_DAY_CLOSE_STATEMENT)).unwrap();
    assert!(day_close.contains(
        "a partial close that names its failure beats a clean claim that lied"
    ));

    let wiki_law = fs::read_to_string(central.join(ROOT_WIKI_LAW_STATEMENT)).unwrap();
    assert!(wiki_law.contains("A wiki is agent-maintained knowledge, never source."));

    let policy = fs::read_to_string(central.join(ROOT_NOW_POLICY)).unwrap();
    assert!(policy.contains("\"schema\": \"central.project-now.policy/v1\""));
    assert!(policy.contains("\"human_scratch_cleanup\": \"human-owned-manual\""));

    let promotions = fs::read_to_string(central.join(ROOT_NOW_PROMOTIONS)).unwrap();
    assert!(promotions.contains("\"schema\": \"central.project-now.promotions/v1\""));
    assert!(promotions.contains("\"entries\": []"));

    let readme = fs::read_to_string(central.join(ROOT_NOW_README)).unwrap();
    assert!(readme.contains("Control/agents/now/"));
    assert!(readme.contains("O:I guardian projection"));
    // Dated session content never rides the template.
    assert!(!readme.contains("Current field state at establishment"));
}

#[test]
fn root_scope_stamps_nothing_under_control_user() {
    let central = initialized_central("root-user-aperture");

    stamp_root(&central).unwrap();

    // Control/user/** is human-authored ground; the stamp adds nothing there.
    assert!(!central.join("Control").join("user").exists());
}

#[test]
fn project_scope_creates_starters_after_projectcentral_init() {
    let central = initialized_central("project-creates");
    let project = central.join("Work/product");
    fs::create_dir_all(&project).unwrap();
    initialize_projectcentral(&central, &project, "example/product").unwrap();

    let plan = stamp_plan_for_project(&project).unwrap();
    let planned: Vec<String> = plan.files.iter().map(|file| file.path.clone()).collect();
    assert_eq!(
        planned,
        vec![
            PROJECT_STRUCTURE_STARTER.to_owned(),
            PROJECT_CONTENT_STARTER.to_owned(),
        ]
    );

    let result = stamp_project(&project).unwrap();
    assert_eq!(result.created.len(), 2);
    assert!(result.skipped_existing.is_empty());
    assert_marked_draft(&project.join(PROJECT_STRUCTURE_STARTER));
    assert_marked_draft(&project.join(PROJECT_CONTENT_STARTER));

    // The stamp adds nothing to the human authorship aperture.
    assert!(!project
        .join(PROJECTCENTRAL_DIR)
        .join("user")
        .join("README.md")
        .exists());

    let second = stamp_project(&project).unwrap();
    assert!(second.created.is_empty());
    assert_eq!(second.skipped_existing.len(), 2);
}

#[test]
fn project_scope_requires_projectcentral_identity() {
    let central = initialized_central("project-unbound");
    let project = central.join("Work/unbound");
    fs::create_dir_all(&project).unwrap();

    let error = stamp_project(&project).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidInput);
}

#[test]
fn project_scope_never_overwrites_project_governance() {
    let central = initialized_central("project-no-clobber");
    let project = central.join("Work/product");
    fs::create_dir_all(&project).unwrap();
    initialize_projectcentral(&central, &project, "example/product").unwrap();

    let live = project.join(PROJECT_STRUCTURE_STARTER);
    fs::create_dir_all(live.parent().unwrap()).unwrap();
    fs::write(&live, "this repo's own structure law\n").unwrap();

    let result = stamp_project(&project).unwrap();
    assert_eq!(result.created, vec![PROJECT_CONTENT_STARTER.to_owned()]);
    assert_eq!(
        result.skipped_existing,
        vec![PROJECT_STRUCTURE_STARTER.to_owned()]
    );
    assert_eq!(
        fs::read_to_string(&live).unwrap(),
        "this repo's own structure law\n"
    );
}

#[test]
fn stamp_actions_are_registered_with_honest_mutation_classes() {
    let mut registry = central_ctrl::create_core_action_registry();
    central_ctrl::register_template_stamp_actions(&mut registry);

    let preview = registry.get("central.template.preview").expect("preview registered");
    assert_eq!(preview.mutation_class, central_ctrl::MutationClass::ReadOnly);
    let stamp = registry.get("central.template.stamp").expect("stamp registered");
    assert_eq!(
        stamp.mutation_class,
        central_ctrl::MutationClass::LocallyMutating
    );
    assert!(stamp.preview_supported);
}
