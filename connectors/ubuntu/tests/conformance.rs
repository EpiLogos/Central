use central_connector_sdk::{
    run_configuration_manager_conformance, run_machine_inspector_conformance,
    run_package_manager_conformance, ConfigurationManagerConformanceFixture,
    ConfigurationStateRequest, Connector, ConnectorContext, MachineInspectionInput,
    MachineInspector, MachineInspectorConformanceFixture, PackageManagerConformanceFixture,
    PackageStateRequest, ReconciliationSourceReference, CONFIGURATION_MANAGER_PORT,
};
use central_ubuntu_connectors::UbuntuServerConnector;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let path = std::env::temp_dir().join(format!(
        "central-ubuntu-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn off_platform_probe_is_explicitly_unavailable() {
    let connector = UbuntuServerConnector::new();
    let probe = connector.probe(
        &CONFIGURATION_MANAGER_PORT,
        &ConnectorContext { platform: "macos".to_owned() },
    );
    assert!(!probe.available);
    assert!(probe.reason.unwrap().contains("does not support platform"));
}

#[cfg(target_os = "linux")]
#[test]
fn ubuntu_machine_inspector_passes_shared_conformance() {
    let connector = UbuntuServerConnector::new();
    let report = run_machine_inspector_conformance(
        &connector,
        &MachineInspectorConformanceFixture {
            platform: "linux".to_owned(),
            expected: None,
        },
    )
    .expect("Ubuntu MachineInspector should satisfy the public contract");
    assert_eq!(report.connector.id, "personal.ubuntu-server");
}

#[cfg(target_os = "linux")]
#[test]
fn ubuntu_package_manager_passes_shared_conformance_against_real_dpkg_state() {
    let connector = UbuntuServerConnector::new();
    let report = run_package_manager_conformance(
        &connector,
        &PackageManagerConformanceFixture {
            platform: "linux".to_owned(),
            request: PackageStateRequest {
                id: "bash".to_owned(),
                present: true,
                source: None,
            },
        },
    )
    .expect("bash is an installed Ubuntu base package and should exercise the real dpkg/apt Connector safely");
    assert_eq!(report.connector.id, "personal.ubuntu-server");
}

#[cfg(target_os = "linux")]
#[test]
fn ubuntu_configuration_manager_passes_shared_conformance_with_a_real_file_fixture() {
    let root = temporary_directory("configuration-conformance");
    let source = root.join("source.conf");
    let target = root.join("materialised/server.conf");
    fs::write(&source, "central_ubuntu_fixture=1\n").unwrap();

    let connector = UbuntuServerConnector::new();
    let report = run_configuration_manager_conformance(
        &connector,
        &ConfigurationManagerConformanceFixture {
            platform: "linux".to_owned(),
            request: ConfigurationStateRequest {
                id: target.to_string_lossy().into_owned(),
                present: true,
                source: Some(ReconciliationSourceReference {
                    kind: "file".to_owned(),
                    reference: source.to_string_lossy().into_owned(),
                }),
            },
        },
    )
    .expect("Ubuntu ConfigurationManager should satisfy the public contract");

    assert_eq!(report.connector.id, "personal.ubuntu-server");
    assert_eq!(fs::read_to_string(&target).unwrap(), "central_ubuntu_fixture=1\n");
    fs::remove_dir_all(root).unwrap();
}

#[cfg(target_os = "linux")]
#[test]
fn machine_inspection_reports_explicit_absence_for_requested_resources() {
    let root = temporary_directory("requested-observation");
    let missing = root.join("not-present.conf");
    let missing_id = missing.to_string_lossy().into_owned();
    let connector = UbuntuServerConnector::new();
    let observation = connector
        .inspect(&MachineInspectionInput {
            package_ids: vec!["bash".to_owned()],
            configuration_ids: vec![missing_id.clone()],
            service_ids: Vec::new(),
        })
        .unwrap();

    assert_eq!(observation.packages.len(), 1);
    assert_eq!(observation.packages[0].id, "bash");
    assert!(observation.packages[0].present);
    assert_eq!(observation.configurations.len(), 1);
    assert_eq!(observation.configurations[0].id, missing_id);
    assert!(!observation.configurations[0].present);
    fs::remove_dir_all(root).unwrap();
}
