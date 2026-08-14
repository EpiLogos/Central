use central_connector_sdk::{
    CapabilityProbe, Connector, ConnectorContext, ConnectorManifest, ConnectorPortDeclaration,
    PortContract, PortError, WorkDiscovery, WorkDiscoveryInput, WorkDiscoveryOutput,
    CONNECTOR_API_VERSION, WORK_DISCOVERY_PORT,
};

pub struct TemplateWorkConnector {
    manifest: ConnectorManifest,
}

impl TemplateWorkConnector {
    pub fn new() -> Self {
        Self {
            manifest: ConnectorManifest {
                api_version: CONNECTOR_API_VERSION.to_owned(),
                id: "template.work-discovery".to_owned(),
                version: "0.1.0".to_owned(),
                display_name: "WorkDiscovery Connector template".to_owned(),
                ports: vec![ConnectorPortDeclaration {
                    id: WORK_DISCOVERY_PORT.id.to_owned(),
                    version: WORK_DISCOVERY_PORT.version.to_owned(),
                }],
                platforms: vec!["*".to_owned()],
                entrypoint: "rust:central-connector-template::TemplateWorkConnector".to_owned(),
                runtime_requirements: Vec::new(),
                dependency_probes: Vec::new(),
                configuration_requirements: Vec::new(),
                mutation_scope: "read-only".to_owned(),
            },
        }
    }
}

impl Default for TemplateWorkConnector {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkDiscovery for TemplateWorkConnector {
    fn list(&self, _input: &WorkDiscoveryInput) -> Result<WorkDiscoveryOutput, PortError> {
        Ok(WorkDiscoveryOutput { items: Vec::new() })
    }
}

impl Connector for TemplateWorkConnector {
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

#[cfg(test)]
mod tests {
    use super::*;
    use central_connector_sdk::{run_work_discovery_conformance, WorkDiscoveryConformanceFixture};
    use std::path::PathBuf;

    #[test]
    fn template_compiles_against_and_passes_the_public_contract() {
        let report = run_work_discovery_conformance(
            &TemplateWorkConnector::new(),
            &WorkDiscoveryConformanceFixture {
                work_root: PathBuf::from("unused"),
                platform: std::env::consts::OS.to_owned(),
                expected_names: Some(Vec::new()),
            },
        ).unwrap();
        assert_eq!(report.port_id, WORK_DISCOVERY_PORT.id);
    }
}
