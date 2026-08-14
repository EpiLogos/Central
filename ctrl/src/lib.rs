pub mod action;
pub mod cli;
pub mod result;
pub mod root;

pub use action::{create_core_action_registry, ActionDescriptor, ActionRegistry, MutationClass};
pub use cli::{run_cli, CliEnvironment, CliExecution};
pub use result::{ActionResult, ResultStatus};
pub use root::{inspect_central, initialize_central, resolve_central_root, ResolvedRoot, RootOptions};
