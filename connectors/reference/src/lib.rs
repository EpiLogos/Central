use central_connector_sdk::{
    CapabilityProbe, Connector, ConnectorContext, ConnectorManifest, ConnectorPortDeclaration,
    ConnectorRegistry, MachineInspectionInput, MachineInspectionOutput, MachineInspector,
    PortContract, PortError, WorkDiscovery, WorkDiscoveryInput, WorkDiscoveryOutput, WorkItem,
    CONNECTOR_API_VERSION, MACHINE_INSPECTOR_PORT, WORK_DISCOVERY_PORT,
};
use std::fs;

fn manifest(id: &str, display_name: &str, entrypoint: &str, ports: &[PortContract]) -> ConnectorManifest {
    ConnectorManifest {
        api_version: CONNECTOR_API_VERSION.to_owned(),
        id: id.to_owned(),
        version: "0.1.0".to_owned(),
        display_name: display_name.to_owned(),
        ports: ports.iter().map(|port| ConnectorPortDeclaration {
            id: port.id.to_owned(),
            version: port.version.to_owned(),
        }).collect(),
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
                &[WORK_DISCOVERY_PORT],
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
                &[WORK_DISCOVERY_PORT],
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

pub struct StaticMachineInspectorConnector {
    manifest: ConnectorManifest,
    observation: MachineInspectionOutput,
}

impl StaticMachineInspectorConnector {
    pub fn new(observation: MachineInspectionOutput) -> Self {
        Self::with_identity(
            "reference.machine-static",
            "Reference static machine inspection",
            "rust:central-reference-connectors::StaticMachineInspectorConnector",
            observation,
        )
    }

    pub fn current_host() -> Self {
        Self::with_identity(
            "reference.machine-host",
            "Reference host identity inspection",
            "rust:central-reference-connectors::StaticMachineInspectorConnector::current_host",
            MachineInspectionOutput {
                platform: std::env::consts::OS.to_owned(),
                architecture: std::env::consts::ARCH.to_owned(),
                capabilities: Vec::new(),
                packages: Vec::new(),
                configurations: Vec::new(),
                services: Vec::new(),
            },
        )
    }

    fn with_identity(id: &str, display_name: &str, entrypoint: &str, observation: MachineInspectionOutput) -> Self {
        Self {
            manifest: manifest(id, display_name, entrypoint, &[MACHINE_INSPECTOR_PORT]),
            observation,
        }
    }
}

impl MachineInspector for StaticMachineInspectorConnector {
    fn inspect(&self, _input: &MachineInspectionInput) -> Result<MachineInspectionOutput, PortError> {
        Ok(self.observation.clone())
    }
}

impl Connector for StaticMachineInspectorConnector {
    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    fn probe(&self, _port: &PortContract, _context: &ConnectorContext) -> CapabilityProbe {
        CapabilityProbe::available()
    }

    fn machine_inspector(&self) -> Option<&dyn MachineInspector> {
        Some(self)
    }
}

pub fn create_default_connector_registry() -> ConnectorRegistry {
    let mut registry = ConnectorRegistry::default();
    registry.register(FilesystemWorkConnector::new()).expect("reference Connector manifest is valid");
    registry.register(StaticWorkConnector::new(Vec::new())).expect("reference Connector manifest is valid");
    registry.register(StaticMachineInspectorConnector::current_host()).expect("reference Connector manifest is valid");
    registry
}
