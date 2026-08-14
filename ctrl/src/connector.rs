use crate::port::{PortContract, WorkDiscovery};
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConnectorPortDeclaration {
    pub id: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConnectorManifest {
    pub id: String,
    pub version: String,
    pub display_name: String,
    pub ports: Vec<ConnectorPortDeclaration>,
    pub platforms: Vec<String>,
    pub entrypoint: String,
    pub runtime_requirements: Vec<String>,
    pub dependency_probes: Vec<String>,
    pub configuration_requirements: Vec<String>,
    pub mutation_scope: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorContext {
    pub platform: String,
}

impl ConnectorContext {
    pub fn current() -> Self {
        Self { platform: std::env::consts::OS.to_owned() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityProbe {
    pub available: bool,
    pub reason: Option<String>,
}

impl CapabilityProbe {
    pub fn available() -> Self {
        Self { available: true, reason: None }
    }
}

pub trait Connector: Send + Sync {
    fn manifest(&self) -> &ConnectorManifest;
    fn probe(&self, port: &PortContract, context: &ConnectorContext) -> CapabilityProbe;
    fn work_discovery(&self) -> Option<&dyn WorkDiscovery> {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConnectorSummary {
    pub id: String,
    pub version: String,
}

impl ConnectorSummary {
    fn from_connector(connector: &dyn Connector) -> Self {
        Self {
            id: connector.manifest().id.clone(),
            version: connector.manifest().version.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IneligibleConnector {
    pub connector: ConnectorSummary,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConnectorDiagnostics {
    pub eligible: Vec<ConnectorSummary>,
    pub ineligible: Vec<IneligibleConnector>,
    pub selected_connector: Option<ConnectorSummary>,
}

pub struct ConnectorResolution<'a> {
    pub connector: Option<&'a dyn Connector>,
    pub diagnostics: ConnectorDiagnostics,
}

#[derive(Default)]
pub struct ConnectorRegistry {
    connectors: Vec<Box<dyn Connector>>,
}

impl ConnectorRegistry {
    pub fn register<C: Connector + 'static>(&mut self, connector: C) -> Result<&mut Self, String> {
        validate_manifest(connector.manifest())?;
        if self.connectors.iter().any(|existing| existing.manifest().id == connector.manifest().id) {
            return Err(format!("Connector already registered: {}", connector.manifest().id));
        }
        self.connectors.push(Box::new(connector));
        Ok(self)
    }

    pub fn resolve<'a>(&'a self, port: &PortContract, context: &ConnectorContext) -> ConnectorResolution<'a> {
        let mut eligible = Vec::<&dyn Connector>::new();
        let mut ineligible = Vec::new();

        for connector in &self.connectors {
            let connector = connector.as_ref();
            let summary = ConnectorSummary::from_connector(connector);
            let manifest = connector.manifest();
            let port_match = manifest.ports.iter().any(|candidate| candidate.id == port.id && candidate.version == port.version);
            if !port_match {
                ineligible.push(IneligibleConnector {
                    connector: summary,
                    reason: format!("does not declare compatible {} {}", port.id, port.version),
                });
                continue;
            }
            if !manifest.platforms.iter().any(|platform| platform == "*" || platform == &context.platform) {
                ineligible.push(IneligibleConnector {
                    connector: summary,
                    reason: format!("unsupported platform: {}", context.platform),
                });
                continue;
            }
            let probe = connector.probe(port, context);
            if !probe.available {
                ineligible.push(IneligibleConnector {
                    connector: summary,
                    reason: probe.reason.unwrap_or_else(|| "capability probe reported unavailable".to_owned()),
                });
                continue;
            }
            eligible.push(connector);
        }

        eligible.sort_by(|left, right| left.manifest().id.cmp(&right.manifest().id));
        ineligible.sort_by(|left, right| left.connector.id.cmp(&right.connector.id));
        let selected = eligible.first().copied();
        let eligible_summaries = eligible.iter().map(|connector| ConnectorSummary::from_connector(*connector)).collect();
        let selected_connector = selected.map(ConnectorSummary::from_connector);

        ConnectorResolution {
            connector: selected,
            diagnostics: ConnectorDiagnostics {
                eligible: eligible_summaries,
                ineligible,
                selected_connector,
            },
        }
    }
}

fn validate_manifest(manifest: &ConnectorManifest) -> Result<(), String> {
    for (label, value) in [
        ("id", manifest.id.as_str()),
        ("version", manifest.version.as_str()),
        ("display_name", manifest.display_name.as_str()),
        ("entrypoint", manifest.entrypoint.as_str()),
        ("mutation_scope", manifest.mutation_scope.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(format!("Connector manifest {label} must be non-empty."));
        }
    }
    if manifest.ports.is_empty() {
        return Err("Connector manifest must declare at least one Port.".to_owned());
    }
    let mut ids = BTreeSet::new();
    for port in &manifest.ports {
        if port.id.trim().is_empty() || port.version.trim().is_empty() {
            return Err("Connector Port declarations require id and version.".to_owned());
        }
        if !ids.insert((&port.id, &port.version)) {
            return Err(format!("Duplicate Connector Port declaration: {} {}", port.id, port.version));
        }
    }
    Ok(())
}
