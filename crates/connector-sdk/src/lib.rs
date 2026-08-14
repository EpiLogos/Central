mod conformance;
mod connector;
mod port;

pub use conformance::{
    run_machine_inspector_conformance, run_work_discovery_conformance, ConformanceFailure,
    ConformanceReport, MachineInspectorConformanceFixture, WorkDiscoveryConformanceFixture,
};
pub use connector::{
    validate_connector_manifest, CapabilityProbe, Connector, ConnectorContext, ConnectorDiagnostics,
    ConnectorManifest, ConnectorPortDeclaration, ConnectorRegistry, ConnectorResolution,
    ConnectorSummary, IneligibleConnector, ManifestError, CONNECTOR_API_VERSION,
};
pub use port::{
    ConfigurationStateRequest, MachineInspectionInput, MachineInspectionOutput, MachineInspector,
    ObservedConfiguration, ObservedPackage, ObservedService, PackageStateRequest, PortContract,
    PortError, PortErrorCode, PortOperationContract, ReconciliationSourceReference,
    ServiceStateRequest, StateChangePreview, StateChangeResult, WorkDiscovery, WorkDiscoveryInput,
    WorkDiscoveryOutput, WorkItem, CONFIGURATION_MANAGER_OPERATIONS, CONFIGURATION_MANAGER_PORT,
    MACHINE_INSPECTOR_OPERATIONS, MACHINE_INSPECTOR_PORT, PACKAGE_MANAGER_OPERATIONS,
    PACKAGE_MANAGER_PORT, SERVICE_MANAGER_OPERATIONS, SERVICE_MANAGER_PORT,
    WORK_DISCOVERY_OPERATIONS, WORK_DISCOVERY_PORT,
};
