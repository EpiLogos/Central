use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PortOperationContract {
    pub name: &'static str,
    pub input_type: &'static str,
    pub output_type: &'static str,
    pub mutation_class: &'static str,
    pub preview_required: bool,
    pub idempotent: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PortContract {
    pub id: &'static str,
    pub version: &'static str,
    pub purpose: &'static str,
    pub operations: &'static [PortOperationContract],
}

pub const WORK_DISCOVERY_OPERATIONS: [PortOperationContract; 1] = [PortOperationContract {
    name: "list",
    input_type: "WorkDiscoveryInput",
    output_type: "WorkDiscoveryOutput",
    mutation_class: "read-only",
    preview_required: false,
    idempotent: true,
}];

pub const WORK_DISCOVERY_PORT: PortContract = PortContract {
    id: "WorkDiscovery",
    version: "1.0.0",
    purpose: "Discover and resolve ordinary Work items without requiring a Central-specific project format.",
    operations: &WORK_DISCOVERY_OPERATIONS,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkDiscoveryInput {
    pub work_root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkItem {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkDiscoveryOutput {
    pub items: Vec<WorkItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PortErrorCode {
    UnsupportedEnvironment,
    MissingDependency,
    InvalidConfiguration,
    CapabilityUnavailable,
    InvalidInput,
    ProviderOperationFailed,
    PermissionFailure,
    VerificationFailure,
    UnexpectedConnectorFailure,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PortError {
    pub code: PortErrorCode,
    pub message: String,
    pub provider_detail: Option<String>,
}

impl PortError {
    pub fn new(code: PortErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), provider_detail: None }
    }

    pub fn provider(message: impl Into<String>) -> Self {
        Self::new(PortErrorCode::ProviderOperationFailed, message)
    }
}

pub trait WorkDiscovery: Send + Sync {
    fn list(&self, input: &WorkDiscoveryInput) -> Result<WorkDiscoveryOutput, PortError>;
}
