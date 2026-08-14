use serde::{Deserialize, Serialize};

use crate::{
    action::MutationClass,
    port::{WORK_DISCOVERY_CONTRACT_ID, WORK_DISCOVERY_PORT_ID, WorkDiscovery},
};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PortCompatibility {
    pub id: String,
    pub contract: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
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
    fn supports_work_discovery(&self) -> bool {
        self.implemented_ports.iter().any(|port| {
            port.id == WORK_DISCOVERY_PORT_ID && port.contract == WORK_DISCOVERY_CONTRACT_ID
        })
    }

    fn supports_environment(&self, environment: &str) -> bool {
        self.supported_environments
            .iter()
            .any(|supported| supported == "*" || supported == environment)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IneligibleConnector {
    pub id: String,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectorDiagnostics {
    pub port: String,
    pub contract: String,
    pub environment: String,
    pub eligible_connectors: Vec<String>,
    pub ineligible_connectors: Vec<IneligibleConnector>,
    pub selected_connector: Option<String>,
}

struct RegisteredWorkDiscovery {
    metadata: ConnectorMetadata,
    implementation: Box<dyn WorkDiscovery>,
}

#[derive(Default)]
pub struct ConnectorRegistry {
    work_discovery: Vec<RegisteredWorkDiscovery>,
}

pub struct WorkDiscoveryResolution<'a> {
    pub selected: Option<&'a dyn WorkDiscovery>,
    pub diagnostics: ConnectorDiagnostics,
}

impl ConnectorRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_work_discovery<C>(&mut self, metadata: ConnectorMetadata, connector: C)
    where
        C: WorkDiscovery + 'static,
    {
        self.work_discovery
            .retain(|registered| registered.metadata.id != metadata.id);
        self.work_discovery.push(RegisteredWorkDiscovery {
            metadata,
            implementation: Box::new(connector),
        });
    }

    pub fn resolve_work_discovery(&self, environment: &str) -> WorkDiscoveryResolution<'_> {
        let mut eligible = Vec::new();
        let mut ineligible = Vec::new();

        for registered in &self.work_discovery {
            let mut reasons = Vec::new();

            if !registered.metadata.supports_work_discovery() {
                reasons.push(format!(
                    "does not declare compatible contract {WORK_DISCOVERY_CONTRACT_ID}"
                ));
            }
            if !registered.metadata.supports_environment(environment) {
                reasons.push(format!("does not support environment {environment}"));
            }
            if reasons.is_empty() {
                let probe = registered.implementation.probe();
                if probe.eligible {
                    eligible.push(registered);
                } else {
                    reasons.extend(probe.reasons);
                }
            }
            if !reasons.is_empty() {
                ineligible.push(IneligibleConnector {
                    id: registered.metadata.id.clone(),
                    reasons,
                });
            }
        }

        eligible.sort_by(|left, right| left.metadata.id.cmp(&right.metadata.id));
        ineligible.sort_by(|left, right| left.id.cmp(&right.id));

        let eligible_connectors = eligible
            .iter()
            .map(|registered| registered.metadata.id.clone())
            .collect::<Vec<_>>();
        let selected_connector = eligible
            .first()
            .map(|registered| registered.metadata.id.clone());
        let selected = eligible
            .first()
            .map(|registered| registered.implementation.as_ref());

        WorkDiscoveryResolution {
            selected,
            diagnostics: ConnectorDiagnostics {
                port: WORK_DISCOVERY_PORT_ID.into(),
                contract: WORK_DISCOVERY_CONTRACT_ID.into(),
                environment: environment.into(),
                eligible_connectors,
                ineligible_connectors: ineligible,
                selected_connector,
            },
        }
    }
}
