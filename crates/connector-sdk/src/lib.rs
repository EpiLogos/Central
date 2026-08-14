mod conformance;
mod connector;
mod port;
mod synchronization_conformance;

pub use conformance::{
    run_configuration_manager_conformance, run_machine_inspector_conformance,
    run_package_manager_conformance, run_service_manager_conformance,
    run_work_discovery_conformance, ConfigurationManagerConformanceFixture, ConformanceFailure,
    ConformanceReport, MachineInspectorConformanceFixture, PackageManagerConformanceFixture,
    ServiceManagerConformanceFixture, WorkDiscoveryConformanceFixture,
};
pub use connector::{
    validate_connector_manifest, CapabilityProbe, Connector, ConnectorContext, ConnectorDiagnostics,
    ConnectorManifest, ConnectorPortDeclaration, ConnectorRegistry, ConnectorResolution,
    ConnectorSummary, IneligibleConnector, ManifestError, CONNECTOR_API_VERSION,
};
pub use port::{
    ConfigurationManager, ConfigurationStateRequest, MachineInspectionInput, MachineInspectionOutput,
    MachineInspector, ObservedConfiguration, ObservedPackage, ObservedService, PackageManager,
    PackageStateRequest, PortContract, PortError, PortErrorCode, PortOperationContract,
    ReconciliationSourceReference, ServiceManager, ServiceStateRequest, StateChangePreview,
    StateChangeResult, SynchronizationRequest, Synchronizer, WorkDiscovery, WorkDiscoveryInput,
    WorkDiscoveryOutput, WorkItem, CONFIGURATION_MANAGER_OPERATIONS, CONFIGURATION_MANAGER_PORT,
    MACHINE_INSPECTOR_OPERATIONS, MACHINE_INSPECTOR_PORT, PACKAGE_MANAGER_OPERATIONS,
    PACKAGE_MANAGER_PORT, SERVICE_MANAGER_OPERATIONS, SERVICE_MANAGER_PORT,
    SYNCHRONIZER_OPERATIONS, SYNCHRONIZER_PORT, WORK_DISCOVERY_OPERATIONS, WORK_DISCOVERY_PORT,
};
pub use synchronization_conformance::{
    run_synchronizer_conformance, SynchronizerConformanceFixture, SynchronizerConformanceReport,
};
