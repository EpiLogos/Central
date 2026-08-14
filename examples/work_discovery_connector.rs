use central_ctrl::sdk::{
    CapabilityProbe, ConnectorMetadata, MutationClass, PortCompatibility,
    WORK_DISCOVERY_CONTRACT_ID, WORK_DISCOVERY_PORT_ID, WorkDiscovery, WorkDiscoveryError, WorkItem,
    conformance,
};
use std::path::Path;

struct ExampleWorkDiscovery;

impl WorkDiscovery for ExampleWorkDiscovery {
    fn probe(&self) -> CapabilityProbe {
        CapabilityProbe::available()
    }

    fn list(&self, _central_root: &Path) -> Result<Vec<WorkItem>, WorkDiscoveryError> {
        Ok(vec![WorkItem {
            name: "example".into(),
            path: "/example".into(),
        }])
    }
}

fn metadata() -> ConnectorMetadata {
    ConnectorMetadata {
        id: "example.work-discovery".into(),
        version: "0.1.0".into(),
        display_name: "Example Work discovery".into(),
        implemented_ports: vec![PortCompatibility {
            id: WORK_DISCOVERY_PORT_ID.into(),
            contract: WORK_DISCOVERY_CONTRACT_ID.into(),
        }],
        supported_environments: vec!["*".into()],
        entrypoint: "examples/work_discovery_connector.rs".into(),
        runtime_requirements: Vec::new(),
        dependency_probes: Vec::new(),
        configuration_requirements: Vec::new(),
        mutation_scope: MutationClass::ReadOnly,
    }
}

fn main() {
    let report = conformance::work_discovery(&metadata(), &ExampleWorkDiscovery, Path::new("."));
    assert!(report.passed, "{:?}", report.failures);
}
