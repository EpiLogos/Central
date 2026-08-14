use central_connector_sdk::{
    CapabilityProbe, Connector, ConnectorContext, ConnectorManifest, ConnectorPortDeclaration,
    ConnectorRegistry, PortContract, PortError, WorkDiscovery, WorkDiscoveryInput,
    WorkDiscoveryOutput, WorkItem, CONNECTOR_API_VERSION, WORK_DISCOVERY_PORT,
};
use std::fs;

fn manifest(id: &str, display_name: &str, entrypoint: &str) -> ConnectorManifest {
    ConnectorManifest {
        api_version: CONNECTOR_API_VERSION.to_owned(),
        id: id.to_owned(),
        version: "0.1.0".to_owned(),
        display_name: display_name.to_owned(),
        ports: vec![ConnectorPortDeclaration {
            id: WORK_DISCOVERY_PORT.id.to_owned(),
            version: WORK_DISCOVERY_PORT.version.to_owned(),
        }],
        platforms: vec!["*".to_owned()],
        entrypoint: entrypoint.to_owned(),
        runtime_requirements: vec!["ctrl-rust".to_owned()],
        dependency_probes: Vec::new(),
        configuration_requirements: Vec::new(),
        mutation_scope: "read-only".to_owned(),
    }
}

pub struct FilesystemWorkConnector {
    manifest: ConnectorManifest,
}

impl FilesystemWorkConnector {
    pub fn new() -> Self {
        Self {
            manifest: manifest(
                "reference.work-filesystem",
                "Reference filesystem Work discovery",
                "rust:central-reference-connectors::FilesystemWorkConnector",
            ),
        }
    }
}

impl Default for FilesystemWorkConnector {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkDiscovery for FilesystemWorkConnector {
    fn list(&self, input: &WorkDiscoveryInput) -> Result<WorkDiscoveryOutput, PortError> {
        let entries = match fs::read_dir(&input.work_root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(WorkDiscoveryOutput { items: Vec::new() });
            }
            Err(error) => return Err(PortError::provider(error.to_string())),
        };
        let mut items = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| PortError::provider(error.to_string()))?;
            let file_type = entry.file_type().map_err(|error| PortError::provider(error.to_string()))?;
            if file_type.is_dir() {
                items.push(WorkItem {
                    name: entry.file_name().to_string_lossy().into_owned(),
                    path: entry.path(),
                });
            }
        }
        items.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(WorkDiscoveryOutput { items })
    }
}

impl Connector for FilesystemWorkConnector {
    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    fn probe(&self, _port: &PortContract, _context: &ConnectorContext) -> CapabilityProbe {
        CapabilityProbe::available()
    }

    fn work_discovery(&self) -> Option<&dyn WorkDiscovery> {
        Some(self)
    }
}

pub struct StaticWorkConnector {
    manifest: ConnectorManifest,
    items: Vec<WorkItem>,
}

impl StaticWorkConnector {
    pub fn new(items: Vec<WorkItem>) -> Self {
        Self {
            manifest: manifest(
                "reference.work-static",
                "Reference static Work discovery",
                "rust:central-reference-connectors::StaticWorkConnector",
            ),
            items,
        }
    }
}

impl WorkDiscovery for StaticWorkConnector {
    fn list(&self, _input: &WorkDiscoveryInput) -> Result<WorkDiscoveryOutput, PortError> {
        let mut items = self.items.clone();
        items.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(WorkDiscoveryOutput { items })
    }
}

impl Connector for StaticWorkConnector {
    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    fn probe(&self, _port: &PortContract, _context: &ConnectorContext) -> CapabilityProbe {
        CapabilityProbe::available()
    }

    fn work_discovery(&self) -> Option<&dyn WorkDiscovery> {
        Some(self)
    }
}

pub fn create_default_connector_registry() -> ConnectorRegistry {
    let mut registry = ConnectorRegistry::default();
    registry.register(FilesystemWorkConnector::new()).expect("reference Connector manifest is valid");
    registry.register(StaticWorkConnector::new(Vec::new())).expect("reference Connector manifest is valid");
    registry
}
