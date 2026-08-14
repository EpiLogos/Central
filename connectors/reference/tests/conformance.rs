use central_connector_sdk::{
    run_machine_inspector_conformance, run_work_discovery_conformance, MachineInspectionOutput,
    MachineInspectorConformanceFixture, ObservedPackage, WorkDiscoveryConformanceFixture, WorkItem,
};
use central_reference_connectors::{
    FilesystemWorkConnector, StaticMachineInspectorConnector, StaticWorkConnector,
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
