use crate::{
    action::ActionRegistry,
    connector::ConnectorRegistry,
    control_registry,
    reference_connectors::{FilesystemWorkDiscovery, filesystem_work_discovery_metadata},
};

pub struct Runtime {
    pub actions: ActionRegistry,
    pub connectors: ConnectorRegistry,
    pub environment: String,
}

impl Runtime {
    pub fn new(connectors: ConnectorRegistry, environment: impl Into<String>) -> Self {
        let mut actions = ActionRegistry::core();
        control_registry::register(&mut actions);
        Self {
            actions,
            connectors,
            environment: environment.into(),
        }
    }

    pub fn without_connectors(environment: impl Into<String>) -> Self {
        Self::new(ConnectorRegistry::new(), environment)
    }
}

impl Default for Runtime {
    fn default() -> Self {
        let mut connectors = ConnectorRegistry::new();
        connectors.register_work_discovery(
            filesystem_work_discovery_metadata(),
            FilesystemWorkDiscovery,
        );
        Self::new(connectors, std::env::consts::OS)
    }
}
