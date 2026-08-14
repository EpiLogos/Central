use crate::sdk::{
    CapabilityProbe, ConnectorMetadata, MutationClass, PortCompatibility,
    WORK_DISCOVERY_CONTRACT_ID, WORK_DISCOVERY_PORT_ID, WorkDiscovery, WorkDiscoveryError,
    WorkDiscoveryErrorKind, WorkItem,
};
use std::{fs, path::Path};

pub const FILESYSTEM_WORK_DISCOVERY_ID: &str = "reference.filesystem-work-discovery";
pub const STATIC_WORK_DISCOVERY_ID: &str = "reference.static-work-discovery";

pub fn filesystem_work_discovery_metadata() -> ConnectorMetadata {
    work_discovery_metadata(
        FILESYSTEM_WORK_DISCOVERY_ID,
        "Reference filesystem Work discovery",
        "rust:connectors/reference/work_discovery.rs#FilesystemWorkDiscovery",
    )
}

pub fn static_work_discovery_metadata() -> ConnectorMetadata {
    work_discovery_metadata(
        STATIC_WORK_DISCOVERY_ID,
        "Reference static Work discovery",
        "rust:connectors/reference/work_discovery.rs#StaticWorkDiscovery",
    )
}

fn work_discovery_metadata(id: &str, display_name: &str, entrypoint: &str) -> ConnectorMetadata {
    ConnectorMetadata {
        id: id.into(),
        version: "0.1.0".into(),
        display_name: display_name.into(),
        implemented_ports: vec![PortCompatibility {
            id: WORK_DISCOVERY_PORT_ID.into(),
            contract: WORK_DISCOVERY_CONTRACT_ID.into(),
        }],
        supported_environments: vec!["*".into()],
        entrypoint: entrypoint.into(),
        runtime_requirements: Vec::new(),
        dependency_probes: Vec::new(),
        configuration_requirements: Vec::new(),
        mutation_scope: MutationClass::ReadOnly,
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FilesystemWorkDiscovery;

impl WorkDiscovery for FilesystemWorkDiscovery {
    fn probe(&self) -> CapabilityProbe {
        CapabilityProbe::available()
    }

    fn list(&self, central_root: &Path) -> Result<Vec<WorkItem>, WorkDiscoveryError> {
        let work_root = central_root.join("Work");
        let entries = fs::read_dir(&work_root).map_err(|error| {
            let kind = if error.kind() == std::io::ErrorKind::NotFound {
                WorkDiscoveryErrorKind::InvalidRoot
            } else {
                WorkDiscoveryErrorKind::ProviderOperationFailed
            };
            WorkDiscoveryError::new(
                kind,
                format!("cannot read Work root {}: {error}", work_root.display()),
            )
        })?;

        let mut items = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| {
                WorkDiscoveryError::new(
                    WorkDiscoveryErrorKind::ProviderOperationFailed,
                    format!("cannot read Work entry: {error}"),
                )
            })?;
            let file_type = entry.file_type().map_err(|error| {
                WorkDiscoveryError::new(
                    WorkDiscoveryErrorKind::ProviderOperationFailed,
                    format!("cannot inspect Work entry: {error}"),
                )
            })?;
            if !file_type.is_dir() {
                continue;
            }

            items.push(WorkItem {
                name: entry.file_name().to_string_lossy().into_owned(),
                path: entry.path().display().to_string(),
            });
        }

        items.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then_with(|| left.path.cmp(&right.path))
        });
        Ok(items)
    }
}

#[derive(Debug, Clone)]
pub struct StaticWorkDiscovery {
    items: Vec<WorkItem>,
    probe: CapabilityProbe,
}

impl StaticWorkDiscovery {
    pub fn new(items: Vec<WorkItem>) -> Self {
        Self {
            items,
            probe: CapabilityProbe::available(),
        }
    }

    pub fn with_probe(items: Vec<WorkItem>, probe: CapabilityProbe) -> Self {
        Self { items, probe }
    }
}

impl WorkDiscovery for StaticWorkDiscovery {
    fn probe(&self) -> CapabilityProbe {
        self.probe.clone()
    }

    fn list(&self, _central_root: &Path) -> Result<Vec<WorkItem>, WorkDiscoveryError> {
        let mut items = self.items.clone();
        items.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then_with(|| left.path.cmp(&right.path))
        });
        Ok(items)
    }
}
