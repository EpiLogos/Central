use central_ctrl::{
    create_core_action_registry, CONFIGURATION_MANAGER_PORT, MACHINE_INSPECTOR_PORT,
    NATIVE_OPEN_PORT, NATIVE_REVEAL_PORT, PACKAGE_MANAGER_PORT, SERVICE_MANAGER_PORT,
    SYNCHRONIZER_PORT, TAG_STORE_PORT, WORK_DISCOVERY_PORT,
};
use std::fs;
use std::path::PathBuf;

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

#[test]
fn cli_reference_names_every_registered_core_action_and_complete_invocation_seam() {
    let documentation = fs::read_to_string(repository_root().join("docs/CLI-REFERENCE.md")).unwrap();
    assert!(documentation.contains("action run <action-id>"));
    assert!(documentation.contains("--json"));
    assert!(documentation.contains("CENTRAL_ROOT"));

    for action in create_core_action_registry().list() {
        assert!(
            documentation.contains(&format!("`{}`", action.id)),
            "CLI reference is missing canonical Action {}",
            action.id
        );
    }
}

#[test]
fn rust_sdk_reference_names_every_published_core_port_and_version() {
    let documentation = fs::read_to_string(repository_root().join("docs/CONNECTOR-SDK-RUST.md")).unwrap();
    let ports = [
        WORK_DISCOVERY_PORT,
        NATIVE_OPEN_PORT,
        NATIVE_REVEAL_PORT,
        TAG_STORE_PORT,
        MACHINE_INSPECTOR_PORT,
        PACKAGE_MANAGER_PORT,
        CONFIGURATION_MANAGER_PORT,
        SERVICE_MANAGER_PORT,
        SYNCHRONIZER_PORT,
    ];

    for port in ports {
        assert!(
            documentation.contains(&format!("`{}`", port.id)),
            "SDK reference is missing public Port {}",
            port.id
        );
        assert!(
            documentation.contains(port.version),
            "SDK reference is missing version {} for {}",
            port.version,
            port.id
        );
    }

    assert!(documentation.contains("PortErrorCode"));
    assert!(documentation.contains("ConnectorRegistry"));
    assert!(documentation.contains("run_synchronizer_conformance"));
}
