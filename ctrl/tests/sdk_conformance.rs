use central_ctrl::reference_connectors::{FilesystemWorkDiscovery, StaticWorkDiscovery, filesystem_work_discovery_metadata, static_work_discovery_metadata};
use central_ctrl::sdk::{PortCompatibility, WorkItem, WORK_DISCOVERY_PORT_ID, conformance};
use tempfile::tempdir;

#[test]
fn both_reference_connectors_pass_public_conformance() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("Central");
    std::fs::create_dir_all(root.join("Work/alpha")).unwrap();
    let first = conformance::work_discovery(&filesystem_work_discovery_metadata(), &FilesystemWorkDiscovery, &root);
    assert!(first.passed, "{:?}", first.failures);
    let second_connector = StaticWorkDiscovery::new(vec![WorkItem { name: "alpha".into(), path: root.join("Work/alpha").display().to_string() }]);
    let second = conformance::work_discovery(&static_work_discovery_metadata(), &second_connector, &root);
    assert!(second.passed, "{:?}", second.failures);
}

#[test]
fn incompatible_contract_fails_conformance() {
    let temp = tempdir().unwrap();
    let mut metadata = static_work_discovery_metadata();
    metadata.implemented_ports = vec![PortCompatibility { id: WORK_DISCOVERY_PORT_ID.into(), contract: "WorkDiscovery/v0".into() }];
    let connector = StaticWorkDiscovery::new(Vec::new());
    let report = conformance::work_discovery(&metadata, &connector, temp.path());
    assert!(!report.passed);
    assert!(report.failures.iter().any(|failure| failure.contains("WorkDiscovery/v1")));
}

#[test]
fn metadata_validation_is_reusable() {
    let mut metadata = static_work_discovery_metadata();
    metadata.id.clear();
    metadata.entrypoint.clear();
    let failures = conformance::metadata_failures(&metadata);
    assert!(failures.iter().any(|failure| failure.contains("id")));
    assert!(failures.iter().any(|failure| failure.contains("entrypoint")));
}
