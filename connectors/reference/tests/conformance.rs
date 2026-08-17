use central_connector_sdk::{
    run_configuration_manager_conformance, run_machine_inspector_conformance,
    run_package_manager_conformance, run_service_manager_conformance,
    run_work_discovery_conformance, ConfigurationManagerConformanceFixture,
    ConfigurationStateRequest, MachineInspectionOutput, MachineInspectorConformanceFixture,
    ObservedConfiguration, ObservedPackage, ObservedService, PackageManagerConformanceFixture,
    PackageStateRequest, ServiceManagerConformanceFixture, ServiceStateRequest,
    WorkDiscoveryConformanceFixture, WorkItem,
};
use central_reference_connectors::{
    FilesystemWorkConnector, InMemoryMachineConnector, StaticMachineInspectorConnector,
    StaticWorkConnector,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let path = std::env::temp_dir().join(format!("central-sdk-{label}-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

fn machine_fixture() -> MachineInspectionOutput {
    MachineInspectionOutput {
        platform: "test-os".to_owned(),
        architecture: "test-arch".to_owned(),
        capabilities: vec!["remote-shell".to_owned()],
        packages: vec![ObservedPackage { id: "git".to_owned(), present: false }],
        configurations: vec![ObservedConfiguration {
            id: "remote-access-policy".to_owned(),
            present: false,
        }],
        services: vec![ObservedService {
            id: "ssh".to_owned(),
            present: true,
            running: false,
            enabled: false,
        }],
    }
}

#[test]
fn filesystem_reference_passes_public_work_discovery_conformance() {
    let work_root = temporary_directory("filesystem");
    fs::create_dir(work_root.join("beta")).unwrap();
    fs::create_dir(work_root.join("alpha")).unwrap();
    fs::write(work_root.join("ordinary-file.txt"), "not a directory").unwrap();
    let report = run_work_discovery_conformance(
        &FilesystemWorkConnector::new(),
        &WorkDiscoveryConformanceFixture {
            work_root,
            platform: std::env::consts::OS.to_owned(),
            expected_names: Some(vec!["alpha".to_owned(), "beta".to_owned()]),
        },
    ).unwrap();
    assert_eq!(report.port_id, "WorkDiscovery");
    assert_eq!(report.connector.id, "reference.work-filesystem");
}

#[test]
fn static_reference_passes_the_same_public_conformance_suite() {
    let connector = StaticWorkConnector::new(vec![WorkItem {
        name: "fixture".to_owned(),
        path: PathBuf::from("fixture"),
    }]);
    let report = run_work_discovery_conformance(
        &connector,
        &WorkDiscoveryConformanceFixture {
            work_root: PathBuf::from("ignored"),
            platform: std::env::consts::OS.to_owned(),
            expected_names: Some(vec!["fixture".to_owned()]),
        },
    ).unwrap();
    assert_eq!(report.connector.id, "reference.work-static");
}

#[test]
fn static_machine_inspector_passes_public_conformance_suite() {
    let observation = MachineInspectionOutput {
        platform: "test-os".to_owned(),
        architecture: "test-arch".to_owned(),
        capabilities: vec!["remote-shell".to_owned()],
        packages: vec![ObservedPackage { id: "git".to_owned(), present: true }],
        configurations: Vec::new(),
        services: Vec::new(),
    };
    let connector = StaticMachineInspectorConnector::new(observation.clone());
    let report = run_machine_inspector_conformance(
        &connector,
        &MachineInspectorConformanceFixture {
            platform: "test-os".to_owned(),
            expected: Some(observation),
        },
    ).unwrap();
    assert_eq!(report.port_id, "MachineInspector");
    assert_eq!(report.connector.id, "reference.machine-static");
}

#[test]
fn package_manager_reference_proves_preview_apply_and_idempotence() {
    let connector = InMemoryMachineConnector::new(machine_fixture());
    let report = run_package_manager_conformance(
        &connector,
        &PackageManagerConformanceFixture {
            platform: "test-os".to_owned(),
            request: PackageStateRequest { id: "git".to_owned(), present: true, source: None },
        },
    ).unwrap();
    assert_eq!(report.port_id, "PackageManager");
    assert_eq!(report.connector.id, "reference.machine-reconciler");
}

#[test]
fn configuration_manager_reference_proves_preview_apply_and_idempotence() {
    let connector = InMemoryMachineConnector::new(machine_fixture());
    let report = run_configuration_manager_conformance(
        &connector,
        &ConfigurationManagerConformanceFixture {
            platform: "test-os".to_owned(),
            request: ConfigurationStateRequest {
                id: "remote-access-policy".to_owned(),
                present: true,
                source: None,
            },
        },
    ).unwrap();
    assert_eq!(report.port_id, "ConfigurationManager");
    assert_eq!(report.connector.id, "reference.machine-reconciler");
}

#[test]
fn service_manager_reference_proves_preview_apply_and_idempotence() {
    let connector = InMemoryMachineConnector::new(machine_fixture());
    let report = run_service_manager_conformance(
        &connector,
        &ServiceManagerConformanceFixture {
            platform: "test-os".to_owned(),
            request: ServiceStateRequest {
                id: "ssh".to_owned(),
                running: Some(true),
                enabled: Some(true),
                source: None,
            },
        },
    ).unwrap();
    assert_eq!(report.port_id, "ServiceManager");
    assert_eq!(report.connector.id, "reference.machine-reconciler");
}
