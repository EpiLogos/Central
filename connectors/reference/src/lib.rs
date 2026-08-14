use central_connector_sdk::{
    CapabilityProbe, ConfigurationManager, ConfigurationStateRequest, Connector, ConnectorContext,
    ConnectorManifest, ConnectorPortDeclaration, ConnectorRegistry, MachineInspectionInput,
    MachineInspectionOutput, MachineInspector, ObservedConfiguration, ObservedPackage,
    ObservedService, PackageManager, PackageStateRequest, PortContract, PortError, ServiceManager,
    ServiceStateRequest, StateChangePreview, StateChangeResult, WorkDiscovery, WorkDiscoveryInput,
    WorkDiscoveryOutput, WorkItem, CONFIGURATION_MANAGER_PORT, CONNECTOR_API_VERSION,
    MACHINE_INSPECTOR_PORT, PACKAGE_MANAGER_PORT, SERVICE_MANAGER_PORT, WORK_DISCOVERY_PORT,
};
use std::fs;
use std::sync::{Arc, Mutex};

fn manifest_with_scope(
    id: &str,
    display_name: &str,
    entrypoint: &str,
    ports: &[PortContract],
    mutation_scope: &str,
) -> ConnectorManifest {
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
        mutation_scope: mutation_scope.to_owned(),
    }
}

fn manifest(id: &str, display_name: &str, entrypoint: &str, ports: &[PortContract]) -> ConnectorManifest {
    manifest_with_scope(id, display_name, entrypoint, ports, "read-only")
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

#[derive(Clone)]
pub struct SharedMachineState {
    state: Arc<Mutex<MachineInspectionOutput>>,
}

impl SharedMachineState {
    pub fn new(observation: MachineInspectionOutput) -> Self {
        Self { state: Arc::new(Mutex::new(observation)) }
    }

    pub fn snapshot(&self) -> MachineInspectionOutput {
        self.state.lock().expect("reference machine state lock poisoned").clone()
    }
}

pub struct InMemoryMachineConnector {
    manifest: ConnectorManifest,
    state: SharedMachineState,
}

impl InMemoryMachineConnector {
    pub fn new(observation: MachineInspectionOutput) -> Self {
        Self::with_state(SharedMachineState::new(observation))
    }

    pub fn with_state(state: SharedMachineState) -> Self {
        Self {
            manifest: manifest_with_scope(
                "reference.machine-reconciler",
                "Reference in-memory machine reconciler",
                "rust:central-reference-connectors::InMemoryMachineConnector",
                &[
                    MACHINE_INSPECTOR_PORT,
                    PACKAGE_MANAGER_PORT,
                    CONFIGURATION_MANAGER_PORT,
                    SERVICE_MANAGER_PORT,
                ],
                "locally-mutating",
            ),
            state,
        }
    }

    pub fn state(&self) -> SharedMachineState {
        self.state.clone()
    }

    fn package_changed(&self, input: &PackageStateRequest) -> bool {
        let state = self.state.state.lock().expect("reference machine state lock poisoned");
        state.packages.iter().find(|item| item.id == input.id).map_or(input.present, |item| item.present != input.present)
    }

    fn configuration_changed(&self, input: &ConfigurationStateRequest) -> bool {
        let state = self.state.state.lock().expect("reference machine state lock poisoned");
        state.configurations.iter().find(|item| item.id == input.id).map_or(input.present, |item| item.present != input.present)
    }

    fn service_changed(&self, input: &ServiceStateRequest) -> bool {
        let state = self.state.state.lock().expect("reference machine state lock poisoned");
        let current = state.services.iter().find(|item| item.id == input.id);
        input.running.map_or(false, |value| current.map_or(value, |item| item.running != value))
            || input.enabled.map_or(false, |value| current.map_or(value, |item| item.enabled != value))
    }
}

impl MachineInspector for InMemoryMachineConnector {
    fn inspect(&self, _input: &MachineInspectionInput) -> Result<MachineInspectionOutput, PortError> {
        Ok(self.state.snapshot())
    }
}

impl PackageManager for InMemoryMachineConnector {
    fn preview(&self, input: &PackageStateRequest) -> Result<StateChangePreview, PortError> {
        let changed = self.package_changed(input);
        Ok(StateChangePreview {
            changed,
            summary: format!(
                "package {} -> {}",
                input.id,
                if input.present { "present" } else { "absent" }
            ),
        })
    }

    fn apply(&self, input: &PackageStateRequest) -> Result<StateChangeResult, PortError> {
        let changed = self.package_changed(input);
        if changed {
            let mut state = self.state.state.lock().expect("reference machine state lock poisoned");
            if let Some(item) = state.packages.iter_mut().find(|item| item.id == input.id) {
                item.present = input.present;
            } else {
                state.packages.push(ObservedPackage { id: input.id.clone(), present: input.present });
                state.packages.sort_by(|left, right| left.id.cmp(&right.id));
            }
        }
        Ok(StateChangeResult {
            changed,
            summary: format!(
                "package {} is {}",
                input.id,
                if input.present { "present" } else { "absent" }
            ),
        })
    }
}

impl ConfigurationManager for InMemoryMachineConnector {
    fn preview(&self, input: &ConfigurationStateRequest) -> Result<StateChangePreview, PortError> {
        let changed = self.configuration_changed(input);
        Ok(StateChangePreview {
            changed,
            summary: format!(
                "configuration {} -> {}",
                input.id,
                if input.present { "present" } else { "absent" }
            ),
        })
    }

    fn apply(&self, input: &ConfigurationStateRequest) -> Result<StateChangeResult, PortError> {
        let changed = self.configuration_changed(input);
        if changed {
            let mut state = self.state.state.lock().expect("reference machine state lock poisoned");
            if let Some(item) = state.configurations.iter_mut().find(|item| item.id == input.id) {
                item.present = input.present;
            } else {
                state.configurations.push(ObservedConfiguration { id: input.id.clone(), present: input.present });
                state.configurations.sort_by(|left, right| left.id.cmp(&right.id));
            }
        }
        Ok(StateChangeResult {
            changed,
            summary: format!(
                "configuration {} is {}",
                input.id,
                if input.present { "present" } else { "absent" }
            ),
        })
    }
}

impl ServiceManager for InMemoryMachineConnector {
    fn preview(&self, input: &ServiceStateRequest) -> Result<StateChangePreview, PortError> {
        let changed = self.service_changed(input);
        Ok(StateChangePreview {
            changed,
            summary: format!(
                "service {} -> running={:?}, enabled={:?}",
                input.id, input.running, input.enabled
            ),
        })
    }

    fn apply(&self, input: &ServiceStateRequest) -> Result<StateChangeResult, PortError> {
        let changed = self.service_changed(input);
        if changed {
            let mut state = self.state.state.lock().expect("reference machine state lock poisoned");
            if let Some(item) = state.services.iter_mut().find(|item| item.id == input.id) {
                if let Some(running) = input.running {
                    item.running = running;
                }
                if let Some(enabled) = input.enabled {
                    item.enabled = enabled;
                }
                item.present = true;
            } else {
                state.services.push(ObservedService {
                    id: input.id.clone(),
                    present: true,
                    running: input.running.unwrap_or(false),
                    enabled: input.enabled.unwrap_or(false),
                });
                state.services.sort_by(|left, right| left.id.cmp(&right.id));
            }
        }
        Ok(StateChangeResult {
            changed,
            summary: format!(
                "service {} is running={:?}, enabled={:?}",
                input.id, input.running, input.enabled
            ),
        })
    }
}

impl Connector for InMemoryMachineConnector {
    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    fn probe(&self, _port: &PortContract, _context: &ConnectorContext) -> CapabilityProbe {
        CapabilityProbe::available()
    }

    fn machine_inspector(&self) -> Option<&dyn MachineInspector> {
        Some(self)
    }

    fn package_manager(&self) -> Option<&dyn PackageManager> {
        Some(self)
    }

    fn configuration_manager(&self) -> Option<&dyn ConfigurationManager> {
        Some(self)
    }

    fn service_manager(&self) -> Option<&dyn ServiceManager> {
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
