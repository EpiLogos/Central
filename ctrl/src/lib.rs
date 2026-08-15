pub mod action;
pub mod cli;
pub mod control;
pub mod machine;
pub mod picker;
pub mod recovery;
pub mod result;
pub mod root;

pub use action::{
    create_core_action_registry, ActionAvailability, ActionDescriptor, ActionExecutionContext,
    ActionInputDefinition, ActionInputSelection, ActionOutputDefinition, ActionRegistry,
    MutationClass,
};
pub use cli::{run_cli, run_cli_with_surface, CliEnvironment, CliExecution};
pub use control::{
    locate_control_root, search_control, ControlSearchMatch, ControlSearchResult,
    ControlSkippedSource, ControlSourceRoot, SourceClass, AGENT_RETRIEVAL_DENY_MARKER,
    CONTROL_ROOTS,
};
pub use machine::{
    explain_machine_apply, explain_machine_declaration, explain_machine_inspection,
    explain_machine_plan, explain_machine_verification, read_machine_declaration,
    AuthoredMachineDeclaration, ConfigurationRequirement, MachineApplyOperation,
    MachineApplyOutcome, MachineApplyReport, MachineDeclaration, MachineDeclarationError,
    MachineDeclarationSource, MachineObservationSource, MachinePlan, MachinePlanEntry,
    MachinePlanStatus, MachinePlanSummary, MachineRequirements, MachineSourceReference,
    MachineVerification, ObservedMachine, PackageRequirement, PresenceState, ServiceRequirement,
    MACHINE_DECLARATION_SCHEMA, MACHINE_DECLARATION_VERSION,
};
pub use picker::{
    run_guided_action_picker, search_action_descriptors, NullTerminalSurface, StdioTerminalSurface,
    TerminalSurface,
};
pub use recovery::{
    explain_recovery, explain_recovery_plan, AuthoredRecoveryDeclaration, RecoveryDeclaration,
    RecoveryDeclarationSource, RecoveryPlan, RecoverySynchronizationPlan,
    RecoverySynchronizationStatus, SynchronizationDeclaration, RECOVERY_DECLARATION_SCHEMA,
    RECOVERY_DECLARATION_VERSION,
};
pub use central_connector_sdk::{
    run_configuration_manager_conformance, run_machine_inspector_conformance,
    run_package_manager_conformance, run_service_manager_conformance,
    run_synchronizer_conformance, validate_connector_manifest, CapabilityProbe,
    ConfigurationManager, ConfigurationManagerConformanceFixture, ConfigurationStateRequest,
    Connector, ConnectorContext, ConnectorDiagnostics, ConnectorManifest,
    ConnectorPortDeclaration, ConnectorRegistry, ConnectorSummary, MachineInspectionInput,
    MachineInspectionOutput, MachineInspector, MachineInspectorConformanceFixture,
    ObservedConfiguration, ObservedPackage, ObservedService, PackageManager,
    PackageManagerConformanceFixture, PackageStateRequest, PortContract, PortError,
    PortErrorCode, ReconciliationSourceReference, ServiceManager,
    ServiceManagerConformanceFixture, ServiceStateRequest, StateChangePreview,
    StateChangeResult, SynchronizationRequest, Synchronizer, SynchronizerConformanceFixture,
    SynchronizerConformanceReport, WorkDiscovery, WorkDiscoveryInput, WorkDiscoveryOutput,
    WorkItem, CONFIGURATION_MANAGER_PORT, CONNECTOR_API_VERSION, MACHINE_INSPECTOR_PORT,
    PACKAGE_MANAGER_PORT, SERVICE_MANAGER_PORT, SYNCHRONIZER_PORT, WORK_DISCOVERY_PORT,
};
pub use central_reference_connectors::{
    create_default_connector_registry, FilesystemWorkConnector, InMemoryMachineConnector,
    SharedMachineState, StaticMachineInspectorConnector, StaticWorkConnector,
};
pub use result::{ActionResult, ResultStatus};
pub use root::{inspect_central, initialize_central, resolve_central_root, ResolvedRoot, RootOptions};
