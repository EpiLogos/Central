use central_ctrl::{
    create_default_connector_registry, initialize_central, run_cli_with_runtime, CliEnvironment,
    ConnectorContext, ConnectorRegistry, NullTerminalSurface, ResultStatus,
};
use central_ubuntu_connectors::UbuntuServerConnector;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let path = std::env::temp_dir().join(format!(
        "central-ubuntu-acceptance-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn arguments(root: &Path, command: &[&str]) -> Vec<String> {
    let mut args = vec![
        "--json".to_owned(),
        "--root".to_owned(),
        root.to_string_lossy().into_owned(),
    ];
    args.extend(command.iter().map(|value| (*value).to_owned()));
    args
}

fn run(
    root: &Path,
    connectors: &ConnectorRegistry,
    command: &[&str],
) -> central_ctrl::CliExecution {
    let mut surface = NullTerminalSurface;
    let connector_context = ConnectorContext { platform: "linux".to_owned() };
    run_cli_with_runtime(
        &arguments(root, command),
        &CliEnvironment::default(),
        &mut surface,
        connectors,
        &connector_context,
    )
}

fn ubuntu_registry() -> ConnectorRegistry {
    let mut connectors = create_default_connector_registry();
    connectors.register(UbuntuServerConnector::new()).unwrap();
    connectors
}

#[cfg(target_os = "linux")]
#[test]
fn headless_ubuntu_can_root_control_plan_reconcile_verify_and_repeat() {
    let fixture = temporary_directory("lifecycle");
    let root = fixture.join("Central");
    initialize_central(&root).unwrap();

    let source = fixture.join("authored/server.conf");
    let target = fixture.join("observed/server.conf");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(&source, "central_home_server_fixture=1\n").unwrap();

    let declaration = json!({
        "schema": "central.machine",
        "version": 1,
        "role": "home-server",
        "capabilities": ["headless", "ubuntu-server"],
        "requirements": {
            "packages": [
                { "id": "bash", "state": "present" }
            ],
            "configurations": [
                {
                    "id": target.to_string_lossy(),
                    "state": "present",
                    "source": {
                        "kind": "file",
                        "reference": source.to_string_lossy()
                    }
                }
            ],
            "services": []
        }
    });
    fs::write(
        root.join("Control/machines/home-server.json"),
        serde_json::to_string_pretty(&declaration).unwrap(),
    )
    .unwrap();

    let connectors = ubuntu_registry();

    let root_result = run(&root, &connectors, &["root"]);
    assert_eq!(root_result.result.status, ResultStatus::Success);
    assert_eq!(root_result.result.data.as_ref().unwrap()["path"], root.to_string_lossy().as_ref());

    let control = run(&root, &connectors, &["control", "open", "machines"]);
    assert_eq!(control.result.status, ResultStatus::Success);
    assert_eq!(
        control.result.data.as_ref().unwrap()["source_class"],
        "authored"
    );

    let inspection = run(&root, &connectors, &["machine", "inspect"]);
    assert_eq!(inspection.result.status, ResultStatus::Success);
    let inspection_data = inspection.result.data.as_ref().unwrap();
    assert_eq!(
        inspection_data["source"]["connector"]["id"],
        "personal.ubuntu-server"
    );
    assert_eq!(inspection_data["observation"]["platform"], "linux");

    let plan = run(&root, &connectors, &["machine", "plan", "home-server"]);
    assert_eq!(plan.result.status, ResultStatus::Success);
    let plan_data = plan.result.data.as_ref().unwrap();
    assert_eq!(plan_data["summary"]["unsupported"], 0);
    assert_eq!(plan_data["summary"]["missing"], 0);
    assert_eq!(plan_data["summary"]["changeable"], 1);
    let config_entry = plan_data["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["kind"] == "configuration")
        .unwrap();
    assert_eq!(config_entry["status"], "changeable");
    assert_eq!(config_entry["port"], "ConfigurationManager");
    assert_eq!(
        config_entry["connector"]["id"],
        "personal.ubuntu-server"
    );

    let apply = run(&root, &connectors, &["machine", "apply", "home-server"]);
    assert_eq!(apply.result.status, ResultStatus::Success);
    let apply_data = apply.result.data.as_ref().unwrap();
    assert_eq!(apply_data["outcome"], "complete");
    assert_eq!(apply_data["operations"].as_array().unwrap().len(), 1);
    assert_eq!(apply_data["operations"][0]["port"], "ConfigurationManager");
    assert_eq!(apply_data["verification"]["satisfied"], true);
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        "central_home_server_fixture=1\n"
    );

    let verify = run(&root, &connectors, &["machine", "verify", "home-server"]);
    assert_eq!(verify.result.status, ResultStatus::Success);
    assert_eq!(verify.result.data.as_ref().unwrap()["satisfied"], true);

    let repeated = run(&root, &connectors, &["machine", "apply", "home-server"]);
    assert_eq!(repeated.result.status, ResultStatus::Success);
    let repeated_data = repeated.result.data.as_ref().unwrap();
    assert_eq!(repeated_data["outcome"], "complete");
    assert_eq!(repeated_data["operations"].as_array().unwrap().len(), 0);

    fs::remove_dir_all(fixture).unwrap();
}

#[cfg(target_os = "linux")]
#[test]
fn ubuntu_connector_changes_provider_resolution_not_core_action_identity() {
    let fixture = temporary_directory("action-identity");
    let root = fixture.join("Central");
    initialize_central(&root).unwrap();

    let reference = create_default_connector_registry();
    let ubuntu = ubuntu_registry();
    let reference_actions = run(&root, &reference, &["action", "list"]);
    let ubuntu_actions = run(&root, &ubuntu, &["action", "list"]);

    assert_eq!(reference_actions.result.status, ResultStatus::Success);
    assert_eq!(ubuntu_actions.result.status, ResultStatus::Success);
    assert_eq!(reference_actions.result.data, ubuntu_actions.result.data);

    let actions = ubuntu_actions.result.data.as_ref().unwrap()["actions"]
        .as_array()
        .unwrap();
    for id in [
        "machine.declaration",
        "machine.inspect",
        "machine.plan",
        "machine.apply",
        "machine.verify",
    ] {
        assert!(actions.iter().any(|action| action["id"] == id));
    }

    fs::remove_dir_all(fixture).unwrap();
}
