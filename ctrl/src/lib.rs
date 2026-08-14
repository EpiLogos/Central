pub mod action;
pub mod cli;
pub mod connector;
pub mod port;
pub mod reference;
pub mod result;
pub mod root;

pub use action::{create_core_action_registry, ActionDescriptor, ActionExecutionContext, ActionRegistry, MutationClass};
pub use cli::{run_cli, CliEnvironment, CliExecution};
pub use connector::{Connector, ConnectorContext, ConnectorDiagnostics, ConnectorManifest, ConnectorRegistry};
pub use port::{PortContract, WorkDiscovery, WorkDiscoveryInput, WorkDiscoveryOutput, WorkItem, WORK_DISCOVERY_PORT};
pub use reference::{create_default_connector_registry, FilesystemWorkConnector, StaticWorkConnector};
pub use result::{ActionResult, ResultStatus};
pub use root::{inspect_central, initialize_central, resolve_central_root, ResolvedRoot, RootOptions};
