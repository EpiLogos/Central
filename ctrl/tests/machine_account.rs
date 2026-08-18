use central_ctrl::{
    create_core_action_registry, initialize_central, run_cli, ActionExecutionContext,
    CliEnvironment, ConnectorContext, ConnectorRegistry, MachineInspectionOutput,
    ObservedConfiguration, ObservedPackage, ObservedService, ResultStatus, RootOptions,
    StaticMachineInspectorConnector,
};
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let path = std::env::temp_dir().join(format!("central-account-{label}-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

fn write_role(root: &PathBuf, role: &str, capabilities: &[&str]) {
    let declaration = json!({
        "schema": "central.machine",
        "version": 1,
        "role": role,
        "capabilities": capabilities,
        "requirements": {
            "packages": [{ "id": "git", "state": "present" }],
            "configurations": [{ "id": "remote-access-policy", "state": "present" }],
            "services": [{ "id": "ssh", "running": true, "enabled": true }]
        }
    });
    fs::write(
        root.join("Control/machines").join(format!("{role}.json")),
        serde_json::to_string_pretty(&declaration).unwrap(),
    ).unwrap();
}

fn observation(capabilities: &[&str]) -> MachineInspectionOutput {
    MachineInspectionOutput {
        platform: "test-os".to_owned(),
        architecture: "test-arch".to_owned(),
        capabilities: capabilities.iter().map(|value| (*value).to_owned()).collect(),
        packages: vec![ObservedPackage { id: "git".to_owned(), present: true }],
        configurations: vec![ObservedConfiguration { id: "remote-access-policy".to_owned(), present: true }],
        services: vec![ObservedService { id: "ssh".to_owned(), present: true, running: true, enabled: true }],
    }
}

fn registry_with_observation(observation: MachineInspectionOutput) -> ConnectorRegistry {
    let mut connectors = ConnectorRegistry::default();
    connectors.register(StaticMachineInspectorConnector::new(observation)).unwrap();
    connectors
}

fn execute_account(connectors: &ConnectorRegistry, root: &PathBuf) -> central_ctrl::ActionResult {
    let registry = create_core_action_registry();
    let options = RootOptions { explicit_root: Some(root.clone()), ..RootOptions::default() };
    let connector_context = ConnectorContext { platform: "test".to_owned() };
    let context = ActionExecutionContext { root_options: &options, connectors, connector_context: &connector_context };
    registry.execute("machine.account", &json!({}), &context)
}

#[test]
fn identity_is_stable_and_persisted_under_derived_local_state() {
    let root = temporary_directory("identity").join("Central");
    initialize_central(&root).unwrap();

    let connectors = registry_with_observation(observation(&["remote-shell"]));
    let first = execute_account(&connectors, &root);
    assert_eq!(first.status, ResultStatus::Success);
    let second = execute_account(&connectors, &root);
    assert_eq!(second.status, ResultStatus::Success);

    let first_id = first.data.unwrap()["current"]["machine_id"].as_str().unwrap().to_owned();
    let second_id = second.data.unwrap()["current"]["machine_id"].as_str().unwrap().to_owned();
    assert_eq!(first_id, second_id, "machine identity must be stable across calls");

    let identity_file = root.join(".central/machines/identity.json");
    assert!(identity_file.is_file(), "identity must persist under .central derived state");
    let persisted: serde_json::Value = serde_json::from_str(&fs::read_to_string(&identity_file).unwrap()).unwrap();
    assert_eq!(persisted["machine_id"], json!(first_id));
}

#[test]
fn account_composes_observed_and_authored_without_writing_control() {
    let root = temporary_directory("compose").join("Central");
    initialize_central(&root).unwrap();
    write_role(&root, "home-server", &["remote-shell"]);

    let authored_path = root.join("Control/machines/home-server.json");
    let authored_before = fs::read_to_string(&authored_path).unwrap();

    let connectors = registry_with_observation(observation(&["remote-shell"]));
    let result = execute_account(&connectors, &root);
    assert_eq!(result.status, ResultStatus::Success);
    let data = result.data.unwrap();

    // Authored intent is read, never modified.
    assert_eq!(fs::read_to_string(&authored_path).unwrap(), authored_before, "observation must never author into Control");

    assert_eq!(data["current"]["hostname"].as_str(), Some("unknown-host"));
    assert_eq!(data["observation_stale"], json!(false));
    assert_eq!(data["provenance"], json!("observed"));
    assert_eq!(data["last_observation"]["connector_id"], json!("reference.machine-static"));

    let authored = data["authored"].as_array().unwrap();
    assert_eq!(authored.len(), 1);
    assert_eq!(authored[0]["role"], json!("home-server"));

    // Capability present -> no drift; requirements satisfied -> no drift.
    assert_eq!(data["drift"].as_array().unwrap().len(), 0);
    assert_eq!(data["reconciliation_available"], json!(false));
}

#[test]
fn drift_and_reconciliation_are_derived_from_observation() {
    let root = temporary_directory("drift").join("Central");
    initialize_central(&root).unwrap();
    // Authored intent expects remote-shell + git; observation lacks remote-shell.
    write_role(&root, "primary-workstation", &["remote-shell", "media-acceleration"]);

    let connectors = registry_with_observation(observation(&["media-acceleration"]));
    let result = execute_account(&connectors, &root);
    assert_eq!(result.status, ResultStatus::Success);
    let data = result.data.unwrap();

    let drift = data["drift"].as_array().unwrap();
    let missing = drift.iter().find(|entry| entry["id"] == "remote-shell").unwrap();
    assert_eq!(missing["kind"], json!("capability"));
    assert_eq!(missing["status"], json!("missing"));
    assert_eq!(missing["role"], json!("primary-workstation"));

    assert_eq!(data["reconciliation_available"], json!(true));
}

#[test]
fn observation_record_persists_under_central_for_stale_fallback() {
    let root = temporary_directory("persist").join("Central");
    initialize_central(&root).unwrap();

    // First account with a connector captures an observation record.
    let connectors = registry_with_observation(observation(&["remote-shell"]));
    let result = execute_account(&connectors, &root);
    assert_eq!(result.status, ResultStatus::Success);
    let machine_id = result.data.as_ref().unwrap()["current"]["machine_id"].as_str().unwrap().to_owned();
    let record_path = root.join(".central/machines/observed").join(format!("{machine_id}.json"));
    assert!(record_path.is_file(), "observation must persist under .central derived state");
    let record: serde_json::Value = serde_json::from_str(&fs::read_to_string(&record_path).unwrap()).unwrap();
    assert_eq!(record["machine_id"], json!(machine_id));
    assert!(record["observed_at"].as_str().unwrap_or_default().len() > 0);

    // Without any connector, the account falls back to the last capture and
    // reports staleness truthfully instead of inventing fresh observation.
    let empty = ConnectorRegistry::default();
    let fallback = execute_account(&empty, &root);
    assert_eq!(fallback.status, ResultStatus::Success);
    let data = fallback.data.unwrap();
    assert_eq!(data["observation_stale"], json!(true));
    assert_eq!(data["last_observation"]["machine_id"], json!(machine_id));
    assert_eq!(data["current"]["machine_id"], json!(machine_id), "machine remains recognisable from cache");
}

#[test]
fn cli_projects_machine_account() {
    let root = temporary_directory("cli").join("Central");
    initialize_central(&root).unwrap();
    write_role(&root, "home-server", &["remote-shell"]);

    let connectors = registry_with_observation(observation(&["remote-shell"]));
    let registry = create_core_action_registry();
    let options = RootOptions { explicit_root: Some(root.clone()), ..RootOptions::default() };
    let connector_context = ConnectorContext { platform: "test".to_owned() };
    let context = ActionExecutionContext { root_options: &options, connectors: &connectors, connector_context: &connector_context };
    let result = registry.execute("machine.account", &json!({}), &context);
    let output = central_ctrl::machine_account::explain_account(result.data.as_ref().unwrap());
    assert!(output.contains("Machine:"));
    assert!(output.contains("Observed: test-os/test-arch"));
    assert!(output.contains("Authored role: home-server"));
    assert!(output.contains("Drift: none"));
}
