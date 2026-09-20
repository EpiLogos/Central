use crate::git_state::GIT_STATE_PORT;
use crate::notification::{UserNotification, USER_NOTIFICATION_PORT};
use crate::port::{
    Automation, ConfigurationManager, MachineInspector, NativeOpen, NativeReveal, PackageManager,
    PortContract, ServiceManager, Synchronizer, TagStore, WorkDiscovery, AUTOMATION_PORT,
    CONFIGURATION_MANAGER_PORT, MACHINE_INSPECTOR_PORT, NATIVE_OPEN_PORT, NATIVE_REVEAL_PORT,
    PACKAGE_MANAGER_PORT, SERVICE_MANAGER_PORT, SYNCHRONIZER_PORT, TAG_STORE_PORT,
    WORK_DISCOVERY_PORT,
};
use crate::source_history::{SourceHistory, SOURCE_HISTORY_PORT};
use serde::Serialize;
use std::collections::BTreeSet;

pub const CONNECTOR_API_VERSION: &str = "central.connector/v1";

/// The Port contracts this SDK publishes. A Connector manifest may only
/// declare these Ports, at their published versions, so a wrong-version or
/// unknown-Port declaration is refused at manifest validation instead of
/// surfacing later at registry resolution or conformance.
pub const PUBLISHED_PORTS: &[&PortContract] = &[
    &AUTOMATION_PORT,
    &CONFIGURATION_MANAGER_PORT,
    &GIT_STATE_PORT,
    &MACHINE_INSPECTOR_PORT,
    &NATIVE_OPEN_PORT,
    &NATIVE_REVEAL_PORT,
    &PACKAGE_MANAGER_PORT,
    &SERVICE_MANAGER_PORT,
    &SOURCE_HISTORY_PORT,
    &SYNCHRONIZER_PORT,
    &TAG_STORE_PORT,
    &USER_NOTIFICATION_PORT,
    &WORK_DISCOVERY_PORT,
];

/// Returns the published contract for a Port id, if this SDK publishes one.
pub fn published_port(port_id: &str) -> Option<&'static PortContract> {
    PUBLISHED_PORTS
        .iter()
        .copied()
        .find(|contract| contract.id == port_id)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConnectorPortDeclaration {
    pub id: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConnectorManifest {
    pub api_version: String,
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
    /// Provider-specific limitations an operator choosing between Connectors
    /// should know about. Every entry must be non-empty.
    #[serde(default)]
    pub known_limitations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ManifestError {
    pub code: String,
    pub message: String,
}

impl ManifestError {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
        }
    }
}

pub fn validate_connector_manifest(manifest: &ConnectorManifest) -> Result<(), ManifestError> {
    if manifest.api_version != CONNECTOR_API_VERSION {
        return Err(ManifestError::new(
            "unsupported_api_version",
            format!(
                "Unsupported Connector API version: {}",
                manifest.api_version
            ),
        ));
    }
    for (label, value) in [
        ("id", manifest.id.as_str()),
        ("version", manifest.version.as_str()),
        ("display_name", manifest.display_name.as_str()),
        ("entrypoint", manifest.entrypoint.as_str()),
        ("mutation_scope", manifest.mutation_scope.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(ManifestError::new(
                "missing_field",
                format!("Connector manifest {label} must be non-empty."),
            ));
        }
    }
    if !matches!(
        manifest.mutation_scope.as_str(),
        "read-only" | "locally-mutating" | "externally-mutating"
    ) {
        return Err(ManifestError::new(
            "invalid_mutation_scope",
            "Connector manifest mutation_scope is invalid.",
        ));
    }
    if manifest.platforms.is_empty() {
        return Err(ManifestError::new(
            "missing_platform",
            "Connector manifest must declare a supported platform or environment.",
        ));
    }
    if manifest.ports.is_empty() {
        return Err(ManifestError::new(
            "missing_port",
            "Connector manifest must declare at least one Port.",
        ));
    }
    let mut declarations = BTreeSet::new();
    for port in &manifest.ports {
        if port.id.trim().is_empty() || port.version.trim().is_empty() {
            return Err(ManifestError::new(
                "invalid_port",
                "Connector Port declarations require id and version.",
            ));
        }
        if !declarations.insert((port.id.clone(), port.version.clone())) {
            return Err(ManifestError::new(
                "duplicate_port",
                format!(
                    "Duplicate Connector Port declaration: {} {}",
                    port.id, port.version
                ),
            ));
        }
        match published_port(&port.id) {
            Some(published) if port.version == published.version => {}
            Some(published) => {
                return Err(ManifestError::new(
                    "unsupported_port_version",
                    format!(
                        "Connector Port {} declares version {}, but this SDK publishes version {}.",
                        port.id, port.version, published.version
                    ),
                ));
            }
            None => {
                let published_ids = PUBLISHED_PORTS
                    .iter()
                    .map(|contract| contract.id)
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(ManifestError::new(
                    "unknown_port",
                    format!(
                        "Unknown Connector Port: {}. Published Ports: {}.",
                        port.id, published_ids
                    ),
                ));
            }
        }
    }
    for limitation in &manifest.known_limitations {
        if limitation.trim().is_empty() {
            return Err(ManifestError::new(
                "invalid_known_limitation",
                "Connector manifest known limitations must be non-empty.",
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorContext {
    pub platform: String,
}

impl ConnectorContext {
    pub fn current() -> Self {
        Self {
            platform: std::env::consts::OS.to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityProbe {
    pub available: bool,
    pub reason: Option<String>,
}

impl CapabilityProbe {
    pub fn available() -> Self {
        Self {
            available: true,
            reason: None,
        }
    }

    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            available: false,
            reason: Some(reason.into()),
        }
    }
}

pub trait Connector: Send + Sync {
    fn manifest(&self) -> &ConnectorManifest;
    fn probe(&self, port: &PortContract, context: &ConnectorContext) -> CapabilityProbe;
    fn work_discovery(&self) -> Option<&dyn WorkDiscovery> {
        None
    }
    fn automation(&self) -> Option<&dyn Automation> {
        None
    }
    fn native_open(&self) -> Option<&dyn NativeOpen> {
        None
    }
    fn native_reveal(&self) -> Option<&dyn NativeReveal> {
        None
    }
    fn tag_store(&self) -> Option<&dyn TagStore> {
        None
    }
    fn machine_inspector(&self) -> Option<&dyn MachineInspector> {
        None
    }
    fn package_manager(&self) -> Option<&dyn PackageManager> {
        None
    }
    fn configuration_manager(&self) -> Option<&dyn ConfigurationManager> {
        None
    }
    fn service_manager(&self) -> Option<&dyn ServiceManager> {
        None
    }
    fn synchronizer(&self) -> Option<&dyn Synchronizer> {
        None
    }
    fn source_history(&self) -> Option<&dyn SourceHistory> {
        None
    }
    fn git_state(&self) -> Option<&dyn crate::git_state::GitState> {
        None
    }
    fn user_notification(&self) -> Option<&dyn UserNotification> {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConnectorSummary {
    pub id: String,
    pub version: String,
}

impl ConnectorSummary {
    pub fn from_connector(connector: &dyn Connector) -> Self {
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
    pub fn register<C: Connector + 'static>(
        &mut self,
        connector: C,
    ) -> Result<&mut Self, ManifestError> {
        validate_connector_manifest(connector.manifest())?;
        if self
            .connectors
            .iter()
            .any(|existing| existing.manifest().id == connector.manifest().id)
        {
            return Err(ManifestError::new(
                "duplicate_connector",
                format!("Connector already registered: {}", connector.manifest().id),
            ));
        }
        self.connectors.push(Box::new(connector));
        Ok(self)
    }

    pub fn resolve<'a>(
        &'a self,
        port: &PortContract,
        context: &ConnectorContext,
    ) -> ConnectorResolution<'a> {
        let mut eligible = Vec::<&dyn Connector>::new();
        let mut ineligible = Vec::new();

        for connector in &self.connectors {
            let connector = connector.as_ref();
            let summary = ConnectorSummary::from_connector(connector);
            let manifest = connector.manifest();
            let port_match = manifest
                .ports
                .iter()
                .any(|candidate| candidate.id == port.id && candidate.version == port.version);
            if !port_match {
                ineligible.push(IneligibleConnector {
                    connector: summary,
                    reason: format!("does not declare compatible {} {}", port.id, port.version),
                });
                continue;
            }
            if !manifest
                .platforms
                .iter()
                .any(|platform| platform == "*" || platform == &context.platform)
            {
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
                    reason: probe
                        .reason
                        .unwrap_or_else(|| "capability probe reported unavailable".to_owned()),
                });
                continue;
            }
            eligible.push(connector);
        }

        eligible.sort_by(|left, right| left.manifest().id.cmp(&right.manifest().id));
        ineligible.sort_by(|left, right| left.connector.id.cmp(&right.connector.id));
        let selected = eligible.first().copied();
        let eligible_summaries = eligible
            .iter()
            .map(|connector| ConnectorSummary::from_connector(*connector))
            .collect();
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

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_manifest() -> ConnectorManifest {
        ConnectorManifest {
            api_version: CONNECTOR_API_VERSION.to_owned(),
            id: "test.connector".to_owned(),
            version: "0.1.0".to_owned(),
            display_name: "Test connector".to_owned(),
            ports: vec![ConnectorPortDeclaration {
                id: WORK_DISCOVERY_PORT.id.to_owned(),
                version: WORK_DISCOVERY_PORT.version.to_owned(),
            }],
            platforms: vec!["*".to_owned()],
            entrypoint: "rust:test::Connector".to_owned(),
            runtime_requirements: Vec::new(),
            dependency_probes: Vec::new(),
            configuration_requirements: Vec::new(),
            mutation_scope: "read-only".to_owned(),
            known_limitations: Vec::new(),
        }
    }

    struct DeclaredConnector {
        manifest: ConnectorManifest,
    }

    impl Connector for DeclaredConnector {
        fn manifest(&self) -> &ConnectorManifest {
            &self.manifest
        }

        fn probe(&self, _port: &PortContract, _context: &ConnectorContext) -> CapabilityProbe {
            CapabilityProbe::available()
        }
    }

    #[test]
    fn correct_manifest_passes() {
        assert!(validate_connector_manifest(&valid_manifest()).is_ok());
    }

    #[test]
    fn manifest_with_known_limitations_passes() {
        let mut manifest = valid_manifest();
        manifest.known_limitations = vec!["manages formulae and casks only".to_owned()];
        assert!(validate_connector_manifest(&manifest).is_ok());
    }

    #[test]
    fn unknown_port_id_is_refused() {
        let mut manifest = valid_manifest();
        manifest.ports[0].id = "NotAPublishedPort".to_owned();
        let error = validate_connector_manifest(&manifest).unwrap_err();
        assert_eq!(error.code, "unknown_port");
        assert!(error.message.contains("NotAPublishedPort"));
        assert!(error.message.contains(WORK_DISCOVERY_PORT.id));
    }

    #[test]
    fn wrong_port_version_is_refused() {
        let mut manifest = valid_manifest();
        manifest.ports[0].version = "2.0.0".to_owned();
        let error = validate_connector_manifest(&manifest).unwrap_err();
        assert_eq!(error.code, "unsupported_port_version");
        assert!(error.message.contains(WORK_DISCOVERY_PORT.id));
        assert!(error.message.contains("2.0.0"));
        assert!(error.message.contains(WORK_DISCOVERY_PORT.version));
    }

    #[test]
    fn empty_known_limitation_entry_is_refused() {
        for entry in ["", "   \t"] {
            let mut manifest = valid_manifest();
            manifest.known_limitations = vec![entry.to_owned()];
            let error = validate_connector_manifest(&manifest).unwrap_err();
            assert_eq!(error.code, "invalid_known_limitation");
        }
    }

    #[test]
    fn published_ports_have_unique_ids_and_carry_the_separate_module_ports() {
        let ids: BTreeSet<&str> = PUBLISHED_PORTS.iter().map(|contract| contract.id).collect();
        assert_eq!(ids.len(), PUBLISHED_PORTS.len());
        for port_id in [
            "Automation",
            "GitState",
            "SourceHistory",
            "UserNotification",
        ] {
            assert!(
                published_port(port_id).is_some(),
                "published registry must carry {port_id}"
            );
        }
        assert_eq!(
            published_port("PackageManager").map(|contract| contract.version),
            Some(PACKAGE_MANAGER_PORT.version)
        );
        assert!(published_port("NotAPublishedPort").is_none());
    }

    #[test]
    fn registry_register_refuses_wrong_port_version_up_front() {
        let mut manifest = valid_manifest();
        manifest.ports[0].version = "2.0.0".to_owned();
        let mut registry = ConnectorRegistry::default();
        let error = registry
            .register(DeclaredConnector { manifest })
            .err()
            .expect("registration must refuse a wrong Port version up front");
        assert_eq!(error.code, "unsupported_port_version");
    }
}
