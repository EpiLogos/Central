use central_ctrl::{
    create_core_action_registry, initialize_central, ActionExecutionContext, ConnectorContext,
    ConnectorRegistry, ResultStatus, RootOptions,
};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const SKILL: &str = include_str!("../../skills/control-maintenance/SKILL.md");
const FIXTURES: &str = include_str!("../../skills/control-maintenance/fixtures/audit-cases.json");

fn temporary_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let scratch = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ProjectCentral/now/tmp");
    fs::create_dir_all(&scratch).unwrap();
    let path = scratch.join(format!(
        "central-control-maintenance-skill-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&path).unwrap();
    path
}

fn fixture<'a>(fixtures: &'a Value, id: &str) -> &'a Value {
    fixtures["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["id"] == id)
        .expect("fixture case must exist")
}

#[test]
fn skill_encodes_control_ownership_audit_classification_and_acceptance_contract() {
    for required in [
        "human-owned authored source",
        "Direct filesystem source is authoritative",
        "stale",
        "duplicate",
        "conflicting",
        "low-value",
        "misplaced",
        "procedure-candidate",
        "missing-durable-area",
        "deletion and scope tests",
        "Skill",
        "Action",
        "verification and confidence",
        "optional topic",
        "tests, CI, review, evidence, or when human review is required",
        "exact test commands",
        "GitHub Actions workflow names/triggers",
        "project-local",
        "target, reason, supporting context, and final diff",
        "explicit acceptance before durable source mutation",
        "Generated audit advice is not authored truth",
        "Do not convert the tree into a mandatory universal profile schema",
    ] {
        assert!(
            SKILL.contains(required),
            "Control-maintenance Skill is missing: {required}"
        );
    }
}

#[test]
fn fixtures_cover_clean_stale_conflict_misplaced_procedure_and_verification_dialogue() {
    let fixtures: Value = serde_json::from_str(FIXTURES).unwrap();
    assert_eq!(fixtures["version"], 1);

    let expected = [
        ("clean-control-tree", "clean"),
        ("stale-content", "stale"),
        ("conflicting-content", "conflicting"),
        ("misplaced-procedure", "procedure-candidate"),
        ("verification-preference-dialogue", "missing-durable-area"),
    ];

    for (id, classification) in expected {
        let case = fixture(&fixtures, id);
        assert_eq!(
            case["expected_findings"][0]["classification"],
            classification
        );
        assert_eq!(case["mutation_expected_without_acceptance"], false);
        assert!(case["sources"]
            .as_array()
            .is_some_and(|sources| !sources.is_empty()));
    }

    let conflict = fixture(&fixtures, "conflicting-content");
    assert_eq!(conflict["sources"].as_array().unwrap().len(), 2);
    assert!(conflict["expected_findings"][0]["recommended_disposition"]
        .as_str()
        .unwrap()
        .contains("human resolution"));

    let misplaced = fixture(&fixtures, "misplaced-procedure");
    assert_eq!(misplaced["expected_findings"][0]["destination"], "Skill");
}

#[test]
fn verification_dialogue_retains_only_cross_project_preference_in_control() {
    let fixtures: Value = serde_json::from_str(FIXTURES).unwrap();
    let case = fixture(&fixtures, "verification-preference-dialogue");

    let question = case["dialogue"]["question"].as_str().unwrap();
    assert!(question.contains("tests, CI, review, evidence"));
    assert_eq!(
        case["expected_findings"][0]["topic"],
        "verification and confidence"
    );

    let proposal = case["expected_control_proposal"]["content"]
        .as_str()
        .unwrap();
    assert_eq!(
        case["expected_control_proposal"]["target"],
        "Control/agents/verification.md"
    );
    assert!(proposal.contains("appropriate executed evidence"));
    assert!(proposal.contains("existing assurance"));
    assert!(proposal.contains("consequential product or authorial decisions"));

    for excluded in case["project_scope_exclusions"].as_array().unwrap() {
        let excluded = excluded.as_str().unwrap();
        assert!(
            !proposal.contains(excluded),
            "project-specific mechanism leaked into Control proposal: {excluded}"
        );
    }
}

#[test]
fn fixture_source_is_ordinary_filesystem_control_and_audit_read_does_not_mutate_it() {
    let fixtures: Value = serde_json::from_str(FIXTURES).unwrap();
    let case = fixture(&fixtures, "clean-control-tree");
    let outer = temporary_directory("filesystem");
    struct Owned(PathBuf);
    impl Drop for Owned { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }
    let _owned = Owned(outer.clone());
    let root = outer.join("Central");
    initialize_central(&root).unwrap();

    for source in case["sources"].as_array().unwrap() {
        let path = root.join(source["path"].as_str().unwrap());
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, source["content"].as_str().unwrap()).unwrap();
    }

    let source_path = root.join("Control/agents/evidence.md");
    let before = fs::read_to_string(&source_path).unwrap();

    let registry = create_core_action_registry();
    let connectors = ConnectorRegistry::default();
    let connector_context = ConnectorContext {
        platform: "fixture-os".to_owned(),
    };
    let root_options = RootOptions {
        explicit_root: Some(root.clone()),
        ..RootOptions::default()
    };
    let context = ActionExecutionContext {
        root_options: &root_options,
        connectors: &connectors,
        connector_context: &connector_context,
    };

    let result = registry.execute(
        "control.search",
        &json!({ "query": "executed evidence" }),
        &context,
    );
    assert_eq!(result.status, ResultStatus::Success);
    let data = result.data.unwrap();
    assert_eq!(data["matches"].as_array().unwrap().len(), 1);
    assert_eq!(
        data["matches"][0]["source_path"],
        "Control/agents/evidence.md"
    );
    assert_eq!(data["matches"][0]["source_class"], "unresolved");
    assert!(data["matches"][0]["source_binding"].is_null());
    assert!(data["matches"][0]["source_revision"]["revision"].as_str().unwrap().starts_with("central.content-fnv1a64/v1:"));

    let after = fs::read_to_string(&source_path).unwrap();
    assert_eq!(
        after, before,
        "audit/read procedure must not mutate authored Control"
    );
    assert_eq!(fs::read_dir(root.join(".central")).unwrap().count(), 0);

}

#[test]
fn declared_human_legacy_file_is_owner_recognised_and_retained_in_place() {
    let outer = temporary_directory("declared-human");
    struct Owned(PathBuf);
    impl Drop for Owned { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }
    let _owned = Owned(outer.clone());
    let root = outer.join("Central");
    initialize_central(&root).unwrap();
    let path = "Control/agents/legacy-human.md";
    let source = root.join(path);
    fs::write(&source, b"declared-human-needle").unwrap();
    let relation_file = root.join(central_ctrl::CONTROL_GROUND_RELATIONS_SOURCE);
    fs::create_dir_all(relation_file.parent().unwrap()).unwrap();
    let relations = serde_json::to_vec(&json!({
        "schema":central_ctrl::CONTROL_GROUND_RELATIONS_SCHEMA,"project_id":"control:root",
        "relations":[{"ref":"central:source:control:root:legacy-human","path":path,
            "provenance":"human-authored","standing":"durable-source",
            "roles":["agent-governance-source"],"treatment":"retain-native-in-place"}]
    })).unwrap();
    fs::write(&relation_file, &relations).unwrap();
    fs::write(root.join("Control/agents/wiki/unselected.txt"), b"declared-human-needle").unwrap();
    let before = fs::read(&source).unwrap();
    let registry = create_core_action_registry();
    let connectors = ConnectorRegistry::default();
    let connector_context = ConnectorContext { platform:"fixture-os".to_owned() };
    let root_options = RootOptions { explicit_root:Some(root.clone()), ..RootOptions::default() };
    let context = ActionExecutionContext { root_options:&root_options, connectors:&connectors, connector_context:&connector_context };
    let owner = registry.execute("projectcentral.source.read",
        &json!({"source_ref":"central:source:control:root:legacy-human"}), &context);
    assert_eq!(owner.status, ResultStatus::Success, "{owner:?}");
    let owner = owner.data.unwrap();
    assert_eq!(owner["source"]["provenance"], "human-authored");
    let search = registry.execute("control.search", &json!({"query":"declared-human-needle"}), &context);
    assert_eq!(search.status, ResultStatus::Success, "{search:?}");
    let data = search.data.unwrap();
    assert_eq!(data["matches"].as_array().unwrap().len(), 1);
    assert_eq!(data["matches"][0]["source_path"], path);
    assert_eq!(data["matches"][0]["source_class"], "authored");
    assert_eq!(data["matches"][0]["source_binding"], owner["source"]);
    assert_eq!(data["matches"][0]["source_revision"]["revision"], owner["revision"]["revision"]);
    assert_eq!(fs::read(&source).unwrap(), before);
    assert_eq!(fs::read(&relation_file).unwrap(), relations);
}
