pub mod action;
pub mod cli;
pub mod control;
pub mod result;
pub mod root;

pub use action::{create_core_action_registry, ActionDescriptor, ActionExecutionContext, ActionRegistry, MutationClass};
pub use cli::{run_cli, CliEnvironment, CliExecution};
pub use control::{
    locate_control_root, search_control, ControlSearchMatch, ControlSearchResult, ControlSourceRoot,
    SourceClass, CONTROL_ROOTS,
};
pub use central_connector_sdk::{
    validate_connector_manifest, CapabilityProbe, Connector, ConnectorContext, ConnectorDiagnostics,
    ConnectorManifest, ConnectorRegistry, PortContract, WorkDiscovery, WorkDiscoveryInput,
    WorkDiscoveryOutput, WorkItem, WORK_DISCOVERY_PORT,
};
pub use central_reference_connectors::{
    create_default_connector_registry, FilesystemWorkConnector, StaticWorkConnector,
};
pub use result::{ActionResult, ResultStatus};
pub use root::{inspect_central, initialize_central, resolve_central_root, ResolvedRoot, RootOptions};
