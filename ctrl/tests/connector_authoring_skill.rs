use central_connector_sdk::{
    run_work_discovery_conformance, ConnectorContext, ConnectorRegistry,
    WorkDiscoveryConformanceFixture, WORK_DISCOVERY_PORT,
};
use central_ctrl::{
    create_core_action_registry, initialize_central, ActionExecutionContext,
    FilesystemWorkConnector, ResultStatus, RootOptions,
};
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const SKILL: &str = include_str!("../../skills/connector-authoring/SKILL.md");

fn temporary_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let path = std::env::temp_dir().join(format!(
        "central-connector-skill-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn skill_requires_the_public_contract_and_correct_architecture_boundary() {
    for required in [
        "Action → Port → public SDK → Connector → target technology",
        "published Port contract as authoritative",
        "central-connector-sdk",
        "Do not begin from a provider implementation",
        "Shared conformance is part of the Port contract",
        "Do not mutate the host while probing",
        "optional platform extension must not become a hidden dependency of core",
        "off-platform",
        "real-target acceptance",
        "macOS hardening lessons carried forward",
    ] {
        assert!(SKILL.contains(required), "Connector-authoring Skill is missing: {required}");
    }

    assert!(SKILL.contains("connectors/template"));
    assert!(SKILL.contains("FilesystemWorkConnector"));
    assert!(SKILL.contains("run_work_discovery_conformance"));
    assert!(SKILL.contains("ConnectorRegistry"));
    assert!(SKILL.contains("work.list"));
}

#[test]
fn reference_proof_runs_public_conformance_registry_resolution_and_canonical_action() {
    let root = temporary_directory("reference").join("Central");
    initialize_central(&root).unwrap();
    fs::create_dir(root.join("Work/alpha")).unwrap();
    fs::create_dir(root.join("Work/beta")).unwrap();
    fs::write(root.join("Work/not-a-project.txt"), "ordinary file").unwrap();

    let connector = FilesystemWorkConnector::new();
    let report = run_work_discovery_conformance(
        &connector,
        &WorkDiscoveryConformanceFixture {
            work_root: root.join("Work"),
            platform: std::env::consts::OS.to_owned(),
            expected_names: Some(vec!["alpha".to_owned(), "beta".to_owned()]),
        },
    )
    .unwrap();
    assert_eq!(report.port_id, WORK_DISCOVERY_PORT.id);
    assert_eq!(report.connector.id, "reference.work-filesystem");

    let mut connectors = ConnectorRegistry::default();
    connectors.register(connector).unwrap();
    let connector_context = ConnectorContext {
        platform: std::env::consts::OS.to_owned(),
    };
    let root_options = RootOptions {
        explicit_root: Some(root),
        ..RootOptions::default()
    };
    let context = ActionExecutionContext {
        root_options: &root_options,
        connectors: &connectors,
        connector_context: &connector_context,
    };

    let result = create_core_action_registry().execute("work.list", &json!({}), &context);
    assert_eq!(result.status, ResultStatus::Success);
    let data = result.data.unwrap();
    assert_eq!(
        data["diagnostics"]["selected_connector"]["id"],
        "reference.work-filesystem"
    );
    let items = data["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["name"], "alpha");
    assert_eq!(items[1]["name"], "beta");
}
