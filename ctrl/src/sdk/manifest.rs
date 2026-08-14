use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MutationClass { ReadOnly, LocallyMutating, ExternallyMutating }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PortCompatibility { pub id: String, pub contract: String }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectorMetadata {
    pub id: String,
    pub version: String,
    pub display_name: String,
    pub implemented_ports: Vec<PortCompatibility>,
    pub supported_environments: Vec<String>,
    pub entrypoint: String,
    pub runtime_requirements: Vec<String>,
    pub dependency_probes: Vec<String>,
    pub configuration_requirements: Vec<String>,
    pub mutation_scope: MutationClass,
}

impl ConnectorMetadata {
    pub fn declares(&self, port_id: &str, contract_id: &str) -> bool {
        self.implemented_ports.iter().any(|port| port.id == port_id && port.contract == contract_id)
    }

    pub fn supports_environment(&self, environment: &str) -> bool {
        self.supported_environments.iter().any(|supported| supported == "*" || supported == environment)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestError { pub message: String }

pub fn parse_connector_manifest(source: &str) -> Result<ConnectorMetadata, ManifestError> {
    serde_json::from_str(source).map_err(|error| ManifestError { message: error.to_string() })
}

pub fn validate_connector_metadata(metadata: &ConnectorMetadata) -> Vec<String> {
    let mut failures = Vec::new();
    if metadata.id.trim().is_empty() { failures.push("connector id must not be empty".into()); }
    if metadata.version.trim().is_empty() { failures.push("connector version must not be empty".into()); }
    if metadata.display_name.trim().is_empty() { failures.push("connector display_name must not be empty".into()); }
    if metadata.implemented_ports.is_empty() { failures.push("connector must declare at least one implemented Port".into()); }
    if metadata.supported_environments.is_empty() { failures.push("connector must declare at least one supported environment".into()); }
    if metadata.entrypoint.trim().is_empty() { failures.push("connector entrypoint must not be empty".into()); }
    failures
}
