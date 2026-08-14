use serde::{Deserialize, Serialize};
use std::path::Path;

pub const WORK_DISCOVERY_PORT_ID: &str = "WorkDiscovery";
pub const WORK_DISCOVERY_CONTRACT_ID: &str = "WorkDiscovery/v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkItem { pub name: String, pub path: String }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityProbe { pub eligible: bool, pub reasons: Vec<String> }

impl CapabilityProbe {
    pub fn available() -> Self { Self { eligible: true, reasons: Vec::new() } }
    pub fn unavailable(reason: impl Into<String>) -> Self { Self { eligible: false, reasons: vec![reason.into()] } }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkDiscoveryErrorKind { InvalidRoot, ProviderOperationFailed }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkDiscoveryError { pub kind: WorkDiscoveryErrorKind, pub message: String }

impl WorkDiscoveryError {
    pub fn new(kind: WorkDiscoveryErrorKind, message: impl Into<String>) -> Self { Self { kind, message: message.into() } }
}

pub trait WorkDiscovery: Send + Sync {
    fn probe(&self) -> CapabilityProbe;
    fn list(&self, central_root: &Path) -> Result<Vec<WorkItem>, WorkDiscoveryError>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IneligibleConnector { pub id: String, pub reasons: Vec<String> }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectorDiagnostics { pub port: String, pub contract: String, pub environment: String, pub eligible_connectors: Vec<String>, pub ineligible_connectors: Vec<IneligibleConnector>, pub selected_connector: Option<String> }
