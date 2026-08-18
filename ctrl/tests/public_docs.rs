use central_connector_sdk::{
    CONFIGURATION_MANAGER_PORT, MACHINE_INSPECTOR_PORT, NATIVE_OPEN_PORT, NATIVE_REVEAL_PORT,
    PACKAGE_MANAGER_PORT, SERVICE_MANAGER_PORT, SYNCHRONIZER_PORT, TAG_STORE_PORT,
    WORK_DISCOVERY_PORT,
};
use central_ctrl::create_core_action_registry;
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

#[test]
fn docs_front_door_indexes_every_document() {
    let docs = repository_root().join("docs");
    let front_door = fs::read_to_string(docs.join("README.md")).unwrap();
    let mut documents = fs::read_dir(&docs)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".md") && name != "README.md")
        .collect::<Vec<_>>();
    documents.sort();
    assert!(!documents.is_empty(), "docs corpus must not be empty");
    for name in documents {
        assert!(
            front_door.contains(&name),
            "docs front door does not index {name}"
        );
    }
}

#[test]
fn root_readme_points_at_the_docs_front_door() {
    let readme = fs::read_to_string(repository_root().join("README.md")).unwrap();
    assert!(
        readme.contains("docs/README.md"),
        "root README must route readers to the docs front door"
    );
}
