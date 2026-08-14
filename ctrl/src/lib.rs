pub mod action;
pub mod cli;
pub mod control;
pub mod machine;
pub mod picker;
pub mod result;
pub mod root;

pub use action::{
    create_core_action_registry, ActionDescriptor, ActionExecutionContext, ActionInputDefinition,
    ActionInputSelection, ActionRegistry, MutationClass,
};
pub use cli::{run_cli, run_cli_with_surface, CliEnvironment, CliExecution};
pub use control::{
    locate_control_root, search_control, ControlSearchMatch, ControlSearchResult, ControlSourceRoot,
    SourceClass, CONTROL_ROOTS,
};
pub use machine::{
    explain_machine_declaration, explain_machine_inspection, explain_machine_plan,
    read_machine_declaration, AuthoredMachineDeclaration, ConfigurationRequirement,
    MachineDeclaration, MachineDeclarationError, MachineDeclarationSource, MachineObservationSource,
    MachinePlan, MachinePlanEntry, MachinePlanStatus, MachinePlanSummary, MachineRequirements,
    MachineSourceReference, ObservedMachine, PackageRequirement, PresenceState, ServiceRequirement,
    MACHINE_DECLARATION_SCHEMA, MACHINE_DECLARATION_VERSION,
};
pub use picker::{
    run_guided_action_picker, search_action_descriptors, NullTerminalSurface, StdioTerminalSurface,
    TerminalSurface,
};
pub use central_connector_sdk::{
    run_machine_inspector_conformance, validate_connector_manifest, CapabilityProbe, Connector,
    ConnectorContext, ConnectorDiagnostics, ConnectorManifest, ConnectorPortDeclaration,
    ConnectorRegistry, ConnectorSummary, ConfigurationStateRequest, MachineInspectionInput,
    MachineInspectionOutput, MachineInspector, MachineInspectorConformanceFixture,
    ObservedConfiguration, ObservedPackage, ObservedService, PackageStateRequest, PortContract,
    PortError, PortErrorCode, ReconciliationSourceReference, ServiceStateRequest, StateChangePreview,
    StateChangeResult, WorkDiscovery, WorkDiscoveryInput, WorkDiscoveryOutput, WorkItem,
    CONFIGURATION_MANAGER_PORT, MACHINE_INSPECTOR_PORT, PACKAGE_MANAGER_PORT, SERVICE_MANAGER_PORT,
    WORK_DISCOVERY_PORT,
};
pub use central_reference_connectors::{
    create_default_connector_registry, FilesystemWorkConnector, StaticMachineInspectorConnector,
    StaticWorkConnector,
};
pub use result::{ActionResult, ResultStatus};
pub use root::{inspect_central, initialize_central, resolve_central_root, ResolvedRoot, RootOptions};
