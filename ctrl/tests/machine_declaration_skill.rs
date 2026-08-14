use central_ctrl::{
    create_core_action_registry, initialize_central, ActionExecutionContext, ConnectorContext,
    ConnectorRegistry, InMemoryMachineConnector, MachineInspectionOutput, ResultStatus, RootOptions,
    StaticMachineInspectorConnector,
};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const SKILL: &str = include_str!("../../skills/machine-declaration/SKILL.md");
const WORKSTATION_FIXTURE: &str =
    include_str!("../../skills/machine-declaration/fixtures/primary-workstation.json");
const SERVER_FIXTURE: &str =
    include_str!("../../skills/machine-declaration/fixtures/home-server.json");

fn temporary_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let path = std::env::temp_dir().join(format!(
        "central-machine-declaration-skill-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn prepare_root(label: &str, fixture: &Value) -> PathBuf {
    let root = temporary_directory(label).join("Central");
    initialize_central(&root).unwrap();
    let role = fixture["role"].as_str().unwrap();
    fs::write(
        root.join("Control/machines").join(format!("{role}.json")),
        serde_json::to_string_pretty(&fixture["declaration"]).unwrap(),
    )
    .unwrap();
    root
}

fn context<'a>(
    root_options: &'a RootOptions,
    connectors: &'a ConnectorRegistry,
    connector_context: &'a ConnectorContext,
) -> ActionExecutionContext<'a> {
    ActionExecutionContext {
        root_options,
        connectors,
        connector_context,
    }
}

fn observation(fixture: &Value) -> MachineInspectionOutput {
    serde_json::from_value(fixture["observation"].clone()).unwrap()
}

fn plan_entry<'a>(plan: &'a Value, expected: &Value) -> &'a Value {
    plan["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| {
            entry["kind"] == expected["kind"] && entry["id"] == expected["id"]
        })
        .expect("fixture requirement must appear in machine plan")
}

#[test]
fn skill_preserves_authorship_observation_ports_and_extension_handoffs() {
    for required in [
        "read the existing machine-role source before proposing changes",
        "A. Existing authored intent",
        "B. Current observation",
        "C. Proposed authored intent",
        "Never copy B wholesale into C",
        "MachineInspector",
        "PackageManager",
        "ConfigurationManager",
        "ServiceManager",
        "Connector coverage",
        "skills/connector-authoring/SKILL.md",
        "explicit acceptance",
        "Final diff",
        "Observation is not intent",
        "Intent is not provider choice",
        "Missing capability is not permission to smuggle implementation into Control",
        "primary-workstation",
        "home-server",
    ] {
        assert!(SKILL.contains(required), "Machine-declaration Skill is missing: {required}");
    }
}

#[test]
fn workstation_fixture_keeps_authored_intent_separate_and_identifies_an_eligible_package_connector() {
    let fixture: Value = serde_json::from_str(WORKSTATION_FIXTURE).unwrap();
    let root = prepare_root("workstation", &fixture);
    let original = fs::read_to_string(root.join("Control/machines/primary-workstation.json")).unwrap();

    let mut connectors = ConnectorRegistry::default();
    connectors
        .register(InMemoryMachineConnector::new(observation(&fixture)))
        .unwrap();
    let connector_context = ConnectorContext { platform: "fixture-os".to_owned() };
    let root_options = RootOptions {
        explicit_root: Some(root.clone()),
        ..RootOptions::default()
    };
    let execution_context = context(&root_options, &connectors, &connector_context);
    let actions = create_core_action_registry();

    let declaration = actions.execute(
        "machine.declaration",
        &json!({ "role": fixture["role"] }),
        &execution_context,
    );
    assert_eq!(declaration.status, ResultStatus::Success);
    let declaration_data = declaration.data.unwrap();
    assert_eq!(declaration_data["declaration"], fixture["declaration"]);
    assert_eq!(
        declaration_data["source"]["path"],
        "Control/machines/primary-workstation.json"
    );

    let inspection = actions.execute("machine.inspect", &json!({}), &execution_context);
    assert_eq!(inspection.status, ResultStatus::Success);
    let inspection_data = inspection.data.unwrap();
    assert_eq!(inspection_data["observation"], fixture["observation"]);
    assert!(inspection_data.get("declaration").is_none());

    let plan = actions.execute(
        "machine.plan",
        &json!({ "role": fixture["role"] }),
        &execution_context,
    );
    assert_eq!(plan.status, ResultStatus::Success);
    let plan_data = plan.data.unwrap();
    assert_eq!(plan_data["authored"]["declaration"], fixture["declaration"]);
    assert_eq!(plan_data["observed"]["observation"], fixture["observation"]);

    let expected = &fixture["expected"];
    let entry = plan_entry(&plan_data, expected);
    assert_eq!(entry["status"], expected["status"]);
    assert_eq!(entry["port"], expected["port"]);
    assert_eq!(entry["connector"]["id"], expected["selected_connector"]);
    assert_eq!(plan_data["summary"]["changeable"], 1);
    assert_eq!(plan_data["summary"]["missing"], 0);

    let after = fs::read_to_string(root.join("Control/machines/primary-workstation.json")).unwrap();
    assert_eq!(after, original, "inspection/planning must not mutate authored source");
    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn server_fixture_preserves_intent_when_configuration_port_is_missing_and_exposes_handoff_evidence() {
    let fixture: Value = serde_json::from_str(SERVER_FIXTURE).unwrap();
    let root = prepare_root("server", &fixture);
    let original = fs::read_to_string(root.join("Control/machines/home-server.json")).unwrap();

    let mut connectors = ConnectorRegistry::default();
    connectors
        .register(StaticMachineInspectorConnector::new(observation(&fixture)))
        .unwrap();
    let connector_context = ConnectorContext { platform: "fixture-os".to_owned() };
    let root_options = RootOptions {
        explicit_root: Some(root.clone()),
        ..RootOptions::default()
    };
    let execution_context = context(&root_options, &connectors, &connector_context);
    let actions = create_core_action_registry();

    let plan = actions.execute(
        "machine.plan",
        &json!({ "role": fixture["role"] }),
        &execution_context,
    );
    assert_eq!(plan.status, ResultStatus::Success);
    let plan_data = plan.data.unwrap();
    assert_eq!(plan_data["authored"]["declaration"], fixture["declaration"]);
    assert_eq!(plan_data["observed"]["observation"], fixture["observation"]);

    let expected = &fixture["expected"];
    let entry = plan_entry(&plan_data, expected);
    assert_eq!(entry["status"], expected["status"]);
    assert_eq!(entry["port"], expected["port"]);
    assert!(entry["connector"].is_null());
    assert_eq!(plan_data["summary"]["missing"], 1);
    assert_eq!(plan_data["summary"]["changeable"], 0);
    assert!(entry["reason"]
        .as_str()
        .unwrap()
        .contains("no eligible ConfigurationManager Connector is available"));
    assert_eq!(expected["handoff_skill"], "connector-authoring");
    assert!(SKILL.contains("preserve the intended configuration requirement and create a Connector-authoring handoff"));

    let after = fs::read_to_string(root.join("Control/machines/home-server.json")).unwrap();
    assert_eq!(after, original, "missing implementation must not rewrite authored intent");
    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}
