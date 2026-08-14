use central_ctrl::{
    connector::ConnectorRegistry,
    port::{CapabilityProbe, WorkDiscovery, WorkDiscoveryError, WorkDiscoveryErrorKind, WorkItem},
    reference_connectors::{StaticWorkDiscovery, static_work_discovery_metadata},
    result::ResultStatus,
    work,
};
use std::path::Path;
use tempfile::tempdir;

#[test]
fn unavailable_work_discovery_is_structured() {
    let temp = tempdir().unwrap();
    let result = work::list(&ConnectorRegistry::new(), "linux", temp.path());
    assert_eq!(result.status, ResultStatus::UnavailableCapability);
    let error = result.error.expect("structured error");
    assert_eq!(
        serde_json::to_value(error.code).unwrap(),
        serde_json::json!("unavailable_capability")
    );
    assert_eq!(
        result.data.unwrap()["diagnostics"]["selected_connector"],
        serde_json::Value::Null
    );
}

#[test]
fn ineligible_connector_is_explained() {
    let mut metadata = static_work_discovery_metadata();
    metadata.id = "reference.darwin-only".into();
    metadata.supported_environments = vec!["macos".into()];

    let mut registry = ConnectorRegistry::new();
    registry.register_work_discovery(metadata, StaticWorkDiscovery::new(Vec::new()));
    let resolution = registry.resolve_work_discovery("linux");

    assert!(resolution.selected.is_none());
    assert_eq!(resolution.diagnostics.ineligible_connectors.len(), 1);
    assert!(resolution.diagnostics.ineligible_connectors[0].reasons[0].contains("linux"));
}

struct FailingWorkDiscovery;

impl WorkDiscovery for FailingWorkDiscovery {
    fn probe(&self) -> CapabilityProbe {
        CapabilityProbe::available()
    }

    fn list(&self, _central_root: &Path) -> Result<Vec<WorkItem>, WorkDiscoveryError> {
        Err(WorkDiscoveryError::new(
            WorkDiscoveryErrorKind::ProviderOperationFailed,
            "controlled reference failure",
        ))
    }
}

#[test]
fn connector_operation_failure_is_structured() {
    let mut metadata = static_work_discovery_metadata();
    metadata.id = "reference.failing-work-discovery".into();
    let mut registry = ConnectorRegistry::new();
    registry.register_work_discovery(metadata, FailingWorkDiscovery);

    let result = work::list(&registry, "linux", Path::new("/unused"));
    assert_eq!(result.status, ResultStatus::ConnectorFailure);
    assert_eq!(
        result.data.unwrap()["connector_error"]["kind"],
        "provider_operation_failed"
    );
}
