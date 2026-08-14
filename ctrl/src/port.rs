use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PortContract {
    pub id: &'static str,
    pub version: &'static str,
    pub purpose: &'static str,
    pub operations: &'static [&'static str],
    pub mutation_class: &'static str,
}

pub const WORK_DISCOVERY_PORT: PortContract = PortContract {
    id: "WorkDiscovery",
    version: "1.0.0",
    purpose: "Discover and resolve ordinary Work items without requiring a Central-specific project format.",
    operations: &["list"],
    mutation_class: "read-only",
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
    InvalidInput,
    ProviderOperationFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PortError {
    pub code: PortErrorCode,
    pub message: String,
}

impl PortError {
    pub fn provider(message: impl Into<String>) -> Self {
        Self { code: PortErrorCode::ProviderOperationFailed, message: message.into() }
    }
}

pub trait WorkDiscovery: Send + Sync {
    fn list(&self, input: &WorkDiscoveryInput) -> Result<WorkDiscoveryOutput, PortError>;
}
