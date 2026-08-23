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
        "central-ubuntu-recovery-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn run(
    root: &Path,
    connectors: &ConnectorRegistry,
    command: &[&str],
) -> central_ctrl::CliExecution {
    let mut args = vec![
        "--json".to_owned(),
        "--root".to_owned(),
        root.to_string_lossy().into_owned(),
    ];
    args.extend(command.iter().map(|value| (*value).to_owned()));
    let mut surface = NullTerminalSurface;
    let connector_context = ConnectorContext { platform: "linux".to_owned() };
    run_cli_with_runtime(
        &args,
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
fn canonical_recovery_reuses_the_real_ubuntu_reconciliation_connectors() {
    let fixture = temporary_directory("provider-proof");
    let root = fixture.join("Central");
    initialize_central(&root).unwrap();

    let source = fixture.join("authored/recovery.conf");
    let target = fixture.join("observed/recovery.conf");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(&source, "central_recovery_provider_proof=1\n").unwrap();

    let declaration = json!({
        "schema": "central.machine",
        "version": 1,
        "role": "home-server",
        "capabilities": [],
        "requirements": {
            "packages": [{ "id": "bash", "state": "present" }],
            "configurations": [{
                "id": target.to_string_lossy(),
                "state": "present",
                "source": {
                    "kind": "file",
                    "reference": source.to_string_lossy()
                }
            }],
            "services": []
        }
    });
    fs::write(
        root.join("Control/machines/home-server.json"),
        serde_json::to_string_pretty(&declaration).unwrap(),
    )
    .unwrap();

    let connectors = ubuntu_registry();
    let first = run(&root, &connectors, &["recover", "home-server"]);
    assert_eq!(first.result.status, ResultStatus::Success);
    let data = first.result.data.as_ref().unwrap();
    assert_eq!(data["outcome"], "complete");
    assert_eq!(
        data["initial_plan"]["synchronization"]["status"],
        "not_configured"
    );
    assert_eq!(
        data["initial_plan"]["machine"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["kind"] == "configuration")
            .unwrap()["connector"]["id"],
        "personal.ubuntu-server"
    );
    assert_eq!(
        data["machine_apply"]["operations"][0]["connector"]["id"],
        "personal.ubuntu-server"
    );
    assert_eq!(data["verification"]["satisfied"], true);
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        "central_recovery_provider_proof=1\n"
    );

    let repeated = run(&root, &connectors, &["recover", "home-server"]);
    assert_eq!(repeated.result.status, ResultStatus::Success);
    let repeated_data = repeated.result.data.as_ref().unwrap();
    assert_eq!(repeated_data["outcome"], "complete");
    assert_eq!(
        repeated_data["machine_apply"]["operations"]
            .as_array()
            .unwrap()
            .len(),
        0
    );

    let actions = run(&root, &connectors, &["action", "list"]);
    assert_eq!(actions.result.status, ResultStatus::Success);
    let catalog = actions.result.data.as_ref().unwrap()["actions"]
        .as_array()
        .unwrap();
    for id in ["central.recovery.plan", "central.recover"] {
        assert!(catalog.iter().any(|action| action["id"] == id));
    }

    fs::remove_dir_all(fixture).unwrap();
}
