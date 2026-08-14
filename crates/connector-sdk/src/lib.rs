mod conformance;
mod connector;
mod port;

pub use conformance::{
    run_work_discovery_conformance, ConformanceFailure, ConformanceReport,
    WorkDiscoveryConformanceFixture,
};
pub use connector::{
    validate_connector_manifest, CapabilityProbe, Connector, ConnectorContext, ConnectorDiagnostics,
    ConnectorManifest, ConnectorPortDeclaration, ConnectorRegistry, ConnectorResolution,
    ConnectorSummary, IneligibleConnector, ManifestError, CONNECTOR_API_VERSION,
};
pub use port::{
    PortContract, PortError, PortErrorCode, PortOperationContract, WorkDiscovery, WorkDiscoveryInput,
    WorkDiscoveryOutput, WorkItem, WORK_DISCOVERY_OPERATIONS, WORK_DISCOVERY_PORT,
};
