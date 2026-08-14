use central_ctrl::{
    action::ActionRegistry,
    connector::ConnectorRegistry,
    port::{CapabilityProbe, WorkDiscovery, WorkDiscoveryError, WorkDiscoveryErrorKind, WorkItem},
    reference_connectors::{
        FILESYSTEM_WORK_DISCOVERY_ID, FilesystemWorkDiscovery, StaticWorkDiscovery,
        filesystem_work_discovery_metadata, static_work_discovery_metadata,
    },
    result::ResultStatus,
    root,
    work,
};
use std::path::Path;
use tempfile::tempdir;

fn item(name: &str, path: &str) -> WorkItem {
    WorkItem {
        name: name.into(),
        path: path.into(),
    }
}

#[test]
fn work_list_descriptor_requires_work_discovery() {
    let registry = ActionRegistry::core();
    let descriptor = registry.get("work.list").expect("work.list Action");
    assert_eq!(descriptor.required_ports, vec!["WorkDiscovery"]);
}

#[test]
fn filesystem_reference_connector_lists_ordinary_work_directories() {
    let temp = tempdir().unwrap();
    let central = temp.path().join("Central");
    root::initialize(&central).unwrap();
    std::fs::create_dir(central.join("Work/alpha")).unwrap();
    std::fs::create_dir(central.join("Work/beta")).unwrap();
    std::fs::write(central.join("Work/not-a-project.txt"), "ignored").unwrap();

    let connector = FilesystemWorkDiscovery;
    let items = connector.list(&central).unwrap();
    let names = items.iter().map(|work| work.name.as_str()).collect::<Vec<_>>();
    assert_eq!(names, vec!["alpha", "beta"]);
}

#[test]
fn two_valid_connectors_have_stable_selection_independent_of_registration_order() {
    fn registry(reverse: bool) -> ConnectorRegistry {
        let mut registry = ConnectorRegistry::new();
        let filesystem = (filesystem_work_discovery_metadata(), FilesystemWorkDiscovery);
        let static_connector = (
            static_work_discovery_metadata(),
            StaticWorkDiscovery::new(vec![item("static", "/tmp/static")]),
        );

        if reverse {
            registry.register_work_discovery(static_connector.0, static_connector.1);
            registry.register_work_discovery(filesystem.0, filesystem.1);
        } else {
            registry.register_work_discovery(filesystem.0, filesystem.1);
            registry.register_work_discovery(static_connector.0, static_connector.1);
        }
        registry
    }

    for reverse in [false, true] {
        let registry = registry(reverse);
        let resolution = registry.resolve_work_discovery("linux");
        assert_eq!(
            resolution.diagnostics.eligible_connectors,
            vec![
                "reference.filesystem-work-discovery",
                "reference.static-work-discovery"
            ]
        );
        assert_eq!(
            resolution.diagnostics.selected_connector.as_deref(),
            Some(FILESYSTEM_WORK_DISCOVERY_ID)
        );
    }
}

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
    assert_eq!(result.data.unwrap()["diagnostics"]["selected_connector"], serde_json::Value::Null);
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
    assert_eq!(result.data.unwrap()["connector_error"]["kind"], "provider_operation_failed");
}
